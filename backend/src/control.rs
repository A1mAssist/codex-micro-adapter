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
//! The reply is a single line, and it is the host's own answer: `ok` once the
//! device loop applied the command, or `err: ...` if it refused it.

use crate::lighting::{SlotStatus, VoiceState};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::SyncSender;
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

/// Longest command line the socket accepts. The whole vocabulary fits in a few
/// dozen bytes; the cap only stops a client from growing the host without bound.
const MAX_LINE: usize = 1024;
/// How long a client may take to send its line and to read the answer.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a client waits for the host to actually apply its command.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// One command waiting for the device loop, plus the line that is waiting to
/// hear what the host actually did with it.
pub struct Job {
    command: Command,
    reply: SyncSender<String>,
}

impl Job {
    /// Hand the command to the host, then give the host's own reply back to
    /// whoever asked. This is what keeps `codex-micro-backend send` honest: a
    /// command the host rejects comes back as `err: …`, not a cheerful `ok`.
    pub fn run(self, host: impl FnOnce(Command) -> String) -> String {
        let message = host(self.command);
        let _ = self.reply.try_send(message.clone());
        message
    }
}

/// Commands waiting for the device loop to pick up.
#[derive(Clone, Default)]
pub struct Queue(Arc<Mutex<Vec<Job>>>);

impl Queue {
    /// Enqueue and block until the host answers, for the socket path.
    fn ask(&self, command: Command) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::sync_channel(1);
        self.0.lock().unwrap().push(Job { command, reply });
        rx.recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| "the host did not answer in time".to_string())
    }

    pub fn drain(&self) -> Vec<Job> {
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
            // one command per connection, answered in place: a client that hangs
            // costs one timeout instead of an unbounded pile of threads
            handle(stream, &queue);
        }
    });
    Ok(bound)
}

fn handle(stream: TcpStream, queue: &Queue) {
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream).take(MAX_LINE as u64);
    let mut line = String::new();
    let reply = match reader.read_line(&mut line) {
        Ok(0) => return,
        // the cap stopped the read before the sender finished a line
        Ok(_) if !line.ends_with('\n') => "err: line too long".to_string(),
        Ok(_) => match parse(&line) {
            Ok(command) => queue.ask(command).unwrap_or_else(|err| format!("err: {err}")),
            Err(err) => format!("err: {err}"),
        },
        Err(_) => return,
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

    /// Stand in for the device loop: apply the next command and answer for it.
    fn fake_host(
        queue: &Queue,
        reply: impl Fn(&Command) -> String + Send + 'static,
    ) -> std::thread::JoinHandle<usize> {
        let queue = queue.clone();
        std::thread::spawn(move || {
            let mut handled = 0;
            while handled == 0 {
                for job in queue.drain() {
                    job.run(|command| reply(&command));
                    handled += 1;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            handled
        })
    }

    #[test]
    fn a_line_round_trips_through_the_socket() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        let host = fake_host(&queue, |command| format!("applied {command:?}"));
        let reply = send(port, "agent 0 working").unwrap();
        assert!(reply.contains("Working"), "the host applied it: {reply}");
        assert_eq!(host.join().unwrap(), 1);
        assert!(queue.drain().is_empty(), "the loop took it off the queue");

        assert!(send(port, "agent 9 napping").unwrap().starts_with("err:"));
        assert!(
            queue.drain().is_empty(),
            "rejected commands never reach the device"
        );
    }

    #[test]
    fn the_client_sees_the_hosts_own_reply() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        let host = fake_host(&queue, |_| "err: agent index 9 out of range".to_string());
        assert_eq!(
            send(port, "agent 9 off").unwrap(),
            "err: agent index 9 out of range",
            "the socket reports what the host did, not just that it parsed"
        );
        host.join().unwrap();
    }

    #[test]
    fn an_overlong_line_is_refused() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        let reply = send(port, &"x".repeat(MAX_LINE * 2));
        assert!(
            reply.is_err() || reply.unwrap().starts_with("err:"),
            "an overlong line is refused, never applied"
        );
        assert!(queue.drain().is_empty(), "nothing reached the host");
    }
}
