//! Work Louder HID framing, ported byte-for-byte from the vendor kit.
//!
//! Source of truth: `@worklouder/wl-device-kit` → `wl_device_comm_impl.ts`
//! (`sendDataHID` / `parseHIDReport` / `parseHIDdata`).
//!
//! Outgoing 64-byte report:
//!   [0] = 0x06 report id
//!   [1] = channel (1 = debug log, 2 = RPC)
//!   [2] = payload length
//!   [3..] = payload, at most 61 bytes; longer messages are split
//!
//! Incoming reports are demultiplexed by channel and accumulated per channel;
//! complete lines are split on `\r?\n`.

pub const REPORT_ID: u8 = 0x06;
pub const REPORT_LEN: usize = 64;
pub const MAX_CHUNK: usize = REPORT_LEN - 3; // 61
pub const CHANNEL_DEBUG: u8 = 1;
pub const CHANNEL_RPC: u8 = 2;

/// Split `payload` into 64-byte HID reports for `channel`.
///
/// An empty payload produces one empty report (the vendor loop is
/// `while offset < len`, so an empty message would in fact send nothing; we
/// mirror that by returning an empty vec).
pub fn encode(channel: u8, payload: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < payload.len() {
        let chunk = MAX_CHUNK.min(payload.len() - offset);
        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        report[1] = channel;
        report[2] = chunk as u8;
        report[3..3 + chunk].copy_from_slice(&payload[offset..offset + chunk]);
        out.push(report);
        offset += chunk;
    }
    out
}

/// Extract `(channel, payload)` from a raw report.
pub fn decode(report: &[u8]) -> Option<(u8, &[u8])> {
    if report.len() < 3 {
        return None;
    }
    let channel = report[1];
    let len = report[2] as usize;
    let end = (3 + len).min(report.len());
    Some((channel, &report[3..end]))
}

/// Per-channel line splitter, matching `parseHIDdata`.
#[derive(Default)]
pub struct LineBuffers {
    debug: String,
    rpc: String,
}

impl LineBuffers {
    /// Feed one report; returns the complete RPC lines it completed.
    pub fn push(&mut self, report: &[u8]) -> Vec<String> {
        let Some((channel, payload)) = decode(report) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(payload).to_string();
        let buf = if channel == CHANNEL_DEBUG {
            &mut self.debug
        } else {
            &mut self.rpc
        };
        buf.push_str(&text);

        let mut lines: Vec<&str> = buf.split('\n').collect();
        let tail = lines.pop().unwrap_or("").trim_end_matches('\r').to_string();
        let complete: Vec<String> = lines
            .into_iter()
            .map(|l| l.trim().trim_end_matches('\r').to_string())
            .filter(|l| !l.is_empty())
            .collect();
        *buf = tail;

        if channel == CHANNEL_RPC {
            complete
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_single_report() {
        let reports = encode(CHANNEL_RPC, b"{\"id\":1}");
        assert_eq!(reports.len(), 1);
        let r = &reports[0];
        assert_eq!(r.len(), 64);
        assert_eq!(r[0], 0x06);
        assert_eq!(r[1], 2);
        assert_eq!(r[2], 8);
        assert_eq!(&r[3..11], b"{\"id\":1}");
        assert!(
            r[11..].iter().all(|b| *b == 0),
            "rest of report is zero padded"
        );
    }

    #[test]
    fn splits_at_61_bytes() {
        let payload = vec![b'x'; 122];
        let reports = encode(CHANNEL_RPC, &payload);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0][2], 61);
        assert_eq!(reports[1][2], 61);
        let payload = vec![b'x'; 62];
        let reports = encode(CHANNEL_RPC, &payload);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[1][2], 1);
    }

    #[test]
    fn round_trips_through_line_splitter() {
        // device sends one json line split over two reports
        let msg = "{\"method\":\"v.oai.hid\",\"params\":{\"k\":\"ACT06\",\"act\":1}}\n";
        let (a, b) = msg.as_bytes().split_at(30);
        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        report[1] = CHANNEL_RPC;
        report[2] = a.len() as u8;
        report[3..3 + a.len()].copy_from_slice(a);
        let mut report2 = [0u8; REPORT_LEN];
        report2[0] = REPORT_ID;
        report2[1] = CHANNEL_RPC;
        report2[2] = b.len() as u8;
        report2[3..3 + b.len()].copy_from_slice(b);

        let mut bufs = LineBuffers::default();
        assert!(bufs.push(&report).is_empty());
        let lines = bufs.push(&report2);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("{\"method\":\"v.oai.hid\""));
    }
}
