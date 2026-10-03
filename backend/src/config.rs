//! On-disk configuration.
//!
//! Mirrors the settings the ChatGPT app keeps for the device — the same names the
//! app writes into `config.toml` under `[desktop]`:
//!
//! ```text
//! codex-micro-lighting-brightness = 100      -> brightness_percent
//! codex-micro-lighting-auto-off   = "1-hour" -> auto_off
//! [desktop.codex-micro-layout]               -> layout
//! ```

use crate::actions::Bindings;
use crate::control::DEFAULT_PORT;
use crate::device::LightingModel;
use crate::layout::Layout;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// `codex-micro-lighting-brightness`, 0..=100.
    pub brightness_percent: u8,
    /// `codex-micro-lighting-auto-off`; `None` means never.
    pub auto_off: Option<String>,
    /// Where the host listens for harness state (`crate::control`).
    pub control_port: u16,
    /// Which client the UI is configured for. Only the user interface reads
    /// this — the control socket is harness-agnostic.
    pub harness: String,

    pub layout: Layout,
    pub bindings: Bindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            brightness_percent: 100,
            auto_off: Some("3-minutes".to_string()),
            control_port: DEFAULT_PORT,
            harness: "generic".to_string(),

            layout: Layout::default(),
            bindings: Bindings::defaults(),
        }
    }
}

impl Config {
    pub fn lighting(&self) -> LightingModel {
        LightingModel {
            brightness: (self.brightness_percent.min(100) as f32) / 100.0,
            inactivity_timeout: auto_off_to_timeout(self.auto_off.as_deref()),
        }
    }

    /// `%APPDATA%\codex-micro\config.json` on Windows.
    pub fn default_path() -> std::path::PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("codex-micro").join("config.json")
    }

    /// A missing file uses defaults; a damaged file remains available for repair.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let config: Self = serde_json::from_str(&text)
                    .map_err(|err| format!("could not parse {}: {err}", path.display()))?;
                config
                    .validate()
                    .map_err(|err| format!("invalid {}: {err}", path.display()))?;
                Ok(config)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(format!("could not read {}: {err}", path.display())),
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        for (key, binding) in &self.bindings.map {
            if crate::actions::parse_binding(binding).is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid binding for {key}: {binding}"),
                ));
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let temporary = path.with_extension(format!("{}.{}.tmp", std::process::id(), stamp));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&text)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

/// The app's auto-off vocabulary, from `codex-micro-settings`.
pub fn auto_off_to_timeout(value: Option<&str>) -> Option<Duration> {
    match value? {
        "30-seconds" => Some(Duration::from_secs(30)),
        "1-minute" => Some(Duration::from_secs(60)),
        "3-minutes" => Some(Duration::from_secs(180)),
        "10-minutes" => Some(Duration::from_secs(600)),
        "30-minutes" => Some(Duration::from_secs(1800)),
        "1-hour" => Some(Duration::from_secs(3600)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_off_vocabulary_matches_the_app() {
        assert_eq!(
            auto_off_to_timeout(Some("30-seconds")),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            auto_off_to_timeout(Some("1-hour")),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(auto_off_to_timeout(Some("off")), None);
        assert_eq!(auto_off_to_timeout(None), None);
    }

    #[test]
    fn defaults_mirror_the_app_settings() {
        let config = Config::default();
        assert_eq!(config.brightness_percent, 100);
        assert_eq!(config.lighting().brightness, 1.0);
        assert_eq!(
            config.lighting().inactivity_timeout,
            Some(Duration::from_secs(180))
        );
        assert_eq!(config.bindings.get("composer.submit"), Some("enter"));
    }

    #[test]
    fn round_trips_through_json() {
        let mut config = Config::default();
        config.brightness_percent = 40;
        config.layout.slots.insert(
            "ACT06".into(),
            crate::layout::SlotConfig {
                keycap_id: "GIT".into(),
                command_id: Some("git.commit".into()),
                action: None,
            },
        );
        config.bindings.set("git.commit", "ctrl+enter");
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.brightness_percent, 40);
        assert_eq!(back.layout.slots["ACT06"].keycap_id, "GIT");
        assert_eq!(back.bindings.get("git.commit"), Some("ctrl+enter"));
    }

    #[test]
    fn partial_json_falls_back_to_defaults_for_missing_fields() {
        let config: Config = serde_json::from_str("{\"brightnessPercent\":50}").unwrap();
        assert_eq!(config.brightness_percent, 50);
        assert_eq!(config.auto_off.as_deref(), Some("3-minutes"));
    }

    #[test]
    fn save_preserves_last_good_config_on_invalid_binding_and_reports_corruption() {
        let path = std::env::temp_dir().join(format!(
            "codex-micro-config-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = Config::default();
        config.save(&path).unwrap();
        config.brightness_percent = 40;
        config.save(&path).unwrap();
        config.bindings.set("ACT06", "hold:nosuchkey");
        assert_eq!(
            config.save(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(Config::load(&path).unwrap().brightness_percent, 40);

        std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(Config::load(&path).unwrap_err().contains("invalid binding"));

        std::fs::write(&path, b"{not json").unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
        std::fs::remove_file(path).unwrap();
    }
}
