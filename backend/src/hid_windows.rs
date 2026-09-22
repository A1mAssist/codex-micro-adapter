//! Windows HID transport for Work Louder devices (pure Rust, no C dependency).
//!
//! Discovery mirrors `WLDeviceDiscovery` / the vendor's `hid-topology-watcher`
//! native addon: walk the HID device interfaces, keep the ones whose vendor id
//! is `0x303A`, whose product id is in the device registry, and whose top-level
//! collection sits on usage page `0xFF00`.
//!
//! I/O matches `WLDeviceCommImpl.sendDataHID`: 64-byte reports whose first byte
//! is the report id (`0x06`) and whose second byte is the channel.

use crate::rpc::Hid as HidIo;
use crate::{PID_CODEX_MICRO, PID_CREATOR_MICRO_V2, VENDOR_ID, VENDOR_USAGE_PAGE};
use std::io;
use std::mem::size_of;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData,
    HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

pub const REPORT_LEN: usize = 64;

/// `SetupDiGetClassDevsW` returns this on failure (`HDEVINFO` is a plain isize).
const INVALID_HDEVINFO: isize = -1;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// `true` for the Codex Micro, `false` for a Creator Micro V2.
    pub is_codex_micro: bool,
    /// Wired vs wireless. ponytail: HID exposes no dependable flag for this, and
    /// the only consumer is a UI label, so we report wired.
    pub is_usb: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The interface path is a flexible array member: windows-sys declares it as
