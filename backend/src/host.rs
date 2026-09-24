//! The loop that both front ends share.
//!
//! `codex-micro-backend run` drives it from the console; the Tauri app drives it
//! from a worker thread and mirrors the events into the UI. Everything that is
//! not "how do I show this" lives here.

use crate::actions::{self, Bindings, Combo, Outcome, Performer};
use crate::control::Command;
use crate::device::{Candidate, Device, DeviceState, Event, LightingModel, Opener};
use crate::layout::{self, Action, EncoderMode, Layout, Trigger};
use crate::lighting::{AgentSlot, SlotStatus, VoiceState};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How often a disconnected host re-scans for the keyboard.
pub const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// How long a poll may park waiting for input.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(50);
/// How long the knob has to be held before the press counts as a hold. The app
/// uses the same 500ms (`wn` in `codex-micro-bridge`).
pub const ENCODER_LONG_PRESS: Duration = Duration::from_millis(500);
/// How often a `hold:` key repeats while it is down.
///
/// ponytail: one fixed rate for every hold. A real keyboard waits 500ms then
/// repeats about 30x a second; what actually needs the repeat is a harness that
/// watches for it - Claude Code's push-to-talk gives up 600ms after the press
/// when no repeat arrives - so 10x a second is enough for both that and typing.
/// Split into delay + rate if a harness ever cares about the difference.
pub const HOLD_REPEAT: Duration = Duration::from_millis(100);

/// The key a `hold:` binding currently has down, and when it last repeated.
///
/// Kept out of `Host` so the press / repeat / release edges are testable without
/// a device: the repeat is what a harness watching for auto-repeat needs, and
/// getting it wrong is silent.
#[derive(Default)]
struct HoldState {
    down: Option<(Combo, Instant)>,
}

impl HoldState {
    /// Put a key down. Any previous hold is returned so the caller can release it
    /// first - two holds at once would leave the first one stuck.
    fn press(&mut self, combo: Combo, now: Instant) -> Option<Combo> {
        let previous = self.down.replace((combo, now)).map(|(combo, _)| combo);
        previous
    }

    /// The key to repeat right now, if a held key is due.
    fn repeat(&mut self, now: Instant) -> Option<Combo> {
        let (combo, last) = self.down.as_mut()?;
        if now.duration_since(*last) < HOLD_REPEAT {
            return None;
        }
        *last = now;
        Some(combo.clone())
    }

    /// Let `combo` up. Only a hold of that same key is released, so a release
    /// that arrives after the binding changed cannot drop someone else's key.
    fn release(&mut self, combo: &Combo) -> Option<Combo> {
        match &self.down {
            Some((held, _)) if held.vk == combo.vk && held.modifiers == combo.modifiers => {
                self.down.take().map(|(combo, _)| combo)
            }
            _ => None,
        }
    }

    /// Whatever is still down, for a device that vanished mid-hold.
    fn cancel(&mut self) -> Option<Combo> {
        self.down.take().map(|(combo, _)| combo)
    }
}

/// Drive one edge of a `hold:` binding: press it, or let it up.
///
/// Split out of `Host` so both edges can be tested against a recording performer
/// without a device - a key left down is the failure mode that matters here, and
/// it is silent on real hardware until something else starts auto-repeating.
fn apply_hold(
    state: &mut HoldState,
    performer: &mut dyn Performer,
    combo: Combo,
    down: bool,
) -> Outcome {
    if down {
        // two holds at once would leave the first key stuck down
        if let Some(previous) = state.press(combo.clone(), Instant::now()) {
            let _ = performer.key_up(&previous);
        }
        return match performer.key_down(&combo) {
            Ok(()) => Outcome::Hold { combo, down },
            Err(err) => Outcome::Failed(err),
        };
    }

    // only release what this binding put down
    let Some(released) = state.release(&combo) else {
        return Outcome::Ignored;
    };
    match performer.key_up(&released) {
        Ok(()) => Outcome::Sent(format!("release {}", released.label)),
        Err(err) => Outcome::Failed(err),
    }
}

