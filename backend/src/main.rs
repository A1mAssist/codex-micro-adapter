//! `codex-micro-backend` — a Rust host for the Work Louder Codex Micro.
//!
//! Layers, each ported from the vendor's own sources (see the module docs):
//!
//! ```text
//! framing  64-byte HID reports, line reassembly
//! rpc      {method,params,id} JSON-RPC, 10s timeout, 50ms cadence, notifications
//! oai      the v.oai.* methods and payload shapes
//! layout   key / encoder / joystick -> action, from the app's own layout chunks
//! device   connect, reconnect backoff, lighting, battery, inactivity
//! lighting the app's `$` / `se` / `ce` derivations
//! actions  action -> keystrokes / text / url for whatever harness has focus
//! control  loopback socket so any harness can push its own state
//! host     the loop both front ends share
//! config   the settings the app keeps for this device
//! ```
//!
//! ```text
//! codex-micro-backend list                     enumerate the HID interfaces
//! codex-micro-backend run [--live]             run the console host
//! codex-micro-backend send "agent 0 working"   talk to a running host
//! codex-micro-backend config                   config path + defaults
//! ```

use codex_micro_backend::config::Config;
use codex_micro_backend::host::{Host, POLL_TIMEOUT};
use codex_micro_backend::performer::LoggingPerformer;
use codex_micro_backend::{control, device};

use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("listen") => listen(&args[1..]),
        Some("run") => run(args.iter().any(|a| a == "--live")),
        Some("send") => send(&args[1..]),
        Some("config") => print_config(),
        Some("window") => print_window(&args[1..]),
        _ => usage(),
    }
}

fn usage() {
    println!("usage: codex-micro-backend <command>");
    println!();
    println!("  list                       enumerate Work Louder HID interfaces");
    println!("  listen [--seconds N]       watch what the keyboard sends, writing nothing");
    println!("  run [--live]               run the host; --live injects real keystrokes");
    println!("  send <control command...>  push state to a running host");
    println!("  config                     print the config path and the defaults");
    println!("  window                     print the window that has focus right now");
}

/// What an agent key would focus if it were pressed now. Handy when a session
/// seems to remember the wrong window. `window --focus <hwnd>` exercises the
/// focus call itself, which is otherwise only reachable from the keyboard.
fn print_window(args: &[String]) {
    if let Some(value) = args.iter().position(|a| a == "--focus").and_then(|i| args.get(i + 1)) {
        let parsed = value
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        let hwnd = isize::from_str_radix(parsed, 16)
            .or_else(|_| value.parse::<isize>())
            .unwrap_or(0);
        return match codex_micro_backend::performer::focus_window(hwnd) {
            Ok(()) => println!("focused window: {hwnd:#x}"),
            Err(err) => println!("could not focus {hwnd:#x}: {err}"),
        };
    }
    match codex_micro_backend::performer::foreground_window() {
        Some(hwnd) => println!("foreground window: {hwnd:#x}"),
        None => println!("no window has focus"),
    }
}

fn print_config() {
    let path = Config::default_path();
    println!("{}", path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&Config::default()).unwrap_or_default()
    );
}

