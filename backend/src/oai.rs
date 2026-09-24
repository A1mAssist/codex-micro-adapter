//! The OpenAI-flavoured device API.
//!
//! Ported from `@worklouder/device-kit-oai` (`RPCApiOAI`): two vendor-specific
//! JSON-RPC methods plus the notification channels the firmware emits.
//!
//! ```text
//! v.oai.thstatus   params: [{id, c, b, e, s, sk, sa}]   per-thread accent lighting
//! v.oai.rgbcfg     params: {ambient:{e,b,s,m,c}, keys:{e,b,s,m,c}}
//! v.oai.hid        notification: {k, act, ag}           key / encoder events
//! v.oai.rad        notification: {a, d}                 joystick angle + distance
//! ```

use serde::{Deserialize, Serialize};

/// `VendorJsonRpcMethods.ThreadsLighting`
pub const METHOD_THREADS_LIGHTING: &str = "v.oai.thstatus";
/// `VendorJsonRpcMethods.RgbConfig`
pub const METHOD_RGB_CONFIG: &str = "v.oai.rgbcfg";
/// `NotifyKeys.hidReceived`
pub const NOTIFY_HID: &str = "v.oai.hid";
/// `NotifyKeys.joystickMove`
pub const NOTIFY_JOYSTICK: &str = "v.oai.rad";

/// Base-kit method used for the live "preview my lighting" button.
pub const METHOD_LIGHTS_PREVIEW: &str = "lights.preview";
/// Base-kit method used for the status snapshot (battery, profile, layer).
pub const METHOD_DEVICE_STATUS: &str = "device.status";
/// Base-kit method used for the firmware version string.
pub const METHOD_SYS_VERSION: &str = "sys.version";

/// Built-in firmware LED effects (`OAILightingEffect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LightingEffect {
    Off,
    Solid,
    Snake,
    Rainbow,
    Breath,
    Gradient,
    #[serde(rename = "shallowBreath")]
    ShallowBreath,
}

impl LightingEffect {
    /// Firmware wire value.
    pub fn code(self) -> u8 {
        match self {
            LightingEffect::Off => 0,
            LightingEffect::Solid => 1,
            LightingEffect::Snake => 2,
            LightingEffect::Rainbow => 3,
            LightingEffect::Breath => 4,
            LightingEffect::Gradient => 5,
            LightingEffect::ShallowBreath => 6,
        }
    }
}

/// One side (keys or ambient ring) of `v.oai.rgbcfg`.
///
/// The vendor minimiser renames every field to a single letter, so the struct
/// serialises straight to the wire shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LightingSide {
    /// effect
    #[serde(rename = "e")]
    pub effect: u8,
    /// brightness, 0..=1
    #[serde(rename = "b")]
    pub brightness: f32,
    /// speed, 0..=1
    #[serde(rename = "s")]
    pub speed: f32,
    /// magic / effect-specific parameter
    #[serde(rename = "m")]
    pub magic: u8,
    /// packed colour
    #[serde(rename = "c")]
    pub color: u32,
}

/// Payload for `v.oai.rgbcfg`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LightingConfig {
    pub ambient: LightingSide,
    pub keys: LightingSide,
}

/// One zone of `lights.preview`.
///
/// Note this method uses the **long** field names, unlike the compact
/// `v.oai.rgbcfg` payload — `sendLightingPreview` forwards the config verbatim.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PreviewSide {
    pub effect: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magic: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

/// Payload for `lights.preview` (`WLDeviceLightingConfig`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PreviewConfig {
    pub backlight: PreviewSide,
    pub underglow: PreviewSide,
}

/// One entry of `v.oai.thstatus`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ThreadLighting {
    pub id: u32,
    #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f32>,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    pub effect: Option<u8>,
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    /// syncKeysLighting, sent as 0/1
    #[serde(rename = "sk", skip_serializing_if = "Option::is_none")]
    pub sync_keys: Option<u8>,
    /// syncAmbientLighting, sent as 0/1
    #[serde(rename = "sa", skip_serializing_if = "Option::is_none")]
    pub sync_ambient: Option<u8>,
}

/// Key / encoder event from `v.oai.hid`.
#[derive(Debug, Clone, Deserialize)]
pub struct HidEvent {
    /// key id, e.g. `ACT06` / `AG02` / `ENC_CW`
    #[serde(rename = "k")]
    pub key: String,
    /// action: 0 = release, 1 = press, 2 = rotate tick
    #[serde(rename = "act")]
    pub act: u8,
    /// agent group, if the key is an agent key
    #[serde(rename = "ag", default)]
    pub agent_group: Option<u8>,
}

/// Joystick event from `v.oai.rad`.
#[derive(Debug, Clone, Deserialize)]
pub struct JoystickEvent {
    /// Angle as a fraction of a turn: 0 right, 0.25 down, 0.5 left, 0.75 up.
    #[serde(rename = "a")]
    pub angle: f32,
    /// Distance from centre, 0..=1. The device rests near 0.5, so the vendor
    /// halves the travel before it reads a direction at all.
    #[serde(rename = "d")]
    pub distance: f32,
}

/// Runtime snapshot returned by `device.status`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeviceStatus {
    #[serde(rename = "firmwareVersion", default)]
    pub firmware_version: Option<String>,
    #[serde(rename = "selectedProfileIndex", default)]
    pub profile_index: Option<u32>,
    #[serde(rename = "selectedLayerIndex", default)]
    pub layer_index: Option<u32>,
    #[serde(rename = "batteryPercentage", default)]
    pub battery_percentage: Option<u32>,
    #[serde(rename = "isCharging", default)]
    pub is_charging: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_config_matches_wire_shape() {
        let cfg = LightingConfig {
            ambient: LightingSide {
                effect: 1,
                brightness: 1.0,
                speed: 0.0,
                magic: 0,
                color: 0xff0000,
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(
            json,
            "{\"ambient\":{\"e\":1,\"b\":1.0,\"s\":0.0,\"m\":0,\"c\":16711680},\"keys\":{\"e\":0,\"b\":0.0,\"s\":0.0,\"m\":0,\"c\":0}}"
        );
    }

    #[test]
    fn thread_lighting_omits_unset_fields() {
        let t = ThreadLighting {
            id: 7,
            brightness: Some(0.5),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&t).unwrap(), "{\"id\":7,\"b\":0.5}");
    }

    #[test]
    fn parses_hid_notification_params() {
        let e: HidEvent = serde_json::from_str("{\"k\":\"ENC_CW\",\"act\":2}").unwrap();
        assert_eq!(e.key, "ENC_CW");
        assert_eq!(e.act, 2);
        assert!(e.agent_group.is_none());
    }
}
