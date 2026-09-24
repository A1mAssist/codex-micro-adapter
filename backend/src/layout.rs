//! Key / encoder / joystick → action mapping.
//!
//! Ported from the frontend chunks `codex-micro-layout` (`g`, `h`, `_`, the keycap
//! catalog) and `codex-micro-bridge` (`wt`, `Tt`, `Ot`, `kt`).
//!
//! Deliberately dropped: the thread/agent-status machinery in
//! `codex-micro-slot-signals` (agent keys painting thread status, pinning source,
//! composer navigation). None of it means anything outside the ChatGPT shell.

use crate::oai::HidEvent;
use std::collections::BTreeMap;

/// HID key ids that map to physical slots, in device order.
pub const SLOT_IDS: [&str; 8] = [
    "ACT06",
    "ACT07",
    "ACT08",
    "ACT09",
    "ACT10",
    "ACT11",
    "ACT12",
    "ACT10_ACT11",
];
/// The six agent keys (`AG00`..`AG05`).
pub const AGENT_SLOTS: [&str; 6] = ["AG00", "AG01", "AG02", "AG03", "AG04", "AG05"];
/// Analog stick directions, as stored in `config.toml`.
pub const STICK_DIRECTIONS: [&str; 4] = ["up", "right", "down", "left"];

/// Joystick dead zone.
///
/// The vendor's `lqs` (in `app-initial-*.js`) rejects a sample below `0.5`
/// before it ever looks at the angle, so a real device rests at half
/// deflection and only the far half of the travel is reported.
pub const STICK_DEAD_ZONE: f32 = 0.5;

