//! Lighting derivation.
//!
//! Ported from `CodexMicroService`'s three helpers in `service-C6nm9ayu.js`:
//!
//! ```text
//! $  (thread_lighting) per-agent-key status lighting  -> v.oai.thstatus
//! se (rgb_config)      keys + ambient ring            -> v.oai.rgbcfg
//! ce (voice_ambient)   push-to-talk / dictation state
//! ```
//!
//! The status palette comes from `app-shared` (`Ere`): working, unread, idle,
//! awaiting-*, error, off.

use crate::oai::{LightingConfig, LightingSide, ThreadLighting};

/// `M` — number of agent keys.
pub const AGENT_SLOT_COUNT: u8 = 6;
/// `W` — breath speed while a slot is selected or pulsing.
pub const PULSE_SPEED: f32 = 0.4;
/// `G` — snake speed while a slot is working.
pub const WORKING_SPEED: f32 = 0.4;
/// `ce('recording')` colour.
pub const VOICE_RECORDING_COLOR: u32 = 3_050_327;
/// `ce('processing' | 'completed')` colour.
pub const VOICE_WHITE: u32 = 0xFF_FFFF;

const EFFECT_OFF: u8 = 0;
const EFFECT_SOLID: u8 = 1;
const EFFECT_SNAKE: u8 = 2;
const EFFECT_BREATH: u8 = 4;

/// What an agent key is showing. Mirrors the app's thread status vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotStatus {
    #[default]
    Off,
    Idle,
    Working,
    Unread,
    AwaitingApproval,
    AwaitingResponse,
    Error,
}

impl SlotStatus {
    /// The kebab-case name — the same token the control socket accepts.
    pub fn to_token(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(name)) => name,
            _ => "off".to_string(),
        }
    }

    /// `Ere` in `app-shared`: status → packed RGB.
    pub fn color(self) -> u32 {
        match self {
            SlotStatus::Off => 0x00_0000,
            SlotStatus::Idle => 0xFF_FFFF,
            SlotStatus::Working => 0x30_4FFE,
            SlotStatus::Unread => 0x00_FF4C,
            SlotStatus::AwaitingApproval | SlotStatus::AwaitingResponse => 0xFF_6D00,
            SlotStatus::Error => 0xFF_0033,
        }
    }
}

/// Push-to-talk / dictation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceState {
    #[default]
    Idle,
    Recording,
    Processing,
    Completed,
}

/// One agent key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentSlot {
    pub id: u8,
    pub status: SlotStatus,
    pub selected: bool,
    pub pulsing: bool,
}

/// `le()` — six idle-but-off agent keys.
pub fn default_agent_slots() -> Vec<AgentSlot> {
    (0..AGENT_SLOT_COUNT)
        .map(|id| AgentSlot {
            id,
            status: SlotStatus::Off,
            selected: false,
            pulsing: false,
        })
        .collect()
}

fn side(effect: u8, brightness: f32, speed: f32, color: u32) -> LightingSide {
    LightingSide {
        effect,
        brightness,
        speed,
        magic: 0,
        color,
    }
}

/// The all-off default (`K.keys` / `K.ambient`).
fn dark() -> LightingSide {
    side(EFFECT_OFF, 0.0, 0.0, 0)
}

/// Port of `$`: per-agent-key status lighting for `v.oai.thstatus`.
///
/// An `off` slot is sent explicitly dark; a selected or pulsing slot breathes at
/// `PULSE_SPEED`, everything else is solid at the slot's status colour.
pub fn thread_lighting(slots: &[AgentSlot], brightness: f32) -> Vec<ThreadLighting> {
    slots
        .iter()
        .map(|slot| {
            if slot.status == SlotStatus::Off {
                return ThreadLighting {
                    id: slot.id as u32,
                    color: Some(0),
                    brightness: Some(0.0),
                    effect: Some(EFFECT_OFF),
                    speed: Some(0.0),
                    sync_keys: Some(0),
                    sync_ambient: Some(0),
                };
            }
            let pulsing = slot.selected || slot.pulsing;
            ThreadLighting {
                id: slot.id as u32,
                color: Some(slot.status.color()),
                brightness: Some(brightness),
                effect: Some(if pulsing { EFFECT_BREATH } else { EFFECT_SOLID }),
                speed: Some(if pulsing { PULSE_SPEED } else { 0.0 }),
                sync_keys: Some(0),
                sync_ambient: Some(0),
            }
        })
        .collect()
}

