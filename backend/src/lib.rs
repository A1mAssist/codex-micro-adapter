pub mod actions;
pub mod config;
pub mod control;
pub mod device;
pub mod framing;
pub mod host;
pub mod layout;
pub mod lighting;
pub mod oai;
pub mod performer;
pub mod rpc;

#[cfg(windows)]
pub mod hid_windows;

pub const VENDOR_ID: u16 = 0x303A;
pub const PID_CODEX_MICRO: u16 = 0x8360;
pub const PID_CREATOR_MICRO_V2: [u16; 2] = [0x8287, 0x8288];
pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;