/// What a key does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum Action {
    /// A ChatGPT app command id, e.g. `composer.submit`.
    Command(String),
    /// Type literal text (used by the `:yolo:` / `:yeet:` keycaps).
    ComposerText { label: String, text: String },
    /// Open a URL.
    ExternalUrl { label: String, url: String },
    /// Hold to talk.
    PushToTalk,
    /// A skill invocation bound per slot.
    Skill { skill_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Single,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Command(&'static str),
    ComposerText(&'static str, &'static str),
    ExternalUrl(&'static str, &'static str),
    /// `named` in the vendor catalog: only meaningful inside the app (MIC).
    Named(&'static str),
    /// `custom-shortcut`: an unbound key the user is expected to map.
    CustomShortcut,
}

#[derive(Debug, Clone, Copy)]
struct Keycap {
    id: &'static str,
    size: Size,
    kind: Kind,
}

/// The keycap catalog, verbatim from `codex-micro-layout`.
const KEYCAPS: &[Keycap] = &[
    Keycap {
        id: "FAST",
        size: Size::Single,
        kind: Kind::Command("composer.toggleFastMode"),
    },
    Keycap {
        id: "APPR",
        size: Size::Single,
        kind: Kind::Command("approval.approve"),
    },
    Keycap {
        id: "REJ",
        size: Size::Single,
        kind: Kind::Command("approval.decline"),
    },
    Keycap {
        id: "SPLIT",
        size: Size::Single,
        kind: Kind::Command("forkThread"),
    },
    Keycap {
        id: "MIC",
        size: Size::Double,
        kind: Kind::Named("Push to talk"),
    },
    Keycap {
        id: "MIC1",
        size: Size::Single,
        kind: Kind::Named("Push to talk"),
    },
    Keycap {
        id: "CODEX",
        size: Size::Single,
        kind: Kind::Command("composer.submit"),
    },
    Keycap {
        id: "BUG",
        size: Size::Single,
        kind: Kind::Command("feedback"),
    },
    Keycap {
        id: "OAI",
        size: Size::Single,
        kind: Kind::ExternalUrl("Open OpenAI docs", "https://developers.openai.com"),
    },
    Keycap {
        id: "TERM",
        size: Size::Single,
        kind: Kind::Command("toggleTerminal"),
    },
    Keycap {
        id: "DWN",
        size: Size::Single,
        kind: Kind::Command("copyConversationMarkdown"),
    },
    Keycap {
        id: "DEL",
        size: Size::Single,
        kind: Kind::Command("archiveThread"),
    },
    Keycap {
        id: "NEW",
        size: Size::Single,
        kind: Kind::Command("newTask"),
    },
    Keycap {
        id: "NAV",
        size: Size::Single,
        kind: Kind::Command("openBrowserTab"),
    },
    Keycap {
        id: "MAGIC",
        size: Size::Single,
        kind: Kind::Command("toggleThreadPin"),
    },
    Keycap {
        id: "DIFF",
        size: Size::Single,
        kind: Kind::Command("toggleReviewTab"),
    },
    Keycap {
        id: "PLAY",
        size: Size::Single,
        kind: Kind::Command("environmentAction1"),
    },
    Keycap {
        id: "GIT",
        size: Size::Single,
        kind: Kind::Command("git.commit"),
    },
    Keycap {
        id: "BRCH",
        size: Size::Single,
        kind: Kind::Command("git.createDraftPullRequest"),
    },
    Keycap {
        id: "BRANCH",
        size: Size::Single,
        kind: Kind::Command("git.createBranch"),
    },
    Keycap {
        id: "MRG",
        size: Size::Single,
        kind: Kind::Command("git.mergePullRequest"),
    },
    Keycap {
        id: "PR",
        size: Size::Single,
        kind: Kind::Command("git.createPullRequest"),
    },
    Keycap {
        id: "PAINT",
        size: Size::Single,
        kind: Kind::Command("composer.addPhotos"),
    },
    Keycap {
        id: "LAB",
        size: Size::Single,
        kind: Kind::Command("settings"),
    },
    Keycap {
        id: "PARTY",
        size: Size::Single,
        kind: Kind::Command("openSideChat"),
    },
    Keycap {
        id: "TIME",
        size: Size::Single,
        kind: Kind::Command("manageTasks"),
    },
    Keycap {
        id: "MIND+",
        size: Size::Single,
        kind: Kind::Command("composer.increaseReasoningEffort"),
    },
    Keycap {
        id: "MIND-",
        size: Size::Single,
        kind: Kind::Command("composer.decreaseReasoningEffort"),
    },
    Keycap {
        id: "EMPT1",
        size: Size::Single,
        kind: Kind::CustomShortcut,
    },
    Keycap {
        id: "EMPT2",
        size: Size::Single,
        kind: Kind::CustomShortcut,
    },
    Keycap {
        id: "EMPT3",
        size: Size::Single,
        kind: Kind::CustomShortcut,
    },
    Keycap {
        id: "EMPT4",
        size: Size::Single,
        kind: Kind::CustomShortcut,
    },
    Keycap {
        id: "SETUP",
        size: Size::Single,
        kind: Kind::Command("settings"),
    },
    Keycap {
        id: "FOLD",
        size: Size::Single,
        kind: Kind::Command("openFolder"),
    },
    Keycap {
        id: "UPL",
        size: Size::Single,
        kind: Kind::Command("composer.addFiles"),
    },
    Keycap {
        id: "APPS",
        size: Size::Single,
        kind: Kind::Command("all-products"),
    },
    Keycap {
        id: "YOLO",
        size: Size::Single,
        kind: Kind::ComposerText("Write :yolo: in the composer", ":yolo:"),
    },
    Keycap {
        id: "YEET",
        size: Size::Single,
        kind: Kind::ComposerText("Write :yeet: in the composer", ":yeet:"),
    },
    Keycap {
        id: "EMPT5",
        size: Size::Double,
        kind: Kind::CustomShortcut,
    },
];

/// Keycaps offered for a slot of the given width — the vendor picker filters the
/// catalog this way (`d()` in `codex-micro-layout`), and ACT10/ACT11 merged into
/// one slot only accepts double-width caps.
pub fn keycaps_for(size: Size) -> impl Iterator<Item = &'static str> {
    KEYCAPS.iter().filter(move |k| k.size == size).map(|k| k.id)
}

/// Catalog lookup: unknown ids fall back to the first entry, as upstream does.
fn keycap(id: &str) -> &'static Keycap {
    KEYCAPS.iter().find(|k| k.id == id).unwrap_or(&KEYCAPS[0])
}