/// Press state for the knob, so a hold can fire before the release arrives.
#[derive(Default)]
struct EncoderHold {
    pressed_at: Option<Instant>,
    long_fired: bool,
}

impl EncoderHold {
    fn press(&mut self, now: Instant) {
        self.pressed_at = Some(now);
        self.long_fired = false;
    }

    /// True exactly once, `ENCODER_LONG_PRESS` after the press.
    fn long_due(&mut self, now: Instant) -> bool {
        let Some(pressed_at) = self.pressed_at else {
            return false;
        };
        if self.long_fired || now.duration_since(pressed_at) < ENCODER_LONG_PRESS {
            return false;
        }
        self.long_fired = true;
        true
    }

    /// Release: `true` when it was short enough to count as a click.
    fn release(&mut self) -> bool {
        let click = self.pressed_at.take().is_some() && !self.long_fired;
        self.long_fired = false;
        click
    }
}

/// What the host produced this tick.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    Device(Event),
    Action(Outcome),
}

impl HostEvent {
    /// One line for a console or a log pane.
    pub fn describe(&self) -> String {
        match self {
            HostEvent::Device(Event::Trigger(trigger)) => format!("trigger {trigger:?}"),
            HostEvent::Device(Event::Connected) => "connected".to_string(),
            HostEvent::Device(Event::Disconnected) => "disconnected".to_string(),
            HostEvent::Device(Event::StateChanged(state)) => describe_state(state),
            HostEvent::Action(Outcome::Sent(what)) => format!("sent {what}"),
            HostEvent::Action(Outcome::Unbound(key)) => format!("unbound {key}"),
            HostEvent::Action(Outcome::Failed(err)) => format!("failed {err}"),
            HostEvent::Action(Outcome::Hold { combo, down }) => {
                format!("{} {}", if *down { "hold" } else { "release" }, combo.label)
            }
            // never reaches the log: the host drops it before pushing
            HostEvent::Action(Outcome::Ignored) => String::new(),
        }
    }
}

fn describe_state(state: &DeviceState) -> String {
    let mut line = state.status.to_token().to_string();
    if let Some(battery) = state.battery {
        line.push_str(&format!(" battery {}%", battery.percentage));
        if battery.is_charging == Some(true) {
            line.push_str(" charging");
        }
    }
    if let Some(error) = &state.error {
        line.push_str(&format!(" error {error}"));
    }
    line
}

/// What the desktop UI reads: everything it shows, in one value.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: String,
    pub transport: Option<String>,
    pub firmware: Option<String>,
    pub battery: Option<u32>,
    pub charging: Option<bool>,
    pub error: Option<String>,
    pub off_for_inactivity: bool,
    pub brightness_percent: u8,
    pub slots: Vec<AgentSlot>,
    pub log: Vec<String>,
}

/// Which session owns which agent key.
///
/// A harness reports an id it already has (a Claude Code `session_id`, a Codex
/// thread id, …) and the host answers with the key it took. Keys are handed out
/// lowest-first; when all six are taken the dullest owner loses its key, because
/// a harness that crashed will never release anything.
///
/// ponytail: steal-the-oldest policy, no per-session pinning — add pinning when
/// somebody actually wants a fixed key per project.
#[derive(Default)]
struct SessionSlots {
    owners: Vec<Option<String>>,
    touched: Vec<Instant>,
    /// The window that was in front while this session last reported activity,
    /// so tapping its agent key can bring the session back.
    windows: Vec<Option<isize>>,
}

impl SessionSlots {
    fn with_keys(count: usize) -> Self {
        Self {
            owners: vec![None; count],
            touched: vec![Instant::now(); count],
            windows: vec![None; count],
        }
    }

    /// Remember the window a session was reporting from.
    fn set_window(&mut self, index: usize, hwnd: isize) {
        if let Some(window) = self.windows.get_mut(index) {
            *window = Some(hwnd);
        }
    }

