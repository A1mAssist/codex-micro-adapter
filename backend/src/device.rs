//! Device lifecycle: discovery, connection, lighting, battery, inactivity.
//!
//! Ported from the host-side `CodexMicroService` (`service-C6nm9ayu.js`). Timing
//! constants keep their upstream names so the vendor source stays greppable:
//!
//! ```text
//! N = [1s, 2s, 5s, 10s]   transport reconnect backoff
//! P = [250ms, 1s, 3s]     HID topology settle retry
//! L = 15s                 service-level RPC timeout (on top of the transport's 10s)
//! R = 10s                 how long `stop()` waits for in-flight device RPCs
//! H = 0.05                joystick dead zone
//! ```
//!
//! Deliberately not ported: the per-thread accent derives (`se` / `$`) and the
//! agent-key status palette. Those paint ChatGPT threads; a daemon driving
//! another harness has no thread list to show.
//!
//! The machine is poll-driven — `poll()` reports input, `tick(now)` performs what
//! is due — so it needs no async runtime and is testable without hardware.

use crate::layout::{self, Layout, Trigger};
use crate::lighting::{self, AgentSlot, SlotStatus, VoiceState};
use crate::oai::{self, HidEvent, JoystickEvent};
use crate::rpc::{Hid, RpcClient, RpcError};
use std::time::{Duration, Instant};

/// `N` — transport reconnect backoff.
pub const RECONNECT_DELAYS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];
/// How often we re-read `device.status` while connected.
pub const BATTERY_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotDetected,
    Detected,
    Connected,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Bluetooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percentage: u32,
    pub is_charging: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceState {
    pub status: Status,
    pub transport: Option<Transport>,
    /// `sys.version` result, e.g. `0.1.37-ai-micro-idf-nimble`.
    pub firmware: Option<String>,
    pub error: Option<String>,
    pub battery: Option<Battery>,
}

impl Default for Status {
    fn default() -> Self {
        Status::NotDetected
    }
}

impl Status {
    /// The name the console and the UI both use.
    pub fn to_token(self) -> &'static str {
        match self {
            Status::NotDetected => "not-detected",
            Status::Detected => "detected",
            Status::Connected => "connected",
            Status::Error => "error",
        }
    }
}

impl Transport {
    pub fn to_token(self) -> &'static str {
        match self {
            Transport::Usb => "usb",
            Transport::Bluetooth => "bluetooth",
        }
    }
}

/// Brightness and auto-off, set by the host.
///
/// What is actually *shown* is derived from the agent slots by
/// [`crate::lighting`] — the same `se` / `$` split the vendor service uses — so
/// there is nothing to configure here beyond brightness and the sleep timer.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingModel {
    /// 0..=1, from `codex-micro-lighting-brightness` (percent / 100).
    pub brightness: f32,
    /// `codex-micro-lighting-auto-off`; `None` disables the auto-off.
    pub inactivity_timeout: Option<Duration>,
}

impl Default for LightingModel {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            inactivity_timeout: None,
        }
    }
}

/// Something the host should act on.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A key / encoder / stick resolved to an action through the layout.
    Trigger(Trigger),
    Connected,
    Disconnected,
    StateChanged(DeviceState),
}

/// A candidate device as reported by discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    pub is_usb: bool,
}

/// Opens the transport for a candidate path.
pub trait Opener {
    type Hid: Hid;
    fn open(&mut self, path: &str) -> Result<Self::Hid, String>;
}

