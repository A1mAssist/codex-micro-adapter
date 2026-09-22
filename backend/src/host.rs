//! The loop that both front ends share.
//!
//! `codex-micro-backend run` drives it from the console; the Tauri app drives it
//! from a worker thread and mirrors the events into the UI. Everything that is
//! not "how do I show this" lives here.

use crate::actions::{self, Bindings, Outcome, Performer};
use crate::control::Command;
use crate::device::{Candidate, Device, DeviceState, Event, LightingModel, Opener};
use crate::lighting::{AgentSlot, SlotStatus, VoiceState};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How often a disconnected host re-scans for the keyboard.
pub const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// How long a poll may park waiting for input.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(50);

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
        if !self.device.is_connected()
            && self
                .last_scan
                .map_or(true, |t| now.duration_since(t) >= SCAN_INTERVAL)
        {
            self.last_scan = Some(now);
            for event in self.device.connect(now, candidate) {
                events.push(HostEvent::Device(event));
            }
        }
        for event in self.device.poll(timeout) {
            events.push(self.dispatch(event));
        }
        for event in self.device.tick(now) {
            events.push(HostEvent::Device(event));
        }
        for event in &events {
            self.push_log(event.describe());
        }
        events
    }

    fn dispatch(&mut self, event: Event) -> HostEvent {
        match event {
            Event::Trigger(trigger) => {
                let outcome = actions::dispatch(&trigger, &self.bindings, self.performer.as_mut());
                HostEvent::Action(outcome)
            }
            other => HostEvent::Device(other),
        }
    }
}