    /// The window this agent key should bring forward, when one was seen.
    fn window(&self, index: usize) -> Option<isize> {
        self.windows.get(index).copied().flatten()
    }

    /// Which session holds this key, if any.
    fn owner(&self, index: usize) -> Option<&str> {
        self.owners.get(index)?.as_deref()
    }

    /// The key `id` should use, claiming or stealing one if it has none.
    /// `slots` is only read, to work out which key is safest to take over.
    fn assign(&mut self, id: &str, slots: &[AgentSlot], now: Instant) -> usize {
        if let Some(index) = self.index_of(id) {
            self.touched[index] = now;
            return index;
        }
        let index = self
            .owners
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| self.victim(slots));
        self.owners[index] = Some(id.to_string());
        self.touched[index] = now;
        index
    }

    /// Give the key back. `None` when this session never had one.
    fn release(&mut self, id: &str) -> Option<usize> {
        let index = self.index_of(id)?;
        self.owners[index] = None;
        if let Some(window) = self.windows.get_mut(index) {
            *window = None;
        }
        Some(index)
    }

    /// A manual `agent <n> …` takes the key back from whichever session had it.
    fn clear(&mut self, index: usize) {
        if let Some(owner) = self.owners.get_mut(index) {
            *owner = None;
        }
        if let Some(window) = self.windows.get_mut(index) {
            *window = None;
        }
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.owners.iter().position(|owner| owner.as_deref() == Some(id))
    }

    /// The key to take over: the dullest status first (`off`, then idle, …),
    /// oldest first within the same status.
    fn victim(&self, slots: &[AgentSlot]) -> usize {
        (0..self.owners.len())
            .min_by_key(|&index| {
                let status = slots.get(index).map_or(SlotStatus::Off, |slot| slot.status);
                (interest(status), self.touched[index])
            })
            .unwrap_or(0)
    }
}

/// How much a status is worth keeping: a session working right now outranks one
/// that already finished, and a key showing nothing is worth nothing.
fn interest(status: SlotStatus) -> u8 {
    match status {
        SlotStatus::Off => 0,
        SlotStatus::Idle => 1,
        SlotStatus::Unread => 2,
        SlotStatus::AwaitingResponse => 3,
        SlotStatus::AwaitingApproval => 4,
        SlotStatus::Error => 5,
        SlotStatus::Working => 6,
    }
}

/// One agent-key tap as the host remembers it for a polling harness UI: the
/// session the tapped key belongs to, under a sequence that always grows so a
/// page can tell a fresh tap from the one it already followed.
fn tapped(current: Option<(u64, String)>, session: &str) -> (u64, String) {
    let seq = current.as_ref().map_or(1, |(seq, _)| seq + 1);
    (seq, session.to_string())
}

pub struct Host<O: Opener> {
    pub device: Device<O>,
    /// Where keystrokes go: the real Windows one, or the logging one for dry runs.
    performer: Box<dyn Performer>,
    bindings: Bindings,
    lighting: LightingModel,
    brightness_percent: u8,
    slots: Vec<AgentSlot>,
    /// Session-to-key bookkeeping, so `session <id> …` can pick a key itself.
    sessions: SessionSlots,
    /// The last agent key the user tapped, for a harness UI that can jump to a
    /// session: the dsh browser half polls this over `activation`. Shared with
    /// the control port, which answers polls while this loop is busy with USB.
    activation: crate::control::Activation,
    fleet: Option<SlotStatus>,
    voice: VoiceState,
    selection: bool,
    last_scan: Option<Instant>,
    log: VecDeque<String>,
    encoder: EncoderHold,
    /// A `hold:` binding currently down, if any.
    held: HoldState,
}