/// Per-slot binding. Mirrors the vendor's slot object: either a keycap reference
/// or an explicit action, plus an optional command override.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotConfig {
    pub keycap_id: String,
    pub command_id: Option<String>,
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EncoderMode {
    /// Ticks scroll the conversation (up/down arrows).
    ConversationScroll,
    /// Ticks move between composer controls — only meaningful in the app.
    ComposerNavigation,
    /// `reasoning` in `config.toml`: the knob walks reasoning effort up and down.
    Reasoning,
    /// Ticks fire whatever the user bound in `encoder`.
    Custom,
}

impl Default for EncoderMode {
    fn default() -> Self {
        EncoderMode::ConversationScroll
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub separate_microphone_keys: bool,
    pub encoder_mode: EncoderMode,
    pub slots: BTreeMap<String, SlotConfig>,
    pub analog_stick: BTreeMap<String, Action>,
    /// Bound only in `EncoderMode::Custom`; keys are `right` / `left`, the
    /// same gesture names the app stores.
    pub encoder: BTreeMap<String, Action>,
}

impl Default for Layout {
    /// The app's own starting layout, so a fresh install types and scrolls
    /// before anything is configured.
    fn default() -> Self {
        let mut slots = BTreeMap::new();
        for (slot, keycap) in [
            ("ACT06", "FAST"),
            ("ACT07", "APPR"),
            ("ACT08", "REJ"),
            ("ACT09", "SPLIT"),
            ("ACT10", "MIC1"),
            ("ACT11", "EMPT1"),
            ("ACT10_ACT11", "MIC"),
            ("ACT12", "CODEX"),
        ] {
            slots.insert(
                slot.to_string(),
                SlotConfig {
                    keycap_id: keycap.to_string(),
                    command_id: None,
                    action: None,
                },
            );
        }

        let mut analog_stick = BTreeMap::new();
        for (direction, command) in [
            ("up", "composer.togglePlanMode"),
            ("right", "navigateForward"),
            ("down", "toggleSidebar"),
            ("left", "navigateBack"),
        ] {
            analog_stick.insert(direction.to_string(), Action::Command(command.to_string()));
        }

        Self {
            separate_microphone_keys: false,
            encoder_mode: EncoderMode::default(),
            slots,
            analog_stick,
            encoder: BTreeMap::new(),
        }
    }
}

/// Resolve one slot to the action it performs. Port of `codex-micro-layout`'s `g`.
///
/// Difference from the app: the vendor resolves `commandId` against its own
/// command registry and refuses ids the current build does not expose. A daemon
/// has no such registry, so any non-empty id is accepted and handed to the
/// action executor.
pub fn resolve_slot(slot: &SlotConfig) -> Option<Action> {
    let cap = keycap(&slot.keycap_id);

    // an explicit action on the slot wins outright
    if let Some(action) = &slot.action {
        match action {
            Action::ComposerText { .. } | Action::Skill { .. } => return Some(action.clone()),
            _ => {}
        }
    }

    let command_id = slot
        .action
        .as_ref()
        .and_then(|a| match a {
            Action::Command(c) => Some(c.clone()),
            _ => None,
        })
        .or_else(|| slot.command_id.clone());

    if let Some(id) = command_id {
        if !id.is_empty() {
            return Some(Action::Command(normalize_command(&id)));
        }
        if cap.kind == Kind::CustomShortcut {
            return None;
        }
    }

    match cap.kind {
        Kind::Named(_) => Some(Action::PushToTalk), // MIC / MIC1
        Kind::Command(c) => Some(Action::Command(c.to_string())),
        Kind::ComposerText(label, text) => Some(Action::ComposerText {
            label: label.to_string(),
            text: text.to_string(),
        }),
        Kind::ExternalUrl(label, url) => Some(Action::ExternalUrl {
            label: label.to_string(),
            url: url.to_string(),
        }),
        Kind::CustomShortcut => None,
    }
}

/// `newThread` is an alias of `newTask`.
fn normalize_command(id: &str) -> String {
    if id == "newThread" {
        "newTask".to_string()
    } else {
        id.to_string()
    }
}

/// Which slot a physical key drives (`codex-micro-layout`'s `h`, plus the
/// ACT10/ACT11 merge from the bridge's `wt`).
///
/// With `separate_microphone_keys` off, ACT10 and ACT11 are one wide key: ACT10
/// drives `ACT10_ACT11` and ACT11 alone is ignored.
pub fn slot_for_key(key: &str, separate_microphone_keys: bool) -> Option<&'static str> {
    if !separate_microphone_keys {
        if key == "ACT10" {
            return Some("ACT10_ACT11");
        }
        if key == "ACT11" {
            return None;
        }
    }
    SLOT_IDS.iter().copied().find(|s| *s == key)
}