/// `[u16; 1]`, so reading `detail.DevicePath` yields one character (`\`) and
/// every `CreateFileW` on the result fails. Read the UTF-16 path out of the raw
/// buffer instead, at the offset `repr(C)` gives the field.
fn device_path(buf: &[u8]) -> String {
    let start = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
    let units: Vec<u16> = buf[start..]
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Enumerate Work Louder HID interfaces.
pub fn enumerate() -> Vec<HidDeviceInfo> {
    enumerate_scanned().1
}

/// `(number of HID interfaces present, Work Louder devices among them)`.
pub fn enumerate_scanned() -> (usize, Vec<HidDeviceInfo>) {
    let mut out = Vec::new();
    let mut scanned = 0usize;
    unsafe {
        let mut guid: GUID = std::mem::zeroed();
        HidD_GetHidGuid(&mut guid);

        let set = SetupDiGetClassDevsW(
            &guid,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if set == INVALID_HDEVINFO {
            return (scanned, out);
        }

        let mut index = 0u32;
        loop {
            let mut iface: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
            iface.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &guid, index, &mut iface) == 0 {
                break;
            }
            index += 1;

            let mut needed = 0u32;
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                std::ptr::null_mut(),
                0,
                &mut needed,
                std::ptr::null_mut(),
            );
            if needed == 0 {
                continue;
            }
            let mut buf = vec![0u8; needed as usize];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                detail,
                needed,
                &mut needed,
                std::ptr::null_mut(),
            ) == 0
            {
                continue;
            }
            scanned += 1;
            let path = device_path(&buf);
            if let Some(info) = describe(&path) {
                out.push(info);
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    (scanned, out)
}

/// Open `path` read-only to read its attributes/caps; returns `None` unless it
/// is a Work Louder interface on the vendor collection.
fn describe(path: &str) -> Option<HidDeviceInfo> {
    unsafe {
        let wpath = wide(path);
        let handle = CreateFileW(
            wpath.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut attrs: HIDD_ATTRIBUTES = std::mem::zeroed();
        attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        let mut preparsed: isize = 0;
        let mut caps: HIDP_CAPS = std::mem::zeroed();
        let ok = HidD_GetAttributes(handle, &mut attrs)
            && HidD_GetPreparsedData(handle, &mut preparsed)
            && HidP_GetCaps(preparsed, &mut caps) >= 0;
        if preparsed != 0 {
            HidD_FreePreparsedData(preparsed);
        }
        CloseHandle(handle);
        if !ok {
            return None;
        }

        let product_id = attrs.ProductID;
        let is_codex_micro = product_id == PID_CODEX_MICRO;
        if attrs.VendorID != VENDOR_ID
            || !(is_codex_micro || PID_CREATOR_MICRO_V2.contains(&product_id))
            || caps.UsagePage != VENDOR_USAGE_PAGE
        {
            return None;
        }

        Some(HidDeviceInfo {
            path: path.to_string(),
            vendor_id: attrs.VendorID,
            product_id,
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            is_codex_micro,
            is_usb: true,
        })
    }
}

/// Prefer a Codex Micro; otherwise the first known device.
pub fn find_codex_micro() -> Option<HidDeviceInfo> {
    let all = enumerate();
    all.iter()
        .find(|d| d.is_codex_micro)
        .cloned()
        .or_else(|| all.into_iter().next())
}

/// `HANDLE` is a raw pointer, which is neither `Send` nor `Sync`. The HID handle
/// is safe to share: writes happen on the caller's thread while a single reader
/// thread blocks in `ReadFile` — the same split `hidapi` uses.
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

impl SendHandle {
    /// Method (not a field access) so closures capture the whole `SendHandle`
    /// instead of the raw pointer inside it.
    fn raw(&self) -> HANDLE {
        self.0
    }
}

/// Open a device for RPC traffic.
///
/// A background thread blocks in `ReadFile` and forwards reports over a channel,
/// which is what lets `read_report` honour a timeout without overlapped I/O.
pub struct WindowsHid {
    handle: SendHandle,
    rx: Receiver<[u8; REPORT_LEN]>,
    /// Set the moment the reader thread stops, so the host can tell "the device
    /// went away" apart from "nothing has arrived yet" without waiting for an
    /// RPC timeout.
    closed: Closed,
}

type Closed = std::sync::Arc<std::sync::atomic::AtomicBool>;

fn spawn_reader(handle: HANDLE) -> io::Result<(Receiver<[u8; REPORT_LEN]>, Closed)> {
    let (tx, rx): (Sender<[u8; REPORT_LEN]>, Receiver<[u8; REPORT_LEN]>) = mpsc::channel();
    let closed: Closed = Closed::default();
    let flag = closed.clone();
    let owned = SendHandle(handle);
    std::thread::Builder::new()
        .name("wl-hid-read".into())
        .spawn(move || {
            loop {
                let mut buf = [0u8; REPORT_LEN];
                let mut read = 0u32;
                let ok = unsafe {
                    ReadFile(
                        owned.raw(),
                        buf.as_mut_ptr(),
                        REPORT_LEN as u32,
                        &mut read,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 || read == 0 {
                    break;
                }
                if tx.send(buf).is_err() {
                    break;
                }
            }
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        })?;
    Ok((rx, closed))
}

impl WindowsHid {
    pub fn open(path: &str) -> io::Result<Self> {
        unsafe {
            let wpath = wide(path);
            let handle = CreateFileW(
                wpath.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let (rx, closed) = spawn_reader(handle)?;
            Ok(Self {
                handle: SendHandle(handle),
                rx,
                closed,
            })
        }
    }
}

impl Drop for WindowsHid {
    fn drop(&mut self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        unsafe { CloseHandle(self.handle.0) };
    }
}

impl HidIo for WindowsHid {
    fn write_report(&mut self, report: &[u8; REPORT_LEN]) -> io::Result<()> {
        unsafe {
            let mut written = 0u32;
            let ok = WriteFile(
                self.handle.0,
                report.as_ptr(),
                REPORT_LEN as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    fn read_report(&mut self, timeout: Duration) -> Option<[u8; REPORT_LEN]> {
        self.rx.recv_timeout(timeout).ok()
    }

    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The opener both front ends use.
pub struct Opener;

impl crate::device::Opener for Opener {
    type Hid = WindowsHid;

    fn open(&mut self, path: &str) -> Result<Self::Hid, String> {
        WindowsHid::open(path).map_err(|e| e.to_string())
    }
}

/// What to connect to, if a Codex Micro is plugged in.
pub fn scan() -> Option<crate::device::Candidate> {
    find_codex_micro().map(|info| crate::device::Candidate {
        path: info.path,
        is_usb: info.is_usb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SP_DEVICE_INTERFACE_DETAIL_DATA_W::DevicePath` is a flexible array member
    /// that windows-sys declares as `[u16; 1]`. Reading that field gives back one
    /// character, which is what silently broke every `CreateFileW` before; the
    /// path has to come out of the raw buffer at the field's offset.
    #[test]
    fn reads_the_whole_interface_path_out_of_the_detail_buffer() {
        const PATH: &str = r"\\?\hid#vid_303a&pid_8360#7&2f4d1a1c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        let start = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
        let bytes = start + 2 * (PATH.len() + 1);
        // 8-byte aligned, so the struct view below is a real one like SetupAPI's
        let mut words = vec![0u64; bytes.div_ceil(8)];
        let buf =
            unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 8) };
        buf[..4].copy_from_slice(&(size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32).to_ne_bytes());
        for (i, unit) in PATH.encode_utf16().chain(std::iter::once(0)).enumerate() {
            let at = start + i * 2;
            buf[at..at + 2].copy_from_slice(&unit.to_ne_bytes());
        }
        assert_eq!(device_path(&buf[..bytes]), PATH);
        // reading the declared field instead is the trap: one unit, not the path
        let detail = unsafe { &*words.as_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() };
        assert_eq!(String::from_utf16_lossy(&detail.DevicePath), "\\");
    }
}