impl<O: Opener> Host<O> {
    pub fn new(
        device: Device<O>,
        bindings: Bindings,
        performer: Box<dyn Performer>,
        brightness_percent: u8,
        lighting: LightingModel,
    ) -> Self {
        Self {
            device,
            performer,
            bindings,
            lighting,
            brightness_percent,
            slots: crate::lighting::default_agent_slots(),
            sessions: SessionSlots::with_keys(usize::from(crate::lighting::AGENT_SLOT_COUNT)),
            activation: crate::control::Activation::default(),
            fleet: None,
            voice: VoiceState::Idle,
            selection: false,
            last_scan: None,
            log: VecDeque::new(),
            encoder: EncoderHold::default(),
            held: HoldState::default(),
        }
    }

    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub fn set_bindings(&mut self, bindings: Bindings) {
        self.bindings = bindings;
    }

    /// Hand the control port the same tap slot this host writes to, so a page
    /// can poll it while this loop is busy with USB.
    pub fn share_activation(&mut self, slot: crate::control::Activation) {
        self.activation = slot;
    }

    pub fn brightness_percent(&self) -> u8 {
        self.brightness_percent
    }

    /// The six agent keys as the host currently believes them to be.
    pub fn slots(&self) -> &[AgentSlot] {
        &self.slots
    }

    /// Recent activity, newest last, for a log pane.
    pub fn log(&self) -> impl DoubleEndedIterator<Item = &String> {
        self.log.iter()
    }

    fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > 200 {
            self.log.pop_front();
        }
    }

    /// Replace the lighting model (brightness and the auto-off timer).
    pub fn set_lighting(&mut self, lighting: LightingModel, brightness_percent: u8) {
        self.lighting = lighting;
        self.brightness_percent = brightness_percent;
        self.device.set_lighting(self.lighting.clone());
    }

    /// Everything the UI shows.
    pub fn snapshot(&self) -> Snapshot {
        let state = self.device.state();
        Snapshot {
            status: state.status.to_token().to_string(),
            transport: state.transport.map(|t| t.to_token().to_string()),
            firmware: state.firmware.clone(),
            battery: state.battery.map(|b| b.percentage),
            charging: state.battery.and_then(|b| b.is_charging),
            error: state.error.clone(),
            off_for_inactivity: self.device.off_for_inactivity,
            brightness_percent: self.brightness_percent,
            slots: self.slots.clone(),
            log: self.log.iter().cloned().collect(),
        }
    }

    /// Swap the keystroke sink: dry run <-> live.
    pub fn set_performer(&mut self, performer: Box<dyn Performer>) {
        self.performer = performer;
    }

    /// Apply one control command. Returns the reply sent back on the socket.
    pub fn apply(&mut self, command: Command) -> String {
        match command {
            Command::Ping => "ok".to_string(),
            Command::Agent { index, status } => {
                let Some(slot) = self.slots.get_mut(index) else {
                    return format!("err: agent index {index} out of range");
                };
                slot.status = status;
                // picking a key by hand wins it back from whichever session had it
                self.sessions.clear(index);
                self.device.set_slots(self.slots.clone());
                format!("ok agent {index} {}", self.slots[index].status.to_token())
            }
            Command::Session { id, status } => {
                let Some(status) = status else {
                    let Some(index) = self.sessions.release(&id) else {
                        return format!("ok session {id} held no key");
                    };
                    self.slots[index].status = SlotStatus::Off;
                    self.device.set_slots(self.slots.clone());
                    return format!("ok session {id} agent {index} off");
                };
                let index = self.sessions.assign(&id, &self.slots, Instant::now());
                let Some(slot) = self.slots.get_mut(index) else {
                    return format!("err: no agent key to give session {id}");
                };
                slot.status = status;
                // A session that is starting or working is the one the user is
                // typing in, so the window in front is almost certainly its
                // terminal. An unread/awaiting event can land while another app
                // is focused, so it must not overwrite what we already know.
                // ponytail: heuristic — let a plugin report a window handle if a
                // multiplexer or a single-window tab layout makes it wrong.
                if matches!(status, SlotStatus::Idle | SlotStatus::Working) {
                    if let Some(hwnd) = crate::performer::foreground_window() {
                        self.sessions.set_window(index, hwnd);
                    }
                }
                self.device.set_slots(self.slots.clone());
                format!("ok session {id} agent {index} {}", status.to_token())
            }
            Command::Fleet(status) => {
                self.fleet = status;
                self.device.set_snaking_ambient(status);
                "ok".to_string()
            }
            Command::Voice(voice) => {
                self.voice = voice;
                self.device.set_voice(voice);
                "ok".to_string()
            }
            Command::Brightness(percent) => {
                self.brightness_percent = percent;
                self.lighting.brightness = f32::from(percent) / 100.0;
                self.device.set_lighting(self.lighting.clone());
                "ok".to_string()
            }
            Command::Selection(visible) => {
                self.selection = visible;
                self.device.set_selection_lighting_visible(visible);
                "ok".to_string()
            }
            // answered from the shared slot the control port also reads
            Command::Activation => crate::control::activation_json(&self.activation),
        }
    }

    /// Whether the next [`Host::pump`] will actually look at a discovery result.
    /// Callers check this before enumerating USB, which is expensive and, most
    /// of the time, thrown away.
    pub fn scan_due(&self, now: Instant) -> bool {
        !self.device.is_connected()
            && self
                .last_scan
                .map_or(true, |t| now.duration_since(t) >= SCAN_INTERVAL)
    }

    /// Connect when due, drain input, push whatever lighting is due.
    ///
    /// `candidate` is only consulted while disconnected, so callers pass whatever
    /// their discovery found this round (or `None`).
    pub fn pump(
        &mut self,
        now: Instant,
        timeout: Duration,
        candidate: Option<Candidate>,
    ) -> Vec<HostEvent> {
        let mut events = Vec::new();
        if self.scan_due(now) {
            self.last_scan = Some(now);
            for event in self.device.connect(now, candidate) {
                events.push(HostEvent::Device(event));
            }
        }
        for event in self.device.poll(timeout) {
            self.dispatch(event, now, &mut events);
        }
        // a press whose release never arrives (unplugged mid-hold) must not fire
        if !self.device.is_connected() {
            self.encoder = EncoderHold::default();
            // and a held key has to come back up, or Windows keeps it down forever
            if let Some(combo) = self.held.cancel() {
                let _ = self.performer.key_up(&combo);
                self.push_log(format!("release {}", combo.label));
            }
        }
        // a held key repeats the way a real keyboard does; harnesses that watch
        // for the repeat (Claude Code's push-to-talk) need it to keep going
        if let Some(combo) = self.held.repeat(now) {
            if let Err(err) = self.performer.key_down(&combo) {
                self.push_log(format!("failed {err}"));
            }
        }
        // the app fires the hold 500ms in, without waiting for the release
        if self.encoder.long_due(now) {
            let trigger = long_press_trigger(self.device.layout());
            events.push(self.run(trigger));
        }
        for event in self.device.tick(now) {
            events.push(HostEvent::Device(event));
        }
        events.retain(|event| !matches!(event, HostEvent::Action(Outcome::Ignored)));
        for event in &events {
            self.push_log(event.describe());
        }
        events
    }

    /// One trigger through the binding table.
    ///
    /// A `hold:` binding is stateful - the host presses it, repeats it and
    /// releases it - so the outcome comes back here rather than being performed
    /// on the spot.
    fn run(&mut self, trigger: Trigger) -> HostEvent {
        match actions::dispatch(&trigger, &self.bindings, self.performer.as_mut()) {
            Outcome::Hold { combo, down } => {
                HostEvent::Action(apply_hold(&mut self.held, self.performer.as_mut(), combo, down))
            }
            other => HostEvent::Action(other),
        }
    }

    fn dispatch(&mut self, event: Event, now: Instant, out: &mut Vec<HostEvent>) {
        let Event::Trigger(trigger) = event else {
            out.push(HostEvent::Device(event));
            return;
        };
        match trigger {
            // the knob is the only control with a time-based gesture, so its press
            // state lives here and the click fires when a short release lands
            Trigger::EncoderPress => {
                self.encoder.press(now);
                out.push(self.run(Trigger::EncoderPress));
            }
            Trigger::EncoderRelease => {
                let click = self.encoder.release();
                out.push(self.run(Trigger::EncoderRelease));
                if click {
                    let trigger = click_trigger(self.device.layout());
                    out.push(self.run(trigger));
                }
            }
            // tapping an agent key brings its session's window forward; when no
            // window was seen the binding table says what the key does instead
            Trigger::AgentKey(index) => {
                let slot = usize::from(index);
                // A harness page can jump to its session, but only once it hears
                // about the tap: this is what the `activation` command answers.
                if let Some(session) = self.sessions.owner(slot).map(str::to_string) {
                    if let Ok(mut shared) = self.activation.lock() {
                        let next = tapped(shared.take(), &session);
                        *shared = Some(next);
                    }
                }
                match self
                    .sessions
                    .window(slot)
                    .map(crate::performer::focus_window)
                {
                    Some(Ok(())) => {
                        for (i, agent) in self.slots.iter_mut().enumerate() {
                            agent.selected = i == slot;
                        }
                        self.device.set_slots(self.slots.clone());
                        out.push(HostEvent::Action(Outcome::Sent(format!(
                            "focus the window of agent {slot}"
                        ))));
                    }
                    _ => out.push(self.run(Trigger::AgentKey(index))),
                }
            }
            other => out.push(self.run(other)),
        }
    }
}

