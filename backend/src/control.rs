//! Loopback control socket: how another harness drives the device.
//!
//! The ChatGPT app reads its agent keys from its own thread list. A
//! harness-agnostic host cannot know that, so the state arrives over a loopback
//! socket as newline-delimited commands and gets applied on the next tick:
//!
//! ```text
//! agent 0 working          # key 0 shows the working colour
//! agent 0 off
//! session 7f3a working     # the host picks a free key and remembers the owner
//! session 7f3a end         # releases that key
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
    /// `session <id> <status>` — the host picks the agent key and remembers which
    /// session owns it, so a harness reports an id it already has instead of
    /// asking the user to hand-assign key numbers. `end`/`off` releases it.
    Session {
        id: String,
        status: Option<SlotStatus>,
    },
    /// `None` clears the fleet-wide status.
    Fleet(Option<SlotStatus>),
    Voice(VoiceState),
    Brightness(u8),
    Selection(bool),
    Ping,
    /// The last agent key the user tapped, as
    /// `{"seq":N,"session":"<id>"|null}`, for a harness UI that can jump to a
    /// session. The dsh browser half polls this over `GET /activation`; the
    /// sequence number lets a poll tell a new tap from the same one.
    Activation,
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
        "session" => {
            let [id, state] = args.as_slice() else {
                return Err("usage: session <id> <status|end>".to_string());
            };
            validate_session_id(id)?;
            let status = match *state {
                "end" | "off" => None,
                other => Some(enum_value(other)?),
            };
            Ok(Command::Session {
                id: (*id).to_string(),
                status,
            })
        }
        "activation" => Ok(Command::Activation),
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

/// Session ids come from other programs, so keep them to one short token: no
/// whitespace (the socket is line-based) and no unbounded growth.
fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("session id must be 1-64 characters".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err(format!("session id has unsupported characters: {id}"));
    }
    Ok(())
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

/// The last agent key the user tapped, shared between the device loop (which
/// writes it) and the control port (which answers it). It is deliberately not a
/// queued command: a page's poll must answer even while USB work has the device
/// loop busy.
pub type Activation = Arc<Mutex<Option<(u64, String)>>>;

/// Key events a `plugin:` binding produced, newest last.
///
/// The keyboard reaches a harness two ways: a keystroke typed at whatever has
/// focus, or - for a harness that owns an API - an event its own plugin acts on.
/// A Web UI whose buttons have no key tokens (`dsh`) is the second kind, so the
/// host keeps a feed the page polls, the same way it answers `activation`.
#[derive(Debug, Clone, Default)]
pub struct Events {
    seq: Arc<Mutex<u64>>,
    log: Arc<Mutex<Vec<(u64, String)>>>,
}

/// How many events a slow poller can fall behind before the oldest are dropped.
/// A page polls once a second, so this is minutes of taps.
const EVENT_BACKLOG: usize = 64;

impl Events {
    /// Record one `plugin:` event. Called from the device loop.
    pub fn push(&self, event: &str) {
        let Ok(mut seq) = self.seq.lock() else { return };
        *seq += 1;
        let id = *seq;
        drop(seq);
        let Ok(mut log) = self.log.lock() else { return };
        log.push((id, event.to_string()));
        if log.len() > EVENT_BACKLOG {
            log.remove(0);
        }
    }

    /// Everything newer than `since`, with the sequence to pass next time.
    pub fn since(&self, since: u64) -> (u64, Vec<String>) {
        let seq = self.seq.lock().map(|s| *s).unwrap_or(0);
        let log = self.log.lock().ok();
        let events = log
            .map(|log| {
                log.iter()
                    .filter(|(id, _)| *id > since)
                    .map(|(_, event)| event.clone())
                    .collect()
            })
            .unwrap_or_default();
        (seq, events)
    }
}

