//! Turning a [`Trigger`] into something another harness actually sees.
//!
//! The ChatGPT build performs commands through its own command registry
//! (`codex-micro-commands` filters the app's command table by platform). A daemon
//! has no such registry, so every command id is looked up in a user-editable
//! binding table instead:
//!
//! ```text
//! "composer.submit" -> "enter"
//! "git.commit"      -> "ctrl+enter"
//! "OAI"             -> "url:https://developers.openai.com"
//! "YOLO"            -> "type::yolo:"
//! ```
//!
//! Binding syntax is deliberately tiny: `mod+mod+key`, `type:<literal text>`, or
//! `url:<https url>`. Nothing here runs a shell — an action can only synthesise
//! keystrokes, type text, or open a URL.
//!
//! Defaults are intentionally almost empty: inventing keystrokes for ChatGPT-only
//! commands would be guessing at another tool's keymap. Unbound actions are
//! reported, not silently dropped.

use crate::layout::{Action, Trigger};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Modifier bits, mirroring the Win32 `MOD_*` / `KEYEVENTF_*` split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: Modifiers,
    /// Virtual key code (`VK_*`).
    pub vk: u16,
    /// What the user wrote, for logging.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Combo(Combo),
    Text(String),
    Url(String),
}

/// What actually happened, so the host can log or surface it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Sent(String),
    Unbound(String),
    Failed(String),
}

pub trait Performer {
    fn send_combo(&mut self, combo: &Combo) -> Result<(), String>;
    fn type_text(&mut self, text: &str) -> Result<(), String>;
    fn open_url(&mut self, url: &str) -> Result<(), String>;
}

/// Action key → binding string. Keys are action ids: a command id
/// (`composer.submit`), a layout action key (`keycap:YOLO`), `stick:up`,
/// `encoder:cw`, or `ptt`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bindings {
    pub map: BTreeMap<String, String>,
}

impl Bindings {
    /// The two mappings the vendor itself hard-codes, plus nothing else.
    pub fn defaults() -> Self {
        let mut map = BTreeMap::new();
        // `composer.submit` is the only ChatGPT command with an unambiguous
        // meaning in any prompt-based harness.
        map.insert("composer.submit".to_string(), "enter".to_string());
        Self { map }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: impl Into<String>, binding: impl Into<String>) {
        self.map.insert(key.into(), binding.into());
    }
}

/// The binding key for a trigger, or `None` when it carries its own payload
/// (composer text, external URL) and therefore needs no binding.
fn lookup_key(trigger: &Trigger) -> Option<String> {
    match trigger {
        Trigger::Act(Action::ComposerText { .. }) | Trigger::Act(Action::ExternalUrl { .. }) => {
            None
        }
        Trigger::Act(action)
        | Trigger::Stick(action)
        | Trigger::EncoderTick(action)
        | Trigger::EncoderClick(Some(action))
        | Trigger::EncoderLongPress(Some(action)) => Some(action_key(action)),
        Trigger::PushToTalk { .. } => Some("ptt".to_string()),
        Trigger::EncoderPress => Some("encoder:press".to_string()),
        Trigger::EncoderRelease => Some("encoder:release".to_string()),
        // the knob gestures the layout did not bind: a harness can bind these
        Trigger::EncoderClick(None) => Some("encoder:click".to_string()),
        Trigger::EncoderLongPress(None) => Some("encoder:longPress".to_string()),
        Trigger::Scroll(-1) => Some("encoder:up".to_string()),
        Trigger::Scroll(_) => Some("encoder:down".to_string()),
    }
}

/// Parse one binding string. Returns `None` for anything malformed.
pub fn parse_binding(binding: &str) -> Option<Step> {
    let binding = binding.trim();
    if let Some(text) = binding.strip_prefix("type:") {
        return Some(Step::Text(text.to_string()));
    }
    if let Some(url) = binding.strip_prefix("url:") {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return None;
        }
        return Some(Step::Url(url.to_string()));
    }
    parse_combo(binding)
}

fn parse_combo(binding: &str) -> Option<Step> {
    let mut modifiers = Modifiers::default();
    let mut parts = binding
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .peekable();
    loop {
        let part = parts.next()?;
        if parts.peek().is_none() {
            let vk = virtual_key(part)?;
            return Some(Step::Combo(Combo {
                modifiers,
                vk,
                label: binding.to_string(),
            }));
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" | "option" => modifiers.alt = true,
            "cmd" | "meta" | "super" | "win" => modifiers.meta = true,
            _ => return None,
        }
    }
}