/// What a short knob press means: the layout's own `click` action in custom mode.
/// The built-in modes decide that inside the app, so the harness gets the bare
/// gesture — bind `encoder:click` to say what it should do here.
fn click_trigger(layout: &Layout) -> Trigger {
    Trigger::EncoderClick(layout::encoder_action(layout, "click"))
}

/// What holding the knob means: custom mode fires its own `longPress` action,
/// every other mode opens the settings page — `c('/settings/codex-micro')` in
/// the app, which is the `settings` command here.
fn long_press_trigger(layout: &Layout) -> Trigger {
    match layout::encoder_action(layout, "longPress") {
        Some(action) => Trigger::EncoderLongPress(Some(action)),
        None if layout.encoder_mode == EncoderMode::Custom => Trigger::EncoderLongPress(None),
        None => Trigger::EncoderLongPress(Some(Action::Command("settings".to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoder_layout(mode: EncoderMode, gesture: &str, action: Action) -> Layout {
        let mut layout = Layout::default();
        layout.encoder_mode = mode;
        layout.encoder.insert(gesture.to_string(), action);
        layout
    }

    fn held(label: &str, vk: u16) -> Combo {
        Combo {
            modifiers: crate::actions::Modifiers::default(),
            vk,
            label: label.to_string(),
        }
    }

    #[test]
    fn a_hold_binding_drives_the_performer_end_to_end() {
        use crate::actions::tests::Recording;

        let mut state = HoldState::default();
        let mut performer = Recording::default();
        let space = held("space", 0x20);

        // press, then the repeats the host emits while it stays down, then release
        assert!(matches!(
            apply_hold(&mut state, &mut performer, space.clone(), true),
            Outcome::Hold { down: true, .. }
        ));
        // read the clock after the press: the press is what starts the interval
        let mut clock = Instant::now();
        for _ in 0..3 {
            clock += HOLD_REPEAT;
            if let Some(combo) = state.repeat(clock) {
                performer.key_down(&combo).unwrap();
            }
        }
        assert_eq!(
            apply_hold(&mut state, &mut performer, space.clone(), false),
            Outcome::Sent("release space".into())
        );

        assert_eq!(
            performer.held,
            vec![
                "down:space", // the press
                "down:space", // three repeats, which is what push-to-talk waits for
                "down:space",
                "down:space",
                "up:space", // and exactly one release
            ]
        );
    }

    #[test]
    fn releasing_a_key_that_is_not_held_does_nothing() {
        use crate::actions::tests::Recording;

        let mut state = HoldState::default();
        let mut performer = Recording::default();
        assert_eq!(
            apply_hold(&mut state, &mut performer, held("space", 0x20), false),
            Outcome::Ignored
        );
        assert!(performer.held.is_empty(), "nothing was pressed or released");
    }

    #[test]
    fn a_hold_presses_then_repeats_at_the_repeat_interval() {
        let start = Instant::now();
        let mut state = HoldState::default();
        assert!(state.press(held("space", 0x20), start).is_none());

        // too soon: a real keyboard does not repeat instantly
        assert!(state.repeat(start + Duration::from_millis(50)).is_none());
        assert_eq!(
            state.repeat(start + HOLD_REPEAT).map(|c| c.label),
            Some("space".to_string()),
            "the key repeats, which is what push-to-talk watches for"
        );
        assert!(state.repeat(start + HOLD_REPEAT).is_none(), "not twice at once");
        assert_eq!(
            state
                .repeat(start + HOLD_REPEAT + HOLD_REPEAT)
                .map(|c| c.label),
            Some("space".to_string())
        );
    }

    #[test]
    fn a_hold_releases_only_its_own_key() {
        let start = Instant::now();
        let mut state = HoldState::default();
        state.press(held("space", 0x20), start);

        // a release for a different key must not drop the held one
        assert!(state.release(&held("enter", 0x0D)).is_none());
        assert_eq!(state.cancel().map(|c| c.label), Some("space".to_string()));

        // and the right key releases once, not twice
        state.press(held("space", 0x20), start);
        assert_eq!(
            state.release(&held("space", 0x20)).map(|c| c.label),
            Some("space".to_string())
        );
        assert!(state.release(&held("space", 0x20)).is_none());
    }

    #[test]
    fn a_second_hold_releases_the_first() {
        let start = Instant::now();
        let mut state = HoldState::default();
        state.press(held("space", 0x20), start);
        assert_eq!(
            state
                .press(held("m", 0x4D), start + Duration::from_millis(10))
                .map(|c| c.label),
            Some("space".to_string()),
            "the superseded key comes back up instead of sticking"
        );
        assert_eq!(state.cancel().map(|c| c.label), Some("m".to_string()));
    }

    #[test]
    fn a_hold_fires_once_and_never_counts_as_a_click() {
        let start = Instant::now();
        let mut hold = EncoderHold::default();
        hold.press(start);
        assert!(!hold.long_due(start + Duration::from_millis(499)));
        assert!(hold.long_due(start + ENCODER_LONG_PRESS));
        assert!(!hold.long_due(start + Duration::from_secs(5)), "fires once");
        assert!(!hold.release(), "a hold is not a click");
        assert!(!hold.long_due(start + Duration::from_secs(6)));
    }

    #[test]
    fn a_release_before_the_threshold_is_a_click() {
        let start = Instant::now();
        let mut hold = EncoderHold::default();
        hold.press(start);
        assert!(!hold.long_due(start + Duration::from_millis(10)));
        assert!(hold.release());
        assert!(!hold.release(), "one click per press");
        assert!(!hold.long_due(start + Duration::from_secs(1)));
    }

    #[test]
    fn knob_gestures_follow_the_mode() {
        let custom_click = encoder_layout(
            EncoderMode::Custom,
            "click",
            Action::Command("archiveThread".into()),
        );
        assert_eq!(
            click_trigger(&custom_click),
            Trigger::EncoderClick(Some(Action::Command("archiveThread".into())))
        );

        let custom_hold = encoder_layout(
            EncoderMode::Custom,
            "longPress",
            Action::Command("newTask".into()),
        );
        assert_eq!(
            long_press_trigger(&custom_hold),
            Trigger::EncoderLongPress(Some(Action::Command("newTask".into())))
        );

        // custom mode without a long press stays a bare gesture for the harness
        let bare = Layout {
            encoder_mode: EncoderMode::Custom,
            ..Layout::default()
        };
        assert_eq!(long_press_trigger(&bare), Trigger::EncoderLongPress(None));

        // every built-in mode opens the settings page on a hold
        for mode in [
            EncoderMode::ComposerNavigation,
            EncoderMode::Reasoning,
            EncoderMode::ConversationScroll,
        ] {
            let layout = Layout {
                encoder_mode: mode,
                ..Layout::default()
            };
            assert_eq!(click_trigger(&layout), Trigger::EncoderClick(None));
            assert_eq!(
                long_press_trigger(&layout),
                Trigger::EncoderLongPress(Some(Action::Command("settings".into())))
            );
        }
    }

    #[test]
    fn a_session_keeps_its_key_until_it_ends() {
        let now = Instant::now();
        let slots = crate::lighting::default_agent_slots();
        let mut table = SessionSlots::with_keys(6);
        assert_eq!(table.assign("a", &slots, now), 0);
        assert_eq!(table.assign("b", &slots, now), 1);
        assert_eq!(table.assign("a", &slots, now), 0, "same session, same key");
        assert_eq!(table.owner(0), Some("a"));
        assert_eq!(table.release("a"), Some(0));
        assert_eq!(table.release("a"), None, "a key is only given back once");
        assert_eq!(table.assign("c", &slots, now), 0, "the freed key is reused");
    }

    #[test]
    fn all_six_taken_the_dullest_key_changes_hands() {
        let now = Instant::now();
        let mut slots = crate::lighting::default_agent_slots();
        let mut table = SessionSlots::with_keys(6);
        for (step, id) in ["a", "b", "c", "d", "e", "f"].into_iter().enumerate() {
            let index = table.assign(id, &slots, now + Duration::from_secs(step as u64));
            slots[index].status = SlotStatus::Working;
        }
        slots[3].status = SlotStatus::Idle; // went quiet a while ago
        slots[2].status = SlotStatus::Unread; // finished, still waiting to be read
        let index = table.assign("g", &slots, now + Duration::from_secs(60));
        assert_eq!(index, 3, "an idle key changes hands before an unread one");
        assert_eq!(table.owner(3), Some("g"));
    }

    #[test]
    fn a_tap_remembers_which_session_to_open() {
        assert_eq!(tapped(None, "a"), (1, "a".to_string()));
        assert_eq!(
            tapped(Some((1, "a".to_string())), "b"),
            (2, "b".to_string()),
            "a polling page follows the highest sequence it has seen, so a \
             repeated tap on the same key still counts as news"
        );
    }

    #[test]
    fn an_agent_key_remembers_the_window_its_session_came_from() {
        let now = Instant::now();
        let slots = crate::lighting::default_agent_slots();
        let mut table = SessionSlots::with_keys(6);
        let index = table.assign("a", &slots, now);
        assert_eq!(table.window(index), None, "nothing seen yet");
        table.set_window(index, 0x1234);
        assert_eq!(table.window(index), Some(0x1234));
        table.clear(index);
        assert_eq!(
            table.window(index),
            None,
            "a manual agent command forgets it"
        );

        let index = table.assign("b", &slots, now);
        table.set_window(index, 0x5678);
        assert_eq!(table.release("b"), Some(index));
        assert_eq!(table.window(index), None, "a released key has no window");
    }

    #[test]
    fn a_manual_agent_command_takes_the_key_back() {
        let now = Instant::now();
        let slots = crate::lighting::default_agent_slots();
        let mut table = SessionSlots::with_keys(6);
        let index = table.assign("a", &slots, now);
        table.clear(index);
        assert_eq!(table.owner(index), None);
        assert_eq!(
            table.assign("b", &slots, now),
            index,
            "the freed key goes to the next session"
        );
    }
}