/// What a key event should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// An action that is not tied to a keycap: a stick push, a knob tick, a skill.
    Act(Action),
    /// A keycap press. The slot (`ACT06`…`ACT12`) is the binding key that wins;
    /// `action` is the catalogue fallback, and `None` when the keycap carries no
    /// action of its own (`EMPT1`…). Either way the slot answers to a binding, so
    /// nothing is swallowed.
    Keycap {
        slot: String,
        action: Option<Action>,
    },
    /// One of the six agent keys was pressed: focus that session's window, or
    /// fall back to the `agent.focus.<n>` binding.
    AgentKey(u8),
    EncoderPress,
    EncoderRelease,
    /// Encoder click: the action the layout binds to `click`, or `None` in the
    /// built-in modes, where the surrounding app decides what a click means.
    EncoderClick(Option<Action>),
    /// Encoder press-and-hold: the action the layout binds to `longPress`, or
    /// `None` in the built-in modes, where the app opens its settings page.
    EncoderLongPress(Option<Action>),
    /// Encoder tick in conversation-scroll mode: `-1` up, `+1` down.
    Scroll(i8),
    /// Encoder tick in custom mode.
    EncoderTick(Action),
    /// Analog stick pushed in a configured direction.
    Stick(Action),
}

/// The action the app's own `encoder` map binds to a knob gesture.
///
/// Only `custom` mode reads that map — every other mode has built-in behaviour
/// (see `docs/PROTOCOL.md`) — so this is `None` for them.
pub fn encoder_action(layout: &Layout, gesture: &str) -> Option<Action> {
    if layout.encoder_mode != EncoderMode::Custom {
        return None;
    }
    layout.encoder.get(gesture).cloned()
}

/// Resolve a key/encoder event. Port of the bridge's `wt` + `Tt` + `Ot`.
pub fn resolve_event(event: &HidEvent, layout: &Layout) -> Option<Trigger> {
    let key = event.key.as_str();

    // The six agent keys are buttons as well as lights. They are not keycaps, so
    // there is no layout action to resolve: the host focuses the session that
    // owns that key, and falls back to the `agent.focus.<n>` binding.
    if let Some(rest) = key.strip_prefix("AG") {
        let index: u8 = rest.parse().ok()?;
        return (event.act == 1 && index < crate::lighting::AGENT_SLOT_COUNT)
            .then_some(Trigger::AgentKey(index));
    }

    // encoder rotation never produces a key action
    if key == "ENC_CW" || key == "ENC_CC" {
        if event.act != 2 {
            return None;
        }
        return match layout.encoder_mode {
            EncoderMode::Custom => {
                let gesture = if key == "ENC_CW" { "right" } else { "left" };
                encoder_action(layout, gesture).map(Trigger::EncoderTick)
            }
            // `conversation-scroll` sends plain arrows; `composer-navigation`
            // only exists inside the app and has no meaning out here.
            EncoderMode::ConversationScroll => {
                Some(Trigger::Scroll(if key == "ENC_CW" { -1 } else { 1 }))
            }
            EncoderMode::ComposerNavigation => None,
            // Straight from the bridge: clockwise is `ArrowUp`, and
            // `ArrowUp` is what the app sends to *decrease* effort.
            EncoderMode::Reasoning => {
                let command = if key == "ENC_CW" {
                    "composer.decreaseReasoningEffort"
                } else {
                    "composer.increaseReasoningEffort"
                };
                Some(Trigger::EncoderTick(Action::Command(command.to_string())))
            }
        };
    }

    if key.starts_with("ENC") {
        return match event.act {
            1 => Some(Trigger::EncoderPress),
            0 => Some(Trigger::EncoderRelease),
            _ => None,
        };
    }

    // The host resolves a keycap through the binding table now, so a slot with
    // no action of its own still reaches it instead of being dropped here.
    let slot_id = slot_for_key(key, layout.separate_microphone_keys)?;
    let action = layout.slots.get(slot_id).and_then(resolve_slot);
    (event.act == 1).then(|| Trigger::Keycap {
        slot: slot_id.to_string(),
        action,
    })
}