pub struct Device<O: Opener> {
    opener: O,
    client: Option<RpcClient<O::Hid>>,
    state: DeviceState,
    layout: Layout,
    lighting: LightingModel,
    /// JSON of the last `v.oai.rgbcfg` pushed (the vendor dedupes the same way).
    applied_config_key: Option<String>,
    /// JSON of the last `v.oai.thstatus` pushed.
    applied_threads_key: Option<String>,
    /// Six agent keys; the host decides what they show.
    slots: Vec<AgentSlot>,
    /// Push-to-talk / dictation state.
    voice: VoiceState,
    /// The selection highlight is on screen, so the keys echo the ambient colour.
    selection_lighting_visible: bool,
    /// A fleet-wide status that takes over the ring (`snakingAmbientStatus`).
    snaking_ambient: Option<SlotStatus>,
    /// Public so the UI can show "lights off" (the vendor keeps it private).
    pub off_for_inactivity: bool,
    reconnect_attempt: usize,
    next_reconnect_at: Option<Instant>,
    inactivity_at: Option<Instant>,
    next_battery_refresh_at: Option<Instant>,
}

impl<O: Opener> Device<O> {
    pub fn new(opener: O, layout: Layout, lighting: LightingModel) -> Self {
        Self {
            opener,
            client: None,
            state: DeviceState::default(),
            layout,
            lighting,
            applied_config_key: None,
            applied_threads_key: None,
            slots: lighting::default_agent_slots(),
            voice: VoiceState::Idle,
            selection_lighting_visible: false,
            snaking_ambient: None,
            off_for_inactivity: false,
            reconnect_attempt: 0,
            next_reconnect_at: None,
            inactivity_at: None,
            next_battery_refresh_at: None,
        }
    }

    pub fn state(&self) -> &DeviceState {
        &self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state.status == Status::Connected
    }

    /// The layout the host resolves key events through.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// What the six agent keys should show.
    pub fn set_slots(&mut self, slots: Vec<AgentSlot>) {
        self.slots = slots;
    }

    /// Push-to-talk / dictation state; drives the ambient ring.
    pub fn set_voice(&mut self, voice: VoiceState) {
        self.voice = voice;
    }

    /// Echo the ambient colour onto the keys while the selection highlight is up.
    pub fn set_selection_lighting_visible(&mut self, visible: bool) {
        self.selection_lighting_visible = visible;
    }

    /// Show a single status across the whole ring, ignoring per-slot lighting.
    pub fn set_snaking_ambient(&mut self, status: Option<SlotStatus>) {
        self.snaking_ambient = status;
    }

    /// Set the lighting model; pushes it on the next tick if it changed.
    pub fn set_lighting(&mut self, lighting: LightingModel) {
        self.lighting = lighting;
        self.off_for_inactivity = false;
        self.schedule_inactivity();
    }

    /// Bring up a device if one is available and the backoff has elapsed.
    pub fn connect(&mut self, now: Instant, candidate: Option<Candidate>) -> Vec<Event> {
        if self.client.is_some() {
            return Vec::new();
        }
        if let Some(at) = self.next_reconnect_at {
            if now < at {
                return Vec::new();
            }
        }
        let Some(candidate) = candidate else {
            self.transition(Status::NotDetected, None, None, None, None);
            return vec![Event::StateChanged(self.state.clone())];
        };

        match self.opener.open(&candidate.path) {
            Ok(hid) => {
                let mut client = RpcClient::new(hid);
                let events = self.handshake(&mut client, &candidate, now);
                self.client = Some(client);
                events
            }
            Err(err) => {
                self.transition(Status::Error, None, None, Some(err), None);
                self.schedule_reconnect(now);
                vec![Event::StateChanged(self.state.clone())]
            }
        }
    }