/// Port of `ce`: the ambient override while the mic / dictation is active.
pub fn voice_ambient(voice: VoiceState, brightness: f32) -> Option<LightingSide> {
    match voice {
        VoiceState::Idle => None,
        VoiceState::Recording => Some(side(
            EFFECT_SNAKE,
            brightness,
            WORKING_SPEED,
            VOICE_RECORDING_COLOR,
        )),
        VoiceState::Processing => Some(side(EFFECT_SNAKE, brightness, WORKING_SPEED, VOICE_WHITE)),
        VoiceState::Completed => Some(side(EFFECT_SOLID, brightness, 0.0, VOICE_WHITE)),
    }
}

/// Port of `se`: the keys + ambient configuration for `v.oai.rgbcfg`.
///
/// * `snaking` — the host is showing a fleet-wide status (`snakingAmbientStatus`);
///   it takes over the ambient ring and leaves the keys alone.
/// * `selection_visible` — the selection highlight is on screen, so the keys echo
///   the ambient colour.
pub fn rgb_config(
    slots: &[AgentSlot],
    voice: VoiceState,
    selection_visible: bool,
    brightness: f32,
    snaking: Option<SlotStatus>,
) -> LightingConfig {
    if let Some(status) = snaking {
        return LightingConfig {
            keys: dark(),
            ambient: side(EFFECT_SNAKE, brightness, WORKING_SPEED, status.color()),
        };
    }

    let voice_side = voice_ambient(voice, brightness);
    let selected = slots.iter().find(|s| s.selected);
    let Some(selected) = selected.filter(|s| s.status != SlotStatus::Off) else {
        return LightingConfig {
            keys: dark(),
            ambient: voice_side.unwrap_or_else(dark),
        };
    };

    let selected_side = side(
        if selected.status == SlotStatus::Working {
            EFFECT_SNAKE
        } else {
            EFFECT_SOLID
        },
        brightness,
        if selected.status == SlotStatus::Working {
            WORKING_SPEED
        } else {
            0.0
        },
        selected.status.color(),
    );
    let ambient = voice_side.unwrap_or_else(|| {
        if selected.status == SlotStatus::Working || selection_visible {
            selected_side
        } else {
            dark()
        }
    });
    let keys = if selection_visible {
        side(EFFECT_SOLID, brightness, 0.0, ambient.color)
    } else {
        dark()
    };
    LightingConfig { keys, ambient }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working(selected: bool) -> AgentSlot {
        AgentSlot {
            id: 0,
            status: SlotStatus::Working,
            selected,
            pulsing: false,
        }
    }

    #[test]
    fn status_tokens_match_the_socket_vocabulary() {
        assert_eq!(SlotStatus::Working.to_token(), "working");
        assert_eq!(SlotStatus::AwaitingApproval.to_token(), "awaiting-approval");
        assert_eq!(SlotStatus::Off.to_token(), "off");
        // the token is exactly what the control socket parses back
        assert_eq!(
            crate::control::parse("agent 0 awaiting-response").unwrap(),
            crate::control::Command::Agent {
                index: 0,
                status: SlotStatus::AwaitingResponse
            }
        );
    }

    #[test]
    fn status_palette_matches_the_app() {
        assert_eq!(SlotStatus::Working.color(), 3166206);
        assert_eq!(SlotStatus::Unread.color(), 65356);
        assert_eq!(SlotStatus::Idle.color(), 16777215);
        assert_eq!(SlotStatus::AwaitingApproval.color(), 16739584);
        assert_eq!(SlotStatus::AwaitingResponse.color(), 16739584);
        assert_eq!(SlotStatus::Error.color(), 16711731);
        assert_eq!(SlotStatus::Off.color(), 0);
    }

    #[test]
    fn off_slots_are_sent_explicitly_dark() {
        let slots = default_agent_slots();
        let threads = thread_lighting(&slots, 1.0);
        assert_eq!(threads.len(), 6);
        assert_eq!(threads[0].id, 0);
        assert_eq!(threads[0].effect, Some(EFFECT_OFF));
        assert_eq!(threads[0].brightness, Some(0.0));
    }

    #[test]
    fn selected_slot_breathes_at_status_colour() {
        let slots = [working(true)];
        let threads = thread_lighting(&slots, 0.5);
        assert_eq!(threads[0].color, Some(3166206));
        assert_eq!(threads[0].effect, Some(EFFECT_BREATH));
        assert_eq!(threads[0].speed, Some(PULSE_SPEED));
        assert_eq!(threads[0].brightness, Some(0.5));
    }

    #[test]
    fn unselected_slot_is_solid() {
        let threads = thread_lighting(&[working(false)], 1.0);
        assert_eq!(threads[0].effect, Some(EFFECT_SOLID));
        assert_eq!(threads[0].speed, Some(0.0));
    }

    #[test]
    fn voice_states_drive_the_ambient_ring() {
        assert!(voice_ambient(VoiceState::Idle, 1.0).is_none());
        let recording = voice_ambient(VoiceState::Recording, 1.0).unwrap();
        assert_eq!(recording.effect, EFFECT_SNAKE);
        assert_eq!(recording.color, 3_050_327);
        let done = voice_ambient(VoiceState::Completed, 1.0).unwrap();
        assert_eq!(done.effect, EFFECT_SOLID);
        assert_eq!(done.color, 0xFF_FFFF);
    }

    #[test]
    fn working_selection_snakes_and_leaves_keys_dark() {
        let cfg = rgb_config(&[working(true)], VoiceState::Idle, false, 1.0, None);
        assert_eq!(cfg.ambient.effect, EFFECT_SNAKE);
        assert_eq!(cfg.ambient.speed, WORKING_SPEED);
        assert_eq!(cfg.ambient.color, 3166206);
        assert_eq!(
            cfg.keys.effect, EFFECT_OFF,
            "keys stay dark unless selection is visible"
        );
    }

    #[test]
    fn selection_highlight_mirrors_ambient_onto_the_keys() {
        let cfg = rgb_config(&[working(true)], VoiceState::Idle, true, 0.7, None);
        assert_eq!(cfg.keys.effect, EFFECT_SOLID);
        assert_eq!(cfg.keys.color, cfg.ambient.color);
        assert_eq!(cfg.keys.brightness, 0.7);
    }

    #[test]
    fn idle_selection_leaves_the_ring_dark() {
        let slot = AgentSlot {
            id: 0,
            status: SlotStatus::Idle,
            selected: true,
            pulsing: false,
        };
        let cfg = rgb_config(&[slot], VoiceState::Idle, false, 1.0, None);
        assert_eq!(cfg.ambient.effect, EFFECT_OFF);
        assert_eq!(cfg.keys.effect, EFFECT_OFF);
    }

    #[test]
    fn snaking_status_takes_over_the_ring() {
        let cfg = rgb_config(
            &[working(true)],
            VoiceState::Idle,
            true,
            1.0,
            Some(SlotStatus::Error),
        );
        assert_eq!(cfg.ambient.effect, EFFECT_SNAKE);
        assert_eq!(cfg.ambient.color, 16711731);
        assert_eq!(
            cfg.keys.effect, EFFECT_OFF,
            "snaking mode ignores selection lighting"
        );
    }

    #[test]
    fn voice_wins_over_the_selected_slot() {
        let cfg = rgb_config(&[working(true)], VoiceState::Recording, false, 1.0, None);
        assert_eq!(cfg.ambient.color, 3_050_327);
        assert_eq!(cfg.keys.effect, EFFECT_OFF);
    }
}