/// Minimal `VK_*` table — the keys a keyboard macro realistically needs.
pub fn virtual_key(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "enter" | "return" => 0x0D,
        "escape" | "esc" => 0x1B,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" => 0x2D,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        _ => {
            if let Some(rest) = lower.strip_prefix('f') {
                if let Ok(n) = rest.parse::<u16>() {
                    if (1..=12).contains(&n) {
                        return Some(0x70 + n - 1);
                    }
                }
                return None;
            }
            let mut chars = lower.chars();
            let first = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            return match first {
                'a'..='z' => Some(0x41 + (first as u16 - 'a' as u16)),
                '0'..='9' => Some(0x30 + (first as u16 - '0' as u16)),
                _ => None,
            };
        }
    };
    Some(named)
}

/// Perform a trigger.
///
/// Encoder scroll needs no configuration: `Tt` in `codex-micro-bridge` maps CW to
/// the up arrow and CC to the down arrow, so that is the built-in fallback.
pub fn dispatch(trigger: &Trigger, bindings: &Bindings, performer: &mut dyn Performer) -> Outcome {
    // payload-carrying actions are already fully specified by the layout
    match trigger {
        Trigger::Act(Action::ComposerText { text, .. })
        | Trigger::EncoderClick(Some(Action::ComposerText { text, .. }))
        | Trigger::EncoderLongPress(Some(Action::ComposerText { text, .. })) => {
            let label = format!("type:{text}");
            return performer
                .type_text(text)
                .map_or_else(Outcome::Failed, |_| Outcome::Sent(label));
        }
        Trigger::Act(Action::ExternalUrl { url, .. })
        | Trigger::EncoderClick(Some(Action::ExternalUrl { url, .. }))
        | Trigger::EncoderLongPress(Some(Action::ExternalUrl { url, .. })) => {
            let label = format!("url:{url}");
            return performer
                .open_url(url)
                .map_or_else(Outcome::Failed, |_| Outcome::Sent(label));
        }
        _ => {}
    }

    let Some(key) = lookup_key(trigger) else {
        return Outcome::Unbound("?".to_string());
    };
    let fallback = match trigger {
        Trigger::Scroll(-1) => Some(Step::Combo(arrow(0x26, "up"))),
        Trigger::Scroll(_) => Some(Step::Combo(arrow(0x28, "down"))),
        _ => None,
    };

    let step = match bindings.get(&key) {
        Some(binding) => match parse_binding(binding) {
            Some(step) => step,
            None => return Outcome::Failed(format!("binding for {key} is malformed: {binding}")),
        },
        None => match fallback {
            Some(step) => step,
            None => return Outcome::Unbound(key),
        },
    };

    match step {
        Step::Combo(combo) => {
            let label = combo.label.clone();
            performer
                .send_combo(&combo)
                .map_or_else(Outcome::Failed, |_| Outcome::Sent(label))
        }
        Step::Text(text) => {
            let label = format!("type:{text}");
            performer
                .type_text(&text)
                .map_or_else(Outcome::Failed, |_| Outcome::Sent(label))
        }
        Step::Url(url) => {
            let label = format!("url:{url}");
            performer
                .open_url(&url)
                .map_or_else(Outcome::Failed, |_| Outcome::Sent(label))
        }
    }
}
fn arrow(vk: u16, label: &str) -> Combo {
    Combo {
        modifiers: Modifiers::default(),
        vk,
        label: label.to_string(),
    }
}

