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

/// The window that has focus right now, as an opaque handle. The host stores it
/// while a session reports activity, so an agent key can bring it back later.
#[cfg(windows)]
pub fn foreground_window() -> Option<isize> {
    windows_impl::foreground_window()
}

#[cfg(not(windows))]
pub fn foreground_window() -> Option<isize> {
    None
}

/// Bring a stored window back to the front.
#[cfg(windows)]
pub fn focus_window(hwnd: isize) -> Result<(), String> {
    windows_impl::focus_window(hwnd)
}

#[cfg(not(windows))]
pub fn focus_window(_hwnd: isize) -> Result<(), String> {
    Err("window focusing is Windows-only for now".to_string())
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsIconic, IsWindow, SetForegroundWindow, ShowWindow, SW_RESTORE,
        SW_SHOWNORMAL,
    };

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

    /// The window the user is looking at right now. Windows hands back a null
    /// handle when nothing has focus, which a session can never be.
    pub fn foreground_window() -> Option<isize> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then_some(hwnd as isize)
    }

    /// Put a stored window back in front, restoring it first when minimised.
    pub fn focus_window(hwnd: isize) -> Result<(), String> {
        let hwnd = hwnd as HWND;
        unsafe {
            if IsWindow(hwnd) == 0 {
                return Err("that window is gone".to_string());
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            if SetForegroundWindow(hwnd) == 0 {
                return Err("Windows refused the focus change".to_string());
            }
        }
        Ok(())
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
