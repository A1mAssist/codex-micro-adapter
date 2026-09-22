//! JSON-RPC 2.0 client for Work Louder devices.
//!
//! Ported from `WLRPCClient` + `WLDeviceCommImpl` in the vendor kit:
//!   * request envelope is `{method, params, id}` — there is **no** `jsonrpc`
//!     field and the abbreviated `{m, p}` form is rejected by firmware;
//!   * ids are integers in `[0, 999)` (firmware constraint);
//!   * non-ASCII characters are `\uXXXX`-escaped before framing;
//!   * one request is in flight at a time, with a 50 ms cooldown between calls;
//!   * a request times out after 10 s;
//!   * device-initiated notifications carry `method` (or compact `m`) and no `id`.

use crate::framing::{self, CHANNEL_RPC};
use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, Instant};

pub const REQUEST_TIMEOUT: Duration = Duration::from_millis(10_000);
pub const INTER_REQUEST_COOLDOWN: Duration = Duration::from_millis(50);
pub const MAX_RPC_ID: u16 = 999;

#[derive(Debug)]
pub enum RpcError {
    Transport(String),
    Timeout,
    Rpc { code: i64, message: String },
    Invalid(String),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::Transport(m) => write!(f, "transport: {m}"),
            RpcError::Timeout => write!(f, "timeout"),
            RpcError::Rpc { code, message } => write!(f, "rpc {code}: {message}"),
            RpcError::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}
impl std::error::Error for RpcError {}

/// Bytes in, bytes out. Implemented by the Windows HID transport and by mocks.
pub trait Hid {
    fn write_report(&mut self, report: &[u8; framing::REPORT_LEN]) -> std::io::Result<()>;
    /// Block until one report arrives, or `timeout` elapses.
    fn read_report(&mut self, timeout: Duration) -> Option<[u8; framing::REPORT_LEN]>;
    /// The transport is gone (unplugged): no report will ever arrive again.
    /// Mocks and in-memory transports are never closed.
    fn is_closed(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}

pub struct RpcClient<H: Hid> {
    hid: H,
    lines: framing::LineBuffers,
    next_id: u16,
    notifications: Vec<Notification>,
    /// Partially received JSON, exactly like the vendor's `rpcResponse` buffer.
    rpc_pending: String,
}

impl<H: Hid> RpcClient<H> {
    pub fn new(hid: H) -> Self {
        Self {
            hid,
            lines: framing::LineBuffers::default(),
            next_id: 0,
            notifications: Vec::new(),
            rpc_pending: String::new(),
        }
    }

    pub fn into_inner(self) -> H {
        self.hid
    }

    /// Notifications collected while waiting for responses.
    pub fn drain_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    /// The device went away under us.
    pub fn is_closed(&self) -> bool {
        self.hid.is_closed()
    }

    fn alloc_id(&mut self) -> u16 {
        self.next_id = (self.next_id + 1) % MAX_RPC_ID;
        self.next_id
    }

