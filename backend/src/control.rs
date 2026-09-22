//! Loopback control socket: how another harness drives the device.
//!
//! The ChatGPT app reads its agent keys from its own thread list. A
//! harness-agnostic host cannot know that, so the state arrives over a loopback
//! socket as newline-delimited commands and gets applied on the next tick:
//!
//! ```text
//! agent 0 working          # key 0 shows the working colour
//! agent 0 off
//! fleet awaiting-approval  # one status across the whole ring
//! fleet off
//! voice recording          # ambient ring, push-to-talk / dictation
//! brightness 80            # 0..=100
//! selection on             # keys echo the ambient colour
//! ping
//! ```
//!
//! The reply is a single line: `ok`, or `err: ...`.

use crate::lighting::{SlotStatus, VoiceState};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Fixed so a hook can be a one-liner.
pub const DEFAULT_PORT: u16 = 27700;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Agent {
        index: usize,
        status: SlotStatus,
    },
    /// `None` clears the fleet-wide status.
    Fleet(Option<SlotStatus>),
    Voice(VoiceState),
    Brightness(u8),
    Selection(bool),
    Ping,
}

/// Parse one command line. The vocabulary matches the enums, kebab-cased.
pub fn parse(line: &str) -> Result<Command, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty command".to_string());
    }
    let mut words = trimmed.split_whitespace();
    let verb = words.next().unwrap_or_default();
    let args: Vec<&str> = words.collect();
    match verb {
        "ping" => Ok(Command::Ping),
        "agent" => {
            let [index, status] = args.as_slice() else {
                return Err("usage: agent <0-5> <status>".to_string());
            };
            let index = index
                .parse::<usize>()
                .map_err(|_| format!("bad agent index: {index}"))?;
            Ok(Command::Agent {
                index,
                status: enum_value(status)?,
            })
        }
        "fleet" => match args.as_slice() {
            ["off"] | [] => Ok(Command::Fleet(None)),
            [status] => Ok(Command::Fleet(Some(enum_value(status)?))),
            _ => Err("usage: fleet <status|off>".to_string()),
        },
        "voice" => match args.as_slice() {
            [state] => Ok(Command::Voice(enum_value(state)?)),
            _ => Err("usage: voice <idle|recording|processing|completed>".to_string()),
        },
        "brightness" => match args.as_slice() {
            [percent] => {
                let percent = percent
                    .parse::<u8>()
                    .map_err(|_| format!("bad brightness: {percent}"))?;
                if percent > 100 {
                    return Err(format!("brightness out of range: {percent}"));
                }
                Ok(Command::Brightness(percent))
            }
            _ => Err("usage: brightness <0-100>".to_string()),
        },
        "selection" => match args.as_slice() {
            ["on"] | ["true"] => Ok(Command::Selection(true)),
            ["off"] | ["false"] => Ok(Command::Selection(false)),
            _ => Err("usage: selection <on|off>".to_string()),
        },
        other => Err(format!("unknown command: {other}")),
    }
}

/// Deserialise an enum from its kebab-case name, so the socket vocabulary can
/// never drift from the types.
fn enum_value<T: serde::de::DeserializeOwned>(token: &str) -> Result<T, String> {
    serde_json::from_value::<T>(serde_json::Value::String(token.to_string()))
        .map_err(|_| format!("unknown value: {token}"))
}

/// Commands waiting for the device loop to pick up.
#[derive(Clone, Default)]
pub struct Queue(Arc<Mutex<Vec<Command>>>);

impl Queue {
    pub fn push(&self, command: Command) {
        self.0.lock().unwrap().push(command);
    }

    pub fn drain(&self) -> Vec<Command> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Bind the control port and hand every parsed command to `queue`.
///
/// `port` of 0 asks the OS for a free port; the bound port is returned, which is
/// what the tests use so they never collide with a running app.
pub fn serve(port: u16, queue: Queue) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let bound = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let queue = queue.clone();
            std::thread::spawn(move || handle(stream, queue));
        }
    });
    Ok(bound)
}

fn handle(stream: TcpStream, queue: Queue) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = match parse(&line) {
        Ok(command) => {
            queue.push(command);
            "ok".to_string()
        }
        Err(err) => format!("err: {err}"),
    };
    let _ = writer.write_all(format!("{reply}\n").as_bytes());
    let _ = writer.flush();
}

/// One-shot client: what `codex-micro-backend send <command>` uses, and what a
/// harness uses when shelling out is easier than opening a socket.
pub fn send(port: u16, line: &str) -> Result<String, String> {
    let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writer
        .write_all(line.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    Ok(reply.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_command_vocabulary() {
        assert_eq!(parse("ping"), Ok(Command::Ping));
        assert_eq!(
            parse("agent 3 working"),
            Ok(Command::Agent {
                index: 3,
                status: SlotStatus::Working
            })
        );
        assert_eq!(
            parse("agent 1 off"),
            Ok(Command::Agent {
                index: 1,
                status: SlotStatus::Off
            })
        );
        assert_eq!(
            parse("fleet awaiting-approval"),
            Ok(Command::Fleet(Some(SlotStatus::AwaitingApproval)))
        );
        assert_eq!(parse("fleet off"), Ok(Command::Fleet(None)));
        assert_eq!(
            parse("voice recording"),
            Ok(Command::Voice(VoiceState::Recording))
        );
        assert_eq!(parse("brightness 80"), Ok(Command::Brightness(80)));
        assert_eq!(parse("selection on"), Ok(Command::Selection(true)));
        assert_eq!(parse("selection false"), Ok(Command::Selection(false)));
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        assert!(parse("").is_err());
        assert!(parse("nope").is_err());
        assert!(parse("agent").is_err());
        assert!(parse("agent x working").is_err());
        assert!(parse("agent 0 napping").is_err());
        assert!(parse("brightness 101").is_err());
        assert!(parse("brightness -1").is_err());
        assert!(parse("selection maybe").is_err());
    }

    #[test]
    fn a_line_round_trips_through_the_socket() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        assert_eq!(send(port, "agent 0 working").unwrap(), "ok");
        assert_eq!(
            queue.drain(),
            vec![Command::Agent {
                index: 0,
                status: SlotStatus::Working
            }]
        );

        assert!(send(port, "agent 9 napping").unwrap().starts_with("err:"));
        assert!(
            queue.drain().is_empty(),
            "rejected commands never reach the device"
        );
    }
}
