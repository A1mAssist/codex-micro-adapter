//! The loop that both front ends share.
//!
//! `codex-micro-backend run` drives it from the console; the Tauri app drives it
//! from a worker thread and mirrors the events into the UI. Everything that is
//! not "how do I show this" lives here.

use crate::actions::{self, Bindings, Outcome, Performer};
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

pub struct Host<O: Opener> {
    pub device: Device<O>,
    /// Where keystrokes go: the real Windows one, or the logging one for dry runs.
    performer: Box<dyn Performer>,
    bindings: Bindings,
    lighting: LightingModel,
    brightness_percent: u8,
    slots: Vec<AgentSlot>,
    fleet: Option<SlotStatus>,
    voice: VoiceState,
    selection: bool,
    last_scan: Option<Instant>,
    log: VecDeque<String>,
    encoder: EncoderHold,
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
            fleet: None,
            voice: VoiceState::Idle,
            selection: false,
            last_scan: None,
            log: VecDeque::new(),
            encoder: EncoderHold::default(),
        }
    }

    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub fn set_bindings(&mut self, bindings: Bindings) {
        self.bindings = bindings;
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
                self.device.set_slots(self.slots.clone());
                format!("ok agent {index} {}", self.slots[index].status.to_token())
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
        }
        // the app fires the hold 500ms in, without waiting for the release
        if self.encoder.long_due(now) {
            let trigger = long_press_trigger(self.device.layout());
            events.push(self.run(trigger));
        }
        for event in self.device.tick(now) {
            events.push(HostEvent::Device(event));
        }
        for event in &events {
            self.push_log(event.describe());
        }
        events
    }

    /// One trigger through the binding table.
    fn run(&mut self, trigger: Trigger) -> HostEvent {
        let outcome = actions::dispatch(&trigger, &self.bindings, self.performer.as_mut());
        HostEvent::Action(outcome)
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
}