    /// Frame and write one request, then wait for its response.
    pub fn call<P: Serialize>(&mut self, method: &str, params: P) -> Result<Value, RpcError> {
        let id = self.alloc_id();
        // Built by hand so the key order matches the vendor's `{method, params, id}`
        // literal byte-for-byte; `serde_json::Value` would sort the keys.
        let params =
            serde_json::to_string(&params).map_err(|e| RpcError::Invalid(e.to_string()))?;
        let body = escape_non_ascii(&format!(
            "{{\"method\":{},\"params\":{},\"id\":{}}}",
            serde_json::to_string(method).map_err(|e| RpcError::Invalid(e.to_string()))?,
            params,
            id
        ));
        for report in framing::encode(CHANNEL_RPC, body.as_bytes()) {
            self.hid
                .write_report(&report)
                .map_err(|e| RpcError::Transport(e.to_string()))?;
        }

        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RpcError::Timeout);
            }
            let Some(report) = self.hid.read_report(remaining) else {
                return Err(RpcError::Timeout);
            };
            for line in self.lines.push(&report) {
                if let Some(value) = self.handle_line(&line, id)? {
                    // vendor cooldown: firmware needs time before the next command
                    std::thread::sleep(INTER_REQUEST_COOLDOWN);
                    return Ok(value);
                }
            }
        }
    }

    /// Read whatever the device has already sent, dispatching notifications.
    ///
    /// The vendor transport has a reader thread that parses reports continuously,
    /// which is how unsolicited notifications (key presses) arrive. Nothing is in
    /// flight during a pump, so any response seen is stale and gets dropped.
    ///
    /// Blocks for `timeout` only when nothing is available yet, then drains the
    /// rest without waiting.
    pub fn pump(&mut self, timeout: Duration) -> usize {
        let mut count = 0;
        loop {
            let wait = if count == 0 { timeout } else { Duration::ZERO };
            let Some(report) = self.hid.read_report(wait) else {
                break;
            };
            for line in self.lines.push(&report) {
                // u16::MAX is never a live request id, so responses are dropped
                // while notifications are kept.
                let _ = self.handle_line(&line, u16::MAX);
            }
            count += 1;
        }
        count
    }
    /// Mirrors the vendor's `parseRpcData`: keep buffering until the accumulated
    /// text parses as JSON, then either resolve our request or record a
    /// notification.
    ///
    /// The firmware may answer with compact keys (`i` / `m` / `p`) as well as the
    /// long ones, and numeric ids are compared as strings — exactly as upstream.
    fn handle_line(&mut self, line: &str, id: u16) -> Result<Option<Value>, RpcError> {
        if self.rpc_pending.is_empty() {
            match line.find('{') {
                Some(start) => self.rpc_pending = line[start..].to_string(),
                None => return Ok(None),
            }
        } else {
            self.rpc_pending.push_str(line);
        }

        // Incomplete JSON: keep the buffer and wait for more (vendor returns false).
        let Ok(value) = serde_json::from_str::<Value>(&self.rpc_pending) else {
            return Ok(None);
        };
        self.rpc_pending.clear();

        let obj = value
            .as_object()
            .ok_or_else(|| RpcError::Invalid(value.to_string()))?;
        let response_id = match obj.get("id").or_else(|| obj.get("i")) {
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let method = obj
            .get("method")
            .or_else(|| obj.get("m"))
            .and_then(|m| m.as_str())
            .map(str::to_string);

        match (response_id, method) {
            // "Received RPC call without id and method" — drop it.
            (None, None) => Ok(None),
            (None, Some(method)) => {
                let params = obj
                    .get("params")
                    .or_else(|| obj.get("p"))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.notifications.push(Notification { method, params });
                Ok(None)
            }
            (Some(response_id), _) => {
                if response_id != id.to_string() {
                    return Ok(None);
                }
                let has_result = obj.contains_key("result");
                let has_error = obj.contains_key("error");
                if has_result == has_error {
                    return Err(RpcError::Invalid(
                        "response has neither result nor error".into(),
                    ));
                }
                if has_error {
                    let err = obj.get("error").and_then(|v| v.as_object());
                    let code = err
                        .and_then(|e| e.get("code"))
                        .and_then(|c| c.as_i64())
                        .unwrap_or(0);
                    let message = err
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("device returned an error")
                        .to_string();
                    return Err(RpcError::Rpc { code, message });
                }
                Ok(Some(obj.get("result").cloned().unwrap_or(Value::Null)))
            }
        }
    }
}
/// `escapeUnicode` from the vendor kit: every non-ASCII char becomes `\uXXXX`.
pub fn escape_non_ascii(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut buf = [0u16; 2];
            for unit in ch.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// Scripted device: pops queued reports, records what we wrote.
    struct MockHid {
        incoming: Arc<Mutex<VecDeque<[u8; framing::REPORT_LEN]>>>,
        written: Arc<Mutex<Vec<[u8; framing::REPORT_LEN]>>>,
    }

    impl Hid for MockHid {
        fn write_report(&mut self, report: &[u8; framing::REPORT_LEN]) -> std::io::Result<()> {
            self.written.lock().unwrap().push(*report);
            Ok(())
        }
        fn read_report(&mut self, _timeout: Duration) -> Option<[u8; framing::REPORT_LEN]> {
            self.incoming.lock().unwrap().pop_front()
        }
    }

    fn client_with(lines: Vec<&str>) -> (RpcClient<MockHid>, Arc<Mutex<Vec<[u8; 64]>>>) {
        let incoming: VecDeque<_> = lines
            .iter()
            .flat_map(|l| framing::encode(CHANNEL_RPC, l.as_bytes()))
            .collect();
        let incoming = Arc::new(Mutex::new(incoming));
        let written = Arc::new(Mutex::new(Vec::new()));
        (
            RpcClient::new(MockHid {
                incoming,
                written: written.clone(),
            }),
            written,
        )
    }

    #[test]
    fn sends_vendor_request_shape_and_parses_result() {
        let (mut client, written) =
            client_with(vec!["{\"result\":{\"version\":\"1.0.37\"},\"id\":1}\n"]);
        let out = client.call("sys.version", Value::Null).unwrap();
        assert_eq!(out["version"], "1.0.37");

        let sent = written.lock().unwrap()[0];
        assert_eq!(sent[0], 0x06);
        assert_eq!(sent[1], CHANNEL_RPC);
        let body = std::str::from_utf8(&sent[3..3 + sent[2] as usize]).unwrap();
        assert_eq!(
            body,
            "{\"method\":\"sys.version\",\"params\":null,\"id\":1}"
        );
        assert!(
            !body.contains("jsonrpc"),
            "vendor envelope has no jsonrpc field"
        );
    }

    #[test]
    fn surfaces_device_error_objects() {
        let (mut client, _) = client_with(vec![
            "{\"error\":{\"code\":-32601,\"message\":\"method not found\"},\"id\":1}\n",
        ]);
        match client.call("nope", Value::Null) {
            Err(RpcError::Rpc { code, message }) => {
                assert_eq!(code, -32601);
                assert_eq!(message, "method not found");
            }
            other => panic!("expected rpc error, got {other:?}"),
        }
    }

    #[test]
    fn collects_notifications_that_are_not_the_response() {
        let (mut client, _) = client_with(vec![
            "{\"method\":\"v.oai.hid\",\"params\":{\"k\":\"ACT06\",\"act\":1}}\n",
            "{\"result\":null,\"id\":1}\n",
        ]);
        client.call("v.oai.rgbcfg", Value::Null).unwrap();
        let notes = client.drain_notifications();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].method, "v.oai.hid");
        assert_eq!(notes[0].params["k"], "ACT06");
    }

    #[test]
    fn accepts_compact_reply_keys() {
        // firmware may answer with `i` instead of `id`
        let (mut client, _) = client_with(vec!["{\"i\":1,\"result\":{\"version\":\"1.0.37\"}}\n"]);
        assert_eq!(
            client.call("sys.version", Value::Null).unwrap()["version"],
            "1.0.37"
        );
    }

    #[test]
    fn reassembles_json_split_over_several_lines() {
        let (mut client, _) = client_with(vec!["{\"result\":{\"a\":1},", "\"id\":1}\n"]);
        assert_eq!(client.call("device.status", Value::Null).unwrap()["a"], 1);
    }

    #[test]
    fn ignores_responses_for_other_ids() {
        let (mut client, _) = client_with(vec![
            "{\"result\":null,\"id\":7}\n",
            "{\"result\":{\"ok\":true},\"id\":1}\n",
        ]);
        assert_eq!(
            client.call("device.status", Value::Null).unwrap()["ok"],
            true
        );
    }
}