/// `{"seq":N,"events":["approve"]}` - the feed a plugin polls.
///
/// Event names are validated on the way in (one token, no quotes), so this needs
/// no escaping step.
pub fn events_json(events: &Events, since: u64) -> String {
    let (seq, list) = events.since(since);
    let body = list
        .iter()
        .map(|event| format!("\"{event}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"seq\":{seq},\"events\":[{body}]}}")
}

/// Commands waiting for the device loop to pick up.
#[derive(Clone, Default)]
pub struct Queue {
    jobs: Arc<Mutex<Vec<Job>>>,
    activation: Activation,
    events: Events,
}

impl Queue {
    /// Enqueue and block until the host answers, for the socket path.
    fn ask(&self, command: Command) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::sync_channel(1);
        self.jobs.lock().unwrap().push(Job { command, reply });
        rx.recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| "the host did not answer in time".to_string())
    }

    pub fn drain(&self) -> Vec<Job> {
        std::mem::take(&mut *self.jobs.lock().unwrap())
    }

    /// The slot the device loop should write the last tap into.
    pub fn activation(&self) -> Activation {
        self.activation.clone()
    }

    /// The feed the device loop writes `plugin:` events into.
    pub fn events(&self) -> Events {
        self.events.clone()
    }
}

/// What a poller sees: the last tap, or an empty slot before the first one.
/// Session ids are validated on the way in (one short token, no quotes), so
/// this is JSON without an escaping step.
pub fn activation_json(activation: &Activation) -> String {
    let Ok(slot) = activation.lock() else {
        return "{\"seq\":0,\"session\":null}".to_string();
    };
    match &*slot {
        Some((seq, session)) => format!("{{\"seq\":{seq},\"session\":\"{session}\"}}"),
        None => "{\"seq\":0,\"session\":null}".to_string(),
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
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let reply = match read_capped_line(&mut reader, &mut line) {
        Ok(0) => return,
        // the cap stopped the read before the sender finished a line
        Ok(_) if !line.ends_with('\n') => "err: line too long".to_string(),
        // a browser can only speak HTTP; every harness uses the line protocol.
        // OPTIONS is here for the preflight a browser may send the poll.
        Ok(_) if line.starts_with("GET ") || line.starts_with("OPTIONS ") => {
            http(&line, &mut reader, &mut writer, queue);
            return;
        }
        Ok(_) => match parse(&line) {
            // answered from the shared slot, never queued: a poll must not wait
            // for whatever the device loop is doing
            Ok(Command::Activation) => activation_json(&queue.activation),
            Ok(command) => queue.ask(command).unwrap_or_else(|err| format!("err: {err}")),
            Err(err) => format!("err: {err}"),
        },
        Err(_) => return,
    };
    let _ = writer.write_all(format!("{reply}\n").as_bytes());
    let _ = writer.flush();
}

/// Read one line, refusing anything longer than the line cap.
fn read_capped_line(reader: &mut impl BufRead, line: &mut String) -> std::io::Result<usize> {
    reader.by_ref().take(MAX_LINE as u64).read_line(line)
}

/// The browser face of the same port. A page cannot open a socket, so the one
/// thing it may ask for is which agent key the user just tapped:
///
/// ```text
/// GET /activation -> {"seq":3,"session":"8e53a70f-…"} | {"seq":3,"session":null}
/// GET /events?since=4 -> {"seq":6,"events":["approve","reject"]}
/// ```
///
/// The line protocol on this port is untouched; anything else answers 404.
/// `OPTIONS` is there for the preflight a browser may send.
fn http(request: &str, reader: &mut impl BufRead, writer: &mut impl Write, queue: &Queue) {
    // swallow the headers, so the browser sees a complete response
    let mut header = String::new();
    loop {
        header.clear();
        match read_capped_line(reader, &mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
        }
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    // the request target is `path` or `path?query`; split before matching, or a
    // poller with a query string gets a 404
    let target = parts.next().unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = Some(path);
    // `?since=N` lets a poller ask for what it has not seen yet
    let since = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("since="))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let (status, body) = match (method, path) {
        // a browser that decides to preflight the poll must not be told 404
        ("OPTIONS", Some("/activation")) | ("OPTIONS", Some("/events")) => {
            ("204 No Content", String::new())
        }
        (_, Some("/activation")) => ("200 OK", activation_json(&queue.activation)),
        (_, Some("/events")) => ("200 OK", events_json(&queue.events, since)),
        _ => ("404 Not Found", "{\"error\":\"not found\"}".to_string()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = writer.write_all(response.as_bytes());
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
            parse("session 7f3a working"),
            Ok(Command::Session {
                id: "7f3a".to_string(),
                status: Some(SlotStatus::Working)
            })
        );
        assert_eq!(
            parse("session 7f3a end"),
            Ok(Command::Session {
                id: "7f3a".to_string(),
                status: None
            })
        );
        assert_eq!(
            parse("session 7f3a off"),
            Ok(Command::Session {
                id: "7f3a".to_string(),
                status: None
            }),
            "off releases the key too — a released key is dark either way"
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
        assert_eq!(parse("activation"), Ok(Command::Activation));
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
        assert!(parse("session").is_err());
        assert!(parse("session 7f3a").is_err());
        assert!(parse("session 7f3a sleeping").is_err());
        assert!(parse("session a/b working").is_err(), "one bare token only");
        assert!(
            parse(&format!("session {} working", "x".repeat(65))).is_err(),
            "an id long enough to be a payload is refused"
        );
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
    fn the_event_feed_hands_out_what_a_poller_has_not_seen() {
        let events = Events::default();
        // nothing yet: a first poll must not replay history
        assert_eq!(events_json(&events, 0), "{\"seq\":0,\"events\":[]}");

        events.push("approve");
        events.push("reject");
        assert_eq!(
            events_json(&events, 0),
            "{\"seq\":2,\"events\":[\"approve\",\"reject\"]}"
        );

        // a poller that already saw both gets an empty list but the right seq
        assert_eq!(events_json(&events, 2), "{\"seq\":2,\"events\":[]}");
        // and one that missed the first still gets the second
        assert_eq!(events_json(&events, 1), "{\"seq\":2,\"events\":[\"reject\"]}");
    }

    #[test]
    fn the_event_feed_drops_the_oldest_when_a_poller_falls_behind() {
        let events = Events::default();
        for i in 0..(EVENT_BACKLOG + 5) {
            events.push(&format!("e{i}"));
        }
        let (seq, list) = events.since(0);
        assert_eq!(seq as usize, EVENT_BACKLOG + 5);
        assert_eq!(
            list.len(),
            EVENT_BACKLOG,
            "a poller minutes behind does not grow the host without bound"
        );
        assert_eq!(list.first().map(String::as_str), Some("e5"));
    }

    #[test]
    fn a_browser_poll_gets_the_events_over_http() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        queue.events().push("approve");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .write_all(b"GET /events?since=0 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(
            response.ends_with("{\"seq\":1,\"events\":[\"approve\"]}"),
            "{response}"
        );
    }

    #[test]
    fn a_browser_poll_gets_the_last_tap() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        *queue.activation().lock().unwrap() = Some((7, "dsh-1".to_string()));
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .write_all(b"GET /activation HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(
            response.contains("Access-Control-Allow-Origin: *"),
            "the page polls from another port and needs CORS: {response}"
        );
        assert!(
            response.ends_with("{\"seq\":7,\"session\":\"dsh-1\"}"),
            "{response}"
        );
    }

    #[test]
    fn a_preflight_gets_its_cors_answer() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .write_all(
                b"OPTIONS /activation HTTP/1.1\r\nOrigin: http://127.0.0.1:3080\r\n\
                  Access-Control-Request-Method: GET\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with("HTTP/1.1 204 No Content"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Methods: GET, OPTIONS"),
            "a preflighted poll needs the methods it may use: {response}"
        );
    }

    #[test]
    fn an_activation_poll_never_waits_for_the_device_loop() {
        // No fake host runs here: a queued command would sit out the 5s reply
        // timeout. The tap slot is answered in place, so this returns at once -
        // a page must not go blind while the loop is stuck in USB work.
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        *queue.activation().lock().unwrap() = Some((2, "dsh-2".to_string()));
        let started = std::time::Instant::now();
        let reply = send(port, "activation").unwrap();
        assert_eq!(reply, "{\"seq\":2,\"session\":\"dsh-2\"}");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "answered from the shared slot, not the queue"
        );
    }

    #[test]
    fn a_browser_poll_for_anything_else_is_a_404() {
        let queue = Queue::default();
        let port = serve(0, queue.clone()).expect("bind");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(b"GET /nope HTTP/1.1\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");
        assert!(queue.drain().is_empty(), "nothing reached the device");
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