/// One line of control protocol, sent to whatever host is running.
fn send(args: &[String]) {
    let line = args.join(" ");
    if line.is_empty() {
        eprintln!("usage: codex-micro-backend send \"agent 0 working\"");
        std::process::exit(2);
    }
    let port = Config::load(&Config::default_path()).control_port;
    match control::send(port, &line) {
        Ok(reply) => {
            println!("{reply}");
            if reply.starts_with("err") {
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("127.0.0.1:{port}: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn list() {
    let (scanned, devices) = codex_micro_backend::hid_windows::enumerate_scanned();
    println!("scanned {scanned} HID interfaces");
    if devices.is_empty() {
        println!("no Work Louder device found");
        return;
    }
    for d in devices {
        println!(
            "{}  vid={:#06x} pid={:#06x} usage_page={:#06x} usage={}  {}",
            if d.is_codex_micro {
                "codex-micro"
            } else {
                "creator-micro-v2"
            },
            d.vendor_id,
            d.product_id,
            d.usage_page,
            d.usage,
            d.path
        );
    }
}

#[cfg(not(windows))]
fn list() {
    println!("device discovery is Windows-only for now");
}

#[cfg(windows)]
/// Watch what the keyboard sends, and write nothing back.
///
/// Opens the vendor collection read-only, so whichever app is already driving
/// the keyboard keeps driving it. Answers one question: do reports arrive?
#[cfg(windows)]
fn listen(args: &[String]) {
    let seconds: u64 = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let Some(candidate) = codex_micro_backend::hid_windows::scan() else {
        eprintln!("no Codex Micro on the vendor collection (usage page 0xFF00)");
        std::process::exit(2);
    };
    println!("device: {}", candidate.path);
    let device = match codex_micro_backend::hid_windows::ListenOnly::open(&candidate.path) {
        Ok(device) => device,
        Err(err) => {
            eprintln!("opening read-only failed: {err}");
            std::process::exit(2);
        }
    };
    println!("watching for {seconds}s — read-only, no report is ever written");
    println!("press keys, turn the knob, move the stick");

    let mut lines = codex_micro_backend::framing::LineBuffers::default();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let (mut reports, mut events) = (0usize, 0usize);
    while Instant::now() < deadline {
        let Some(report) = device.next(Duration::from_millis(250)) else {
            if device.is_closed() {
                eprintln!("device closed — unplugged?");
                break;
            }
            continue;
        };
        reports += 1;
        let at = start.elapsed().as_secs_f64();
        let payload: Vec<String> = report[..(3 + report[2] as usize).min(report.len())]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        println!("[{at:6.3}s] {}", payload.join(" "));
        for line in lines.push(&report) {
            if line.contains("v.oai.") {
                events += 1;
            }
            println!("          {line}");
        }
    }
    println!("---");
    println!("{reports} reports, {events} device events in {seconds}s");
    if reports == 0 {
        std::process::exit(2);
    }
}

#[cfg(not(windows))]
fn listen(_args: &[String]) {
    println!("listening needs the Windows HID backend");
}

#[cfg(windows)]
fn run(live: bool) {
    let config_path = Config::default_path();
    let config = Config::load(&config_path);
    println!("config: {}", config_path.display());
    println!(
        "mode:   {}",
        if live {
            "live (keystrokes injected)"
        } else {
            "dry run (use --live to inject)"
        }
    );

    let queue = control::Queue::default();
    let performer: Box<dyn codex_micro_backend::actions::Performer> = if live {
        Box::new(codex_micro_backend::performer::WindowsPerformer)
    } else {
        Box::new(LoggingPerformer)
    };
    let mut host = Host::new(
        device::Device::new(
            codex_micro_backend::hid_windows::Opener,
            config.layout.clone(),
            config.lighting(),
        ),
        config.bindings.clone(),
        performer,
        config.brightness_percent,
        config.lighting(),
    );
    // the port answers an `activation` poll from this slot, so a page gets an
    // answer even while the loop below is stuck in USB work
    host.share_activation(queue.activation());
    match control::serve(config.control_port, queue.clone()) {
        Ok(port) => println!("control: 127.0.0.1:{port}  (codex-micro-backend send ...)"),
        Err(err) => eprintln!(
            "control: 127.0.0.1:{} unavailable: {err}",
            config.control_port
        ),
    }

    loop {
        for job in queue.drain() {
            println!("[ctl] {}", job.run(|command| host.apply(command)));
        }
        let now = Instant::now();
        // only enumerate the USB tree when the host would actually use the answer
        let candidate = if host.scan_due(now) {
            codex_micro_backend::hid_windows::scan()
        } else {
            None
        };
        for event in host.pump(now, POLL_TIMEOUT, candidate) {
            println!("[evt] {}", event.describe());
        }
        if !host.device.is_connected() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(not(windows))]
fn run(_live: bool) {
    println!("the host needs the Windows HID backend");
}