    fn handshake(
        &mut self,
        client: &mut RpcClient<O::Hid>,
        candidate: &Candidate,
        now: Instant,
    ) -> Vec<Event> {
        let transport = if candidate.is_usb {
            Transport::Usb
        } else {
            Transport::Bluetooth
        };
        match client.call(oai::METHOD_SYS_VERSION, serde_json::Value::Null) {
            Ok(value) => {
                let firmware = version_of(&value);
                self.reconnect_attempt = 0;
                self.next_reconnect_at = None;
                self.applied_config_key = None;
                self.applied_threads_key = None;
                self.off_for_inactivity = false;
                self.transition(Status::Connected, Some(transport), firmware, None, None);
                self.next_battery_refresh_at = Some(now);
                self.schedule_inactivity();
                vec![Event::Connected, Event::StateChanged(self.state.clone())]
            }
            Err(err) => {
                self.transition(
                    Status::Error,
                    Some(transport),
                    None,
                    Some(err.to_string()),
                    None,
                );
                self.schedule_reconnect(now);
                vec![Event::StateChanged(self.state.clone())]
            }
        }
    }

    fn schedule_reconnect(&mut self, now: Instant) {
        let delay = RECONNECT_DELAYS[self.reconnect_attempt.min(RECONNECT_DELAYS.len() - 1)];
        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
        self.next_reconnect_at = Some(now + delay);
    }

    pub fn disconnect(&mut self) -> Vec<Event> {
        self.client = None;
        self.applied_config_key = None;
        self.applied_threads_key = None;
        self.inactivity_at = None;
        self.next_battery_refresh_at = None;
        self.transition(Status::NotDetected, None, None, None, None);
        vec![Event::Disconnected, Event::StateChanged(self.state.clone())]
    }

    fn transition(
        &mut self,
        status: Status,
        transport: Option<Transport>,
        firmware: Option<String>,
        error: Option<String>,
        battery: Option<Battery>,
    ) {
        self.state = DeviceState {
            status,
            transport,
            firmware,
            error,
            battery,
        };
    }

    fn schedule_inactivity(&mut self) {
        // `handleLightingActivity` in the vendor source: any input resets the clock.
        self.inactivity_at = match (self.lighting.inactivity_timeout, self.is_connected()) {
            (Some(timeout), true) => Some(Instant::now() + timeout),
            _ => None,
        };
    }

    /// Drain device notifications and turn input into layout triggers.
    pub fn poll(&mut self, timeout: Duration) -> Vec<Event> {
        // unplugged: the reader thread stopped, so polling would just time out
        // forever and the UI would keep showing the last battery reading
        if self.client.as_ref().is_some_and(|c| c.is_closed()) {
            return self.fail(RpcError::Transport("device removed".into()));
        }
        let Some(client) = self.client.as_mut() else {
            return Vec::new();
        };
        client.pump(timeout);
        let mut events = Vec::new();
        for note in client.drain_notifications() {
            match note.method.as_str() {
                oai::NOTIFY_HID => match serde_json::from_value::<HidEvent>(note.params) {
                    Ok(hid) => {
                        self.schedule_inactivity();
                        self.off_for_inactivity = false;
                        if let Some(trigger) = layout::resolve_event(&hid, &self.layout) {
                            events.push(Event::Trigger(trigger));
                        }
                    }
                    Err(err) => events.push(Event::StateChanged(DeviceState {
                        error: Some(format!("bad hid notification: {err}")),
                        ..self.state.clone()
                    })),
                },
                oai::NOTIFY_JOYSTICK => {
                    if let Ok(stick) = serde_json::from_value::<JoystickEvent>(note.params) {
                        self.schedule_inactivity();
                        if let Some(trigger) =
                            layout::resolve_stick(stick.angle, stick.distance, &self.layout)
                        {
                            events.push(Event::Trigger(trigger));
                        }
                    }
                }
                _ => {}
            }
        }
        events
    }

