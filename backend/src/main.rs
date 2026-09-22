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
        Some("run") => run(args.iter().any(|a| a == "--live")),
        Some("send") => send(&args[1..]),
        Some("config") => print_config(),
        _ => usage(),
    }
}

fn usage() {
    println!("usage: codex-micro-backend <command>");
    println!();
    println!("  list                       enumerate Work Louder HID interfaces");
    println!("  run [--live]               run the host; --live injects real keystrokes");
    println!("  send <control command...>  push state to a running host");
    println!("  config                     print the config path and the defaults");
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
    match control::serve(config.control_port, queue.clone()) {
        Ok(port) => println!("control: 127.0.0.1:{port}  (codex-micro-backend send ...)"),
        Err(err) => eprintln!(
            "control: 127.0.0.1:{} unavailable: {err}",
            config.control_port
        ),
    }

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