/// The binding key for an action coming out of the layout.
fn action_key(action: &Action) -> String {
    match action {
        Action::Command(id) => id.clone(),
        Action::ComposerText { text, .. } => format!("type:{text}"),
        Action::ExternalUrl { url, .. } => format!("url:{url}"),
        Action::Skill { skill_name } => format!("skill:{skill_name}"),
        Action::PushToTalk => "ptt".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recording {
        combos: Vec<String>,
        texts: Vec<String>,
        urls: Vec<String>,
    }

    impl Performer for Recording {
        fn send_combo(&mut self, combo: &Combo) -> Result<(), String> {
            self.combos.push(combo.label.clone());
            Ok(())
        }
        fn type_text(&mut self, text: &str) -> Result<(), String> {
            self.texts.push(text.to_string());
            Ok(())
        }
        fn open_url(&mut self, url: &str) -> Result<(), String> {
            self.urls.push(url.to_string());
            Ok(())
        }
    }

    #[test]
    fn parses_combos_and_modifiers() {
        let Some(Step::Combo(combo)) = parse_binding("ctrl+shift+p") else {
            panic!("no combo")
        };
        assert!(combo.modifiers.ctrl && combo.modifiers.shift && !combo.modifiers.alt);
        assert_eq!(combo.vk, 0x50);
        let Some(Step::Combo(combo)) = parse_binding("enter") else {
            panic!("no combo")
        };
        assert_eq!(combo.vk, 0x0D);
        let Some(Step::Combo(combo)) = parse_binding("f5") else {
            panic!("no combo")
        };
        assert_eq!(combo.vk, 0x74);
    }

    #[test]
    fn rejects_malformed_bindings() {
        assert!(parse_binding("ctrl+").is_none());
        assert!(parse_binding("ctrl+nosuchkey").is_none());
        assert!(
            parse_binding("url:file:///etc/passwd").is_none(),
            "only http(s) urls"
        );
    }

    #[test]
    fn default_binding_sends_enter_for_submit() {
        let bindings = Bindings::defaults();
        let mut performer = Recording::default();
        let outcome = dispatch(
            &Trigger::Act(Action::Command("composer.submit".into())),
            &bindings,
            &mut performer,
        );
        assert_eq!(outcome, Outcome::Sent("enter".into()));
        assert_eq!(performer.combos, vec!["enter"]);
    }

    #[test]
    fn unbound_command_is_reported_not_swallowed() {
        let mut performer = Recording::default();
        let outcome = dispatch(
            &Trigger::Act(Action::Command("git.commit".into())),
            &Bindings::defaults(),
            &mut performer,
        );
        assert_eq!(outcome, Outcome::Unbound("git.commit".into()));
        assert!(performer.combos.is_empty());
    }

    #[test]
    fn scroll_uses_the_vendors_arrow_mapping_without_config() {
        let mut performer = Recording::default();
        assert_eq!(
            dispatch(&Trigger::Scroll(-1), &Bindings::default(), &mut performer),
            Outcome::Sent("up".into())
        );
        assert_eq!(
            dispatch(&Trigger::Scroll(1), &Bindings::default(), &mut performer),
            Outcome::Sent("down".into())
        );
        assert_eq!(performer.combos, vec!["up", "down"]);
    }

    #[test]
    fn typing_and_urls_go_through() {
        let bindings = Bindings::default();
        let mut performer = Recording::default();
        let outcome = dispatch(
            &Trigger::Act(Action::ComposerText {
                label: "Write :yolo:".into(),
                text: ":yolo:".into(),
            }),
            &bindings,
            &mut performer,
        );
        assert_eq!(outcome, Outcome::Sent("type::yolo:".into()));
        assert_eq!(performer.texts, vec![":yolo:"]);

        let outcome = dispatch(
            &Trigger::Act(Action::ExternalUrl {
                label: "docs".into(),
                url: "https://developers.openai.com".into(),
            }),
            &Bindings::default(),
            &mut performer,
        );
        assert_eq!(
            outcome,
            Outcome::Sent("url:https://developers.openai.com".into())
        );
        assert_eq!(performer.urls, vec!["https://developers.openai.com"]);
    }

    #[test]
    fn knob_gestures_fall_back_to_their_own_binding_keys() {
        let mut performer = Recording::default();
        let mut bindings = Bindings::default();
        bindings.set("encoder:click", "ctrl+enter");
        bindings.set("encoder:longPress", "type:hold");
        assert_eq!(
            dispatch(&Trigger::EncoderClick(None), &bindings, &mut performer),
            Outcome::Sent("ctrl+enter".into())
        );
        assert_eq!(
            dispatch(&Trigger::EncoderLongPress(None), &bindings, &mut performer),
            Outcome::Sent("type:hold".into())
        );
        assert_eq!(performer.texts, vec!["hold"]);
        // unbound, they are reported like every other action
        assert_eq!(
            dispatch(&Trigger::EncoderClick(None), &Bindings::default(), &mut performer),
            Outcome::Unbound("encoder:click".into())
        );
    }

    #[test]
    fn a_gesture_the_layout_bound_runs_that_action() {
        let mut performer = Recording::default();
        let mut bindings = Bindings::default();
        bindings.set("git.commit", "ctrl+enter");
        assert_eq!(
            dispatch(
                &Trigger::EncoderClick(Some(Action::Command("git.commit".into()))),
                &bindings,
                &mut performer,
            ),
            Outcome::Sent("ctrl+enter".into())
        );
        // and one that carries its own payload needs no binding at all
        assert_eq!(
            dispatch(
                &Trigger::EncoderLongPress(Some(Action::ComposerText {
                    label: "Write :yolo:".into(),
                    text: ":yolo:".into(),
                })),
                &Bindings::default(),
                &mut performer,
            ),
            Outcome::Sent("type::yolo:".into())
        );
    }
}