    /// Push lighting / refresh battery / go dark when due.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let mut events = Vec::new();
        if !self.is_connected() {
            return events;
        }
        if let Some(at) = self.inactivity_at {
            if now >= at {
                self.inactivity_at = None;
                self.off_for_inactivity = true;
            }
        }
        if let Some(at) = self.next_battery_refresh_at {
            if now >= at {
                self.next_battery_refresh_at = Some(now + BATTERY_REFRESH_INTERVAL);
                events.extend(self.refresh_battery());
            }
        }
        // covers both the normal push and the one-off dark payload
        events.extend(self.apply_lighting());
        events
    }

    fn refresh_battery(&mut self) -> Vec<Event> {
        let Some(client) = self.client.as_mut() else {
            return Vec::new();
        };
        match client.call(oai::METHOD_DEVICE_STATUS, serde_json::Value::Null) {
            Ok(value) => {
                let battery =
                    value
                        .get("batteryPercentage")
                        .and_then(|v| v.as_u64())
                        .map(|percentage| Battery {
                            percentage: percentage as u32,
                            is_charging: value.get("isCharging").and_then(|v| v.as_bool()),
                        });
                let firmware = self.state.firmware.clone();
                let transport = self.state.transport;
                self.transition(Status::Connected, transport, firmware, None, battery);
                vec![Event::StateChanged(self.state.clone())]
            }
            Err(err) => {
                if is_transport_fatal(&err) {
                    return self.fail(err);
                }
                // a rejected status read is not fatal; drop the stale battery
                let firmware = self.state.firmware.clone();
                let transport = self.state.transport;
                self.transition(
                    Status::Connected,
                    transport,
                    firmware,
                    self.state.error.clone(),
                    None,
                );
                vec![Event::StateChanged(self.state.clone())]
            }
        }
    }

    /// Push both lighting channels, deduping each payload exactly like the
    /// vendor's `appliedLightingConfigKey` / `appliedThreadLightingKey`.
    ///
    /// While the device is dark for inactivity both go out with zero brightness.
    fn apply_lighting(&mut self) -> Vec<Event> {
        let brightness = if self.off_for_inactivity {
            0.0
        } else {
            self.lighting.brightness
        };
        let mut events = Vec::new();

        let config = lighting::rgb_config(
            &self.slots,
            self.voice,
            self.selection_lighting_visible,
            brightness,
            self.snaking_ambient,
        );
        events.extend(self.write_if_changed(oai::METHOD_RGB_CONFIG, &config, true));

        let threads = lighting::thread_lighting(&self.slots, brightness);
        events.extend(self.write_if_changed(oai::METHOD_THREADS_LIGHTING, &threads, false));
        events
    }

    /// Send `payload` unless the identical JSON was already sent, and remember the
    /// last payload per channel.
    fn write_if_changed<T: serde::Serialize>(
        &mut self,
        method: &str,
        payload: &T,
        is_config: bool,
    ) -> Vec<Event> {
        let key = serde_json::to_string(payload).unwrap_or_default();
        let applied = if is_config {
            &self.applied_config_key
        } else {
            &self.applied_threads_key
        };
        if applied.as_deref() == Some(key.as_str()) {
            return Vec::new();
        }
        let Some(client) = self.client.as_mut() else {
            return Vec::new();
        };
        match client.call(method, payload) {
            Ok(_) => {
                *if is_config {
                    &mut self.applied_config_key
                } else {
                    &mut self.applied_threads_key
                } = Some(key);
                Vec::new()
            }
            Err(err) => self.fail_or_report(err),
        }
    }
    /// A transport-fatal error tears the connection down so we reconnect.
    fn fail_or_report(&mut self, err: RpcError) -> Vec<Event> {
        if is_transport_fatal(&err) {
            self.fail(err)
        } else {
            let state = DeviceState {
                error: Some(err.to_string()),
                ..self.state.clone()
            };
            vec![Event::StateChanged(state)]
        }
    }

    fn fail(&mut self, err: RpcError) -> Vec<Event> {
        self.client = None;
        self.applied_config_key = None;
        self.applied_threads_key = None;
        let now = Instant::now();
        let transport = self.state.transport;
        let firmware = self.state.firmware.clone();
        self.transition(
            Status::Error,
            transport,
            firmware,
            Some(err.to_string()),
            None,
        );
        self.schedule_reconnect(now);
        vec![Event::Disconnected, Event::StateChanged(self.state.clone())]
    }
}