/// Nearest of the four stick directions, ported from the vendor's `lqs`.
///
/// The angle is a fraction of a full turn, not degrees: `0.0` is right, `0.25`
/// down, `0.5` left and `0.75` up, and the vendor cuts the circle into eighths.
/// Reading it as degrees collapsed every push onto "up" - the whole travel sat
/// below the first bucket.
pub fn stick_direction(angle: f32) -> &'static str {
    let a = angle.rem_euclid(1.0);
    if (0.625..0.875).contains(&a) {
        "up"
    } else if (0.125..0.375).contains(&a) {
        "down"
    } else if (0.375..0.625).contains(&a) {
        "left"
    } else {
        "right"
    }
}

/// Resolve a stick position, honouring the dead zone.
pub fn resolve_stick(angle: f32, distance: f32, layout: &Layout) -> Option<Trigger> {
    if distance < STICK_DEAD_ZONE {
        return None;
    }
    layout
        .analog_stick
        .get(stick_direction(angle))
        .cloned()
        .map(Trigger::Stick)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(keycap_id: &str) -> SlotConfig {
        SlotConfig {
            keycap_id: keycap_id.to_string(),
            ..Default::default()
        }
    }

    fn layout_with(slot_id: &str, cfg: SlotConfig) -> Layout {
        let mut layout = Layout::default();
        layout.slots.insert(slot_id.to_string(), cfg);
        layout
    }

    fn hid(key: &str, act: u8) -> HidEvent {
        HidEvent {
            key: key.to_string(),
            act,
            agent_group: None,
        }
    }

    #[test]
    fn reasoning_mode_matches_the_bridge_direction() {
        let mut layout = Layout::default();
        layout.encoder_mode = EncoderMode::Reasoning;
        assert_eq!(
            resolve_event(&hid("ENC_CW", 2), &layout),
            Some(Trigger::EncoderTick(Action::Command(
                "composer.decreaseReasoningEffort".into()
            ))),
            "clockwise is ArrowUp, and ArrowUp decreases effort"
        );
        assert_eq!(
            resolve_event(&hid("ENC_CC", 2), &layout),
            Some(Trigger::EncoderTick(Action::Command(
                "composer.increaseReasoningEffort".into()
            )))
        );
    }

    #[test]
    fn every_encoder_mode_round_trips_through_json() {
        for mode in [
            EncoderMode::ConversationScroll,
            EncoderMode::ComposerNavigation,
            EncoderMode::Reasoning,
            EncoderMode::Custom,
        ] {
            let text = serde_json::to_string(&mode).unwrap();
            assert_eq!(
                serde_json::from_str::<EncoderMode>(&text).unwrap(),
                mode,
                "{text}"
            );
        }
        // the app's own spelling for the reasoning mode
        assert_eq!(
            serde_json::from_str::<EncoderMode>("\"reasoning\"").unwrap(),
            EncoderMode::Reasoning
        );
    }

    #[test]
    fn catalog_resolves_to_commands() {
        assert_eq!(
            resolve_slot(&slot("CODEX")),
            Some(Action::Command("composer.submit".into()))
        );
        assert_eq!(
            resolve_slot(&slot("NEW")),
            Some(Action::Command("newTask".into()))
        );
        assert_eq!(
            resolve_slot(&slot("YOLO")),
            Some(Action::ComposerText {
                label: "Write :yolo: in the composer".into(),
                text: ":yolo:".into()
            })
        );
        assert_eq!(
            resolve_slot(&slot("OAI")).map(|a| matches!(a, Action::ExternalUrl { .. })),
            Some(true)
        );
    }

    #[test]
    fn mic_keycaps_become_push_to_talk() {
        assert_eq!(resolve_slot(&slot("MIC")), Some(Action::PushToTalk));
        assert_eq!(resolve_slot(&slot("MIC1")), Some(Action::PushToTalk));
        // `named` entries with no MIC id resolve via the same branch
        assert_eq!(
            resolve_slot(&slot("EMPT1")),
            None,
            "custom-shortcut is unbound"
        );
    }

    #[test]
    fn explicit_command_override_wins_and_newthread_aliases() {
        let cfg = SlotConfig {
            keycap_id: "CODEX".into(),
            command_id: Some("newThread".into()),
            action: None,
        };
        assert_eq!(resolve_slot(&cfg), Some(Action::Command("newTask".into())));
    }

    #[test]
    fn act10_and_act11_merge_unless_separated() {
        assert_eq!(slot_for_key("ACT10", false), Some("ACT10_ACT11"));
        assert_eq!(slot_for_key("ACT11", false), None);
        assert_eq!(slot_for_key("ACT11", true), Some("ACT11"));
        assert_eq!(slot_for_key("ACT06", false), Some("ACT06"));
        assert_eq!(
            slot_for_key("AG00", false),
            None,
            "agent keys are not slots"
        );
    }

    #[test]
    fn agent_keys_are_buttons_too() {
        let layout = Layout::default();
        assert_eq!(
            resolve_event(&hid("AG00", 1), &layout),
            Some(Trigger::AgentKey(0)),
            "the first agent key focuses agent 0"
        );
        assert_eq!(
            resolve_event(&hid("AG05", 1), &layout),
            Some(Trigger::AgentKey(5))
        );
        assert_eq!(
            resolve_event(&hid("AG00", 0), &layout),
            None,
            "only presses fire"
        );
        assert_eq!(resolve_event(&hid("AG09", 1), &layout), None, "out of range");
        assert_eq!(resolve_event(&hid("AGXX", 1), &layout), None);
    }

    #[test]
    fn only_presses_fire_actions() {
        let layout = layout_with("ACT06", slot("CODEX"));
        assert_eq!(
            resolve_event(&hid("ACT06", 1), &layout),
            Some(Trigger::Keycap {
                slot: "ACT06".into(),
                action: Some(Action::Command("composer.submit".into())),
            })
        );
        assert_eq!(resolve_event(&hid("ACT06", 0), &layout), None);
        assert_eq!(resolve_event(&hid("ACT06", 2), &layout), None);
    }

    #[test]
    fn a_slot_with_no_action_still_reaches_the_bindings() {
        // the app ships ACT11 as an empty keycap, and the separate-microphone
        // switch is the only thing that gives it a key of its own: it must still
        // resolve, or the user's second microphone key is silent
        let mut layout = Layout::default();
        layout.separate_microphone_keys = true;
        assert_eq!(
            resolve_event(&hid("ACT11", 1), &layout),
            Some(Trigger::Keycap {
                slot: "ACT11".into(),
                action: None,
            })
        );
        // and the same key is the right half of the wide key when they are merged
        layout.separate_microphone_keys = false;
        assert_eq!(
            resolve_event(&hid("ACT10", 1), &layout),
            Some(Trigger::Keycap {
                slot: "ACT10_ACT11".into(),
                action: Some(Action::PushToTalk),
            })
        );
        assert_eq!(resolve_event(&hid("ACT11", 1), &layout), None);
    }

    #[test]
    fn the_slot_binding_beats_the_keycap_catalogue() {
        // this is the whole point of the slot names: a harness can give ACT07 a
        // job of its own without changing which keycap sits there
        let mut bindings = crate::actions::Bindings::default();
        bindings.set("ACT06", "ctrl+1");
        bindings.set("composer.submit", "ctrl+2");
        let mut performer = crate::actions::tests::Recording::default();
        let layout = layout_with("ACT06", slot("CODEX"));

        let Some(trigger) = resolve_event(&hid("ACT06", 1), &layout) else {
            panic!("no trigger")
        };
        assert_eq!(
            crate::actions::dispatch(&trigger, &bindings, &mut performer),
            crate::actions::Outcome::Sent("ctrl+1".into())
        );

        // with no slot binding, the catalogue default still applies
        let mut performer = crate::actions::tests::Recording::default();
        let outcome = crate::actions::dispatch(
            &trigger,
            &crate::actions::Bindings::defaults(),
            &mut performer,
        );
        assert_eq!(outcome, crate::actions::Outcome::Sent("enter".into()));
    }

    #[test]
    fn encoder_modes() {
        let mut layout = layout_with("ACT06", slot("CODEX"));
        assert_eq!(
            resolve_event(&hid("ENC_CW", 2), &layout),
            Some(Trigger::Scroll(-1))
        );
        assert_eq!(
            resolve_event(&hid("ENC_CC", 2), &layout),
            Some(Trigger::Scroll(1))
        );
        // rotation keys never produce press/release; the encoder click is `ENC_CLK`
        assert_eq!(resolve_event(&hid("ENC_CW", 1), &layout), None);
        assert_eq!(
            resolve_event(&hid("ENC_CLK", 1), &layout),
            Some(Trigger::EncoderPress)
        );
        assert_eq!(
            resolve_event(&hid("ENC_CLK", 0), &layout),
            Some(Trigger::EncoderRelease)
        );

        layout.encoder_mode = EncoderMode::Custom;
        layout
            .encoder
            .insert("left".into(), Action::Command("archiveThread".into()));
        assert_eq!(
            resolve_event(&hid("ENC_CC", 2), &layout),
            Some(Trigger::EncoderTick(Action::Command(
                "archiveThread".into()
            )))
        );
        assert_eq!(
            resolve_event(&hid("ENC_CW", 2), &layout),
            None,
            "right turn is unbound in custom mode"
        );
    }

    #[test]
    fn stick_needs_distance_and_a_binding() {
        let mut layout = Layout::default();
        layout.analog_stick.insert(
            "up".into(),
            Action::Command("composer.increaseReasoningEffort".into()),
        );
        assert_eq!(resolve_stick(0.75, 0.49, &layout), None, "inside dead zone");
        assert_eq!(
            resolve_stick(0.75, 0.9, &layout),
            Some(Trigger::Stick(Action::Command(
                "composer.increaseReasoningEffort".into()
            ))),
            "0.75 of a turn is up"
        );
        layout.analog_stick.remove("down");
        assert_eq!(
            resolve_stick(0.25, 0.9, &layout),
            None,
            "an unbound direction does nothing"
        );
    }

    #[test]
    fn default_layout_is_the_apps_own() {
        let layout = Layout::default();
        assert_eq!(
            resolve_slot(&layout.slots["ACT06"]),
            Some(Action::Command("composer.toggleFastMode".into()))
        );
        assert_eq!(
            resolve_slot(&layout.slots["ACT12"]),
            Some(Action::Command("composer.submit".into()))
        );
        assert_eq!(
            resolve_slot(&layout.slots["ACT10_ACT11"]),
            Some(Action::PushToTalk)
        );
        assert_eq!(
            layout.analog_stick["up"],
            Action::Command("composer.togglePlanMode".into())
        );
        assert_eq!(
            layout.analog_stick["left"],
            Action::Command("navigateBack".into())
        );
        assert!(
            layout.encoder.is_empty(),
            "the knob starts unbound in custom mode"
        );
    }

    #[test]
    fn stick_direction_buckets() {
        // the vendor's own eighths, straight out of `lqs`
        assert_eq!(stick_direction(0.75), "up");
        assert_eq!(stick_direction(0.8), "up");
        assert_eq!(stick_direction(0.0), "right");
        assert_eq!(stick_direction(0.05), "right");
        assert_eq!(stick_direction(0.9), "right");
        assert_eq!(stick_direction(0.25), "down");
        assert_eq!(stick_direction(0.5), "left");
        assert_eq!(
            stick_direction(1.75),
            "up",
            "a wrap-around sample still lands in a bucket"
        );
    }
}
