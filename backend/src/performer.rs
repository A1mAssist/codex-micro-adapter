//! `Performer` implementations.
//!
//! The logging performer is the default for `run`; injecting keystrokes into
//! whatever happens to be focused is opt-in via `--live`.

use crate::actions::{Combo, Performer};

/// Prints what it would do and touches nothing.
#[derive(Debug, Default)]
pub struct LoggingPerformer;

impl Performer for LoggingPerformer {
    fn send_combo(&mut self, combo: &Combo) -> Result<(), String> {
        println!("[key ] {}", combo.label);
        Ok(())
    }
    fn type_text(&mut self, text: &str) -> Result<(), String> {
        println!("[type] {text}");
        Ok(())
    }
    fn open_url(&mut self, url: &str) -> Result<(), String> {
        println!("[url ] {url}");
        Ok(())
    }
}

#[cfg(windows)]
pub use windows_impl::WindowsPerformer;

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// Virtual keys for the four modifiers, in press order.
    const MODIFIER_VKS: [(fn(&crate::actions::Modifiers) -> bool, u16); 4] = [
        (|m| m.ctrl, 0x11),
        (|m| m.shift, 0x10),
        (|m| m.alt, 0x12),
        (|m| m.meta, 0x5B),
    ];

    /// Real keyboard injection through `SendInput`.
    #[derive(Debug, Default)]
    pub struct WindowsPerformer;

    fn key_event(vk: u16, scan: u16, flags: u32) -> INPUT {
        let mut input: INPUT = unsafe { std::mem::zeroed() };
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous = INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        };
        input
    }

    fn send(events: &[INPUT]) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let sent = unsafe {
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent == events.len() as u32 {
            Ok(())
        } else {
            Err(format!("SendInput sent {sent} of {} events", events.len()))
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl Performer for WindowsPerformer {
        fn send_combo(&mut self, combo: &Combo) -> Result<(), String> {
            let mut events = Vec::new();
            for (enabled, vk) in MODIFIER_VKS {
                if enabled(&combo.modifiers) {
                    events.push(key_event(vk, 0, 0));
                }
            }
            events.push(key_event(combo.vk, 0, 0));
            events.push(key_event(combo.vk, 0, KEYEVENTF_KEYUP));
            for (enabled, vk) in MODIFIER_VKS.into_iter().rev() {
                if enabled(&combo.modifiers) {
                    events.push(key_event(vk, 0, KEYEVENTF_KEYUP));
                }
            }
            send(&events)
        }

        fn type_text(&mut self, text: &str) -> Result<(), String> {
            // KEYEVENTF_UNICODE carries UTF-16 code units directly, so no keymap
            // or clipboard round trip is involved.
            for unit in text.encode_utf16() {
                let down = key_event(0, unit, KEYEVENTF_UNICODE);
                let up = key_event(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
                send(&[down, up])?;
            }
            Ok(())
        }

        fn open_url(&mut self, url: &str) -> Result<(), String> {
            let operation = wide("open");
            let target = wide(url);
            let result = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    operation.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    SW_SHOWNORMAL,
                )
            };
            if result as isize <= 32 {
                Err(format!(
                    "ShellExecuteW failed with code {}",
                    result as isize
                ))
            } else {
                Ok(())
            }
        }
    }

    // `c_void` is only referenced through the HWND null above; keep the import honest.
    #[allow(dead_code)]
    fn _assert_c_void(_: *mut c_void) {}
}