/// The vendor's `Q()` / `Z()` predicates: the link is gone, not the call rejected.
pub fn is_transport_fatal(err: &RpcError) -> bool {
    matches!(err, RpcError::Timeout | RpcError::Transport(_))
}

/// `sys.version` answers `{"version": "..."}` on some firmware, a bare string on others.
fn version_of(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        other => other
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{encode, CHANNEL_RPC, REPORT_LEN};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct MockHid {
        incoming: Arc<Mutex<VecDeque<[u8; REPORT_LEN]>>>,
        written: Arc<Mutex<Vec<String>>>,
        /// reassembling buffer: a short report marks the end of a message
        pending: String,
        /// what the real transport flips when the reader thread stops
        closed: Arc<AtomicBool>,
    }

    impl Hid for MockHid {
        fn write_report(&mut self, report: &[u8; REPORT_LEN]) -> std::io::Result<()> {
            let len = report[2] as usize;
            self.pending
                .push_str(&String::from_utf8_lossy(&report[3..3 + len]));
            if len < crate::framing::MAX_CHUNK {
                let message = std::mem::take(&mut self.pending);
                self.written.lock().unwrap().push(message);
            }
            Ok(())
        }
        fn read_report(&mut self, _timeout: Duration) -> Option<[u8; REPORT_LEN]> {
            self.incoming.lock().unwrap().pop_front()
        }
        fn is_closed(&self) -> bool {
            self.closed.load(Ordering::SeqCst)
        }
    }

    struct MockOpener {
        incoming: Arc<Mutex<VecDeque<[u8; REPORT_LEN]>>>,
        written: Arc<Mutex<Vec<String>>>,
        closed: Arc<AtomicBool>,
        fail: bool,
    }

    impl Opener for MockOpener {
        type Hid = MockHid;
        fn open(&mut self, _path: &str) -> Result<Self::Hid, String> {
            if self.fail {
                return Err("no device".into());
            }
            Ok(MockHid {
                incoming: self.incoming.clone(),
                written: self.written.clone(),
                pending: String::new(),
                closed: self.closed.clone(),
            })
        }
    }

    struct Rig {
        device: Device<MockOpener>,
        written: Arc<Mutex<Vec<String>>>,
        closed: Arc<AtomicBool>,
    }

    impl Rig {
        fn methods(&self) -> Vec<String> {
            self.written
                .lock()
                .unwrap()
                .iter()
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .filter_map(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_string))
                .collect()
        }
        fn writes(&self) -> usize {
            self.written.lock().unwrap().len()
        }
        /// Last payload sent for a given method.
        fn last_for(&self, method: &str) -> serde_json::Value {
            let w = self.written.lock().unwrap();
            w.iter()
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .filter(|v| v.get("method").and_then(|m| m.as_str()) == Some(method))
                .last()
                .expect("no write for method")
        }
    }

    fn rig(lines: &[&str], layout: Layout) -> Rig {
        let incoming: VecDeque<_> = lines
            .iter()
            .flat_map(|l| encode(CHANNEL_RPC, l.as_bytes()))
            .collect();
        let incoming = Arc::new(Mutex::new(incoming));
        let written = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let opener = MockOpener {
            incoming,
            written: written.clone(),
            closed: closed.clone(),
            fail: false,
        };
        Rig {
            device: Device::new(opener, layout, LightingModel::default()),
            written,
            closed,
        }
    }

    fn failing_rig() -> Device<MockOpener> {
        let opener = MockOpener {
            incoming: Arc::new(Mutex::new(VecDeque::new())),
            written: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(AtomicBool::new(false)),
            fail: true,
        };
        Device::new(opener, Layout::default(), LightingModel::default())
    }

    fn candidate() -> Candidate {
        Candidate {
            path: "mock".into(),
            is_usb: true,
        }
    }

    fn layout_with_codex_on_act06() -> Layout {
        let mut layout = Layout::default();
        layout.slots.insert(
            "ACT06".into(),
            layout::SlotConfig {
                keycap_id: "CODEX".into(),
                ..Default::default()
            },
        );
        layout
    }

    #[test]
    fn connect_sends_sys_version_and_reports_connected() {
        let mut r = rig(
            &["{\"result\":{\"version\":\"0.1.37\"},\"id\":1}\n"],
            Layout::default(),
        );
        let events = r.device.connect(Instant::now(), Some(candidate()));
        assert_eq!(r.methods(), vec!["sys.version"]);
        assert!(r.device.is_connected());
        assert!(events.contains(&Event::Connected));
        assert_eq!(r.device.state().transport, Some(Transport::Usb));
        assert_eq!(r.device.state().firmware.as_deref(), Some("0.1.37"));
    }

    #[test]
    fn an_unplugged_transport_tears_the_session_down() {
        let mut r = rig(
            &["{\"result\":{\"version\":\"0.1.37\"},\"id\":1}\n"],
            Layout::default(),
        );
        r.device.connect(Instant::now(), Some(candidate()));
        assert!(r.device.is_connected());

        // the reader thread sets this the moment ReadFile stops answering, so the
        // host learns about the unplug instead of timing out over the next minute
        r.closed.store(true, Ordering::SeqCst);
        let events = r.device.poll(Duration::ZERO);
        assert_eq!(events[0], Event::Disconnected);
        assert!(!r.device.is_connected(), "and it reconnects on the next scan");
        assert_eq!(r.device.state().status, Status::Error);
    }

    #[test]
    fn failed_open_backs_off_before_retrying() {
        let mut device = failing_rig();
        let t0 = Instant::now();
        device.connect(t0, Some(candidate()));
        assert_eq!(device.state().status, Status::Error);
        assert_eq!(device.reconnect_attempt, 1, "one backoff step consumed");
        // first backoff is 1s, so an immediate retry is swallowed
        device.connect(t0 + Duration::from_millis(500), Some(candidate()));
        assert_eq!(
            device.reconnect_attempt, 1,
            "retry before the deadline is ignored"
        );
        // after the deadline it retries and advances the backoff
        device.connect(t0 + Duration::from_millis(1_100), Some(candidate()));
        assert_eq!(device.reconnect_attempt, 2);
    }

    #[test]
    fn hid_notification_becomes_a_trigger() {
        let mut r = rig(
            &[
                "{\"result\":{\"version\":\"1\"},\"id\":1}\n",
                "{\"method\":\"v.oai.hid\",\"params\":{\"k\":\"ACT06\",\"act\":1}}\n",
            ],
            layout_with_codex_on_act06(),
        );
        r.device.connect(Instant::now(), Some(candidate()));
        let events = r.device.poll(Duration::ZERO);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Event::Trigger(Trigger::Act(layout::Action::Command(c))) if c == "composer.submit"
        ));
    }

    #[test]
    fn joystick_notification_respects_dead_zone() {
        let mut layout = Layout::default();
        layout.analog_stick.insert(
            "up".into(),
            layout::Action::Command("composer.increaseReasoningEffort".into()),
        );
        let mut r = rig(
            &[
                "{\"result\":{\"version\":\"1\"},\"id\":1}\n",
                "{\"method\":\"v.oai.rad\",\"params\":{\"a\":0,\"d\":0.01}}\n",
                "{\"method\":\"v.oai.rad\",\"params\":{\"a\":10,\"d\":0.9}}\n",
            ],
            layout,
        );
        r.device.connect(Instant::now(), Some(candidate()));
        let events = r.device.poll(Duration::ZERO);
        assert_eq!(events.len(), 1, "dead-zone sample produces nothing");
    }

    #[test]
    fn lighting_pushes_once_then_dedupes() {
        let mut r = rig(
            &[
                "{\"result\":{\"version\":\"1\"},\"id\":1}\n",
                "{\"result\":{\"batteryPercentage\":42,\"isCharging\":false},\"id\":2}\n",
                "{\"result\":null,\"id\":3}\n",
                "{\"result\":null,\"id\":4}\n",
            ],
            Layout::default(),
        );
        r.device.connect(Instant::now(), Some(candidate()));
        let now = Instant::now();
        r.device.tick(now);
        assert_eq!(r.last_for("v.oai.rgbcfg")["method"], "v.oai.rgbcfg");
        assert_eq!(r.last_for("v.oai.thstatus")["method"], "v.oai.thstatus");
        let before = r.writes();
        r.device.tick(now + Duration::from_millis(20));
        assert_eq!(r.writes(), before, "identical payload is not re-sent");
    }

    #[test]
    fn tick_refreshes_battery_from_device_status() {
        let mut r = rig(
            &[
                "{\"result\":{\"version\":\"1\"},\"id\":1}\n",
                "{\"result\":{\"batteryPercentage\":42,\"isCharging\":false},\"id\":2}\n",
                "{\"result\":null,\"id\":3}\n",
                "{\"result\":null,\"id\":4}\n",
            ],
            Layout::default(),
        );
        r.device.connect(Instant::now(), Some(candidate()));
        r.device.tick(Instant::now());
        assert!(r.methods().contains(&"device.status".to_string()));
        assert_eq!(
            r.device.state().battery,
            Some(Battery {
                percentage: 42,
                is_charging: Some(false)
            })
        );
    }

    #[test]
    fn timeout_tears_down_and_schedules_reconnect() {
        let mut r = rig(
            &["{\"result\":{\"version\":\"1\"},\"id\":1}\n"],
            Layout::default(),
        );
        r.device.connect(Instant::now(), Some(candidate()));
        let events = r.device.fail(RpcError::Timeout);
        assert!(!r.device.is_connected());
        assert_eq!(events[0], Event::Disconnected);
        assert_eq!(r.device.state().status, Status::Error);
        assert_eq!(r.device.reconnect_attempt, 1);
    }

    #[test]
    fn inactivity_goes_dark_then_next_input_restores() {
        let mut r = rig(
            &[
                "{\"result\":{\"version\":\"1\"},\"id\":1}\n",
                "{\"result\":{\"batteryPercentage\":10},\"id\":2}\n",
                "{\"result\":null,\"id\":3}\n",
                "{\"result\":null,\"id\":4}\n",
                "{\"result\":null,\"id\":5}\n",
                "{\"result\":null,\"id\":6}\n",
                "{\"result\":null,\"id\":7}\n",
            ],
            Layout::default(),
        );
        // a selected, working agent key puts something on the ambient ring
        r.device.set_slots(vec![AgentSlot {
            id: 0,
            status: SlotStatus::Working,
            selected: true,
            pulsing: false,
        }]);
        r.device.set_lighting(LightingModel {
            brightness: 0.8,
            inactivity_timeout: Some(Duration::from_millis(30)),
        });
        r.device.connect(Instant::now(), Some(candidate()));
        let t0 = Instant::now();
        r.device.tick(t0);
        assert_eq!(r.last_for("v.oai.rgbcfg")["params"]["ambient"]["b"], 0.8);
        assert_eq!(r.last_for("v.oai.thstatus")["params"][0]["b"], 0.8);

        r.device.tick(t0 + Duration::from_millis(40));
        assert!(r.device.off_for_inactivity);
        assert_eq!(
            r.last_for("v.oai.rgbcfg")["params"]["ambient"]["b"],
            0.0,
            "dark payload"
        );
        assert_eq!(
            r.last_for("v.oai.thstatus")["params"][0]["b"],
            0.0,
            "dark payload"
        );
    }
}
