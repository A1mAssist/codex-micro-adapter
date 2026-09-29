//! Codex Micro, wrapped in a window.
//!
//! The window is a thin shell around the same host loop the console binary runs:
//! the device lives on a worker thread, publishes a [`Snapshot`], and takes
//! commands either from the UI or from the loopback socket a harness plugin
//! writes to.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use codex_micro_backend::actions::Performer;
use codex_micro_backend::config::Config;
use codex_micro_backend::control::{self, Command, Queue};
use codex_micro_backend::host::{Snapshot, POLL_TIMEOUT};
use codex_micro_backend::performer::LoggingPerformer;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Manager, State};
use tauri_plugin_autostart::ManagerExt;

mod vendor;

/// What the window asks the worker thread to do.
enum UiMessage {
    Apply(Command),
    Save(Config),
    Live(bool),
}

struct App {
    ui: Mutex<Sender<UiMessage>>,
    /// Port serving the app's own renderer bundle, when it has been extracted.
    vendor_port: Option<u16>,
    snapshot: Arc<Mutex<Snapshot>>,
    config: Mutex<Config>,
    config_path: PathBuf,
    live: Arc<Mutex<bool>>,
}

/// Everything the window renders from.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    snapshot: Snapshot,
    config: Config,
    live: bool,
    config_path: String,
    /// The build's own version, for the About sheet.
    version: String,
}

fn build_status(app: &App) -> Status {
    Status {
        snapshot: app.snapshot.lock().unwrap().clone(),
        config: app.config.lock().unwrap().clone(),
        live: *app.live.lock().unwrap(),
        config_path: app.config_path.display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
fn status(app: State<App>) -> Status {
    build_status(&app)
}

/// Run one line of the control protocol: the same vocabulary the socket and
/// `codex-micro-backend send` speak, so the UI has no private API.
#[tauri::command]
fn apply(app: State<App>, command: String) -> String {
    match control::parse(&command) {
        Ok(command) => {
            let _ = app.ui.lock().unwrap().send(UiMessage::Apply(command));
            "ok".to_string()
        }
        Err(err) => format!("err: {err}"),
    }
}

#[tauri::command]
fn save_config(app: State<App>, config: Config) -> Result<Status, String> {
    config.save(&app.config_path).map_err(|e| e.to_string())?;
    *app.config.lock().unwrap() = config.clone();
    let _ = app.ui.lock().unwrap().send(UiMessage::Save(config));
    Ok(build_status(&app))
}

#[tauri::command]
fn set_live(app: State<App>, live: bool) -> Status {
    *app.live.lock().unwrap() = live;
    let _ = app.ui.lock().unwrap().send(UiMessage::Live(live));
    build_status(&app)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn autostart_status(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Is the app's own settings page available to open?
#[tauri::command]
fn vendor_available(app: State<App>) -> bool {
    app.vendor_port.is_some()
}

/// Opens the ChatGPT app's own Codex Micro settings page, served from the
/// extracted bundle, in its own window.
#[tauri::command]
fn open_vendor_page(app_handle: tauri::AppHandle, app: State<App>) -> Result<String, String> {
    let port = app.vendor_port.ok_or(
        "the app's webview bundle was not extracted - run: node scripts/extract-vendor-webview.mjs",
    )?;
    let url = format!("http://127.0.0.1:{port}/settings/codex-micro");
    if let Some(window) = app_handle.get_webview_window("vendor") {
        let _ = window.set_focus();
        return Ok(url);
    }
    let parsed = url.parse().map_err(|e| format!("bad url: {e}"))?;
    tauri::WebviewWindowBuilder::new(&app_handle, "vendor", tauri::WebviewUrl::External(parsed))
        .title("Codex Micro - app settings page")
        .inner_size(1240.0, 900.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(url)
}

#[cfg(windows)]
#[tauri::command]
fn devices() -> Vec<codex_micro_backend::hid_windows::HidDeviceInfo> {
    codex_micro_backend::hid_windows::enumerate()
}

#[cfg(not(windows))]
#[tauri::command]
fn devices() -> Vec<serde_json::Value> {
    Vec::new()
}

fn main() {
    let config_path = Config::default_path();
    let config = Config::load(&config_path);
    let queue = Queue::default();
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let live = Arc::new(Mutex::new(false));
    let (ui_tx, ui_rx) = mpsc::channel();
    let control_port = config.control_port;

    // The app's own renderer bundle, when scripts/extract-vendor-webview.mjs has
    // been run. Its settings page is what the "app settings page" button opens.
    let vendor_port = match vendor::root() {
        Some(root) => match vendor::serve(root) {
            Ok(port) => {
                println!("app webview on 127.0.0.1:{port}");
                Some(port)
            }
            Err(err) => {
                eprintln!("app webview unavailable: {err}");
                None
            }
        },
        None => None,
    };

    tauri::Builder::default()
        .manage(App {
            ui: Mutex::new(ui_tx),
            vendor_port,
            snapshot: snapshot.clone(),
            config: Mutex::new(config.clone()),
            config_path,
            live: live.clone(),
        })
        .invoke_handler(tauri::generate_handler![
            status,
            apply,
            save_config,
            set_live,
            set_autostart,
            autostart_status,
            devices,
            vendor_available,
            open_vendor_page
        ])
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--hidden")
                .build(),
        )
        .setup(move |app_handle| {
            let menu = tauri::menu::MenuBuilder::new(app_handle)
                .text("show", "Show")
                .text("quit", "Quit")
                .build()?;
            let tray_handle = app_handle.handle().clone();
            tauri::tray::TrayIconBuilder::new()
                .icon(
                    app_handle
                        .default_window_icon()
                        .cloned()
                        .expect("default app icon"),
                )
                .menu(&menu)
                .on_menu_event(move |_tray, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = tray_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => tray_handle.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app_handle)?;
            if let Some(window) = app_handle.get_webview_window("main") {
                let close_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        // ponytail: hide-on-close keeps the worker alive; add an explicit
                        // preference only if users actually need close-vs-hide control.
                        let _ = close_window.hide();
                    }
                });
                if std::env::args_os().any(|arg| arg == "--hidden") {
                    let _ = window.hide();
                }
            }
            {
                let (ready_tx, ready_rx) = mpsc::sync_channel(0);
                let config = config.clone();
                let queue = queue.clone();
                let snapshot = snapshot.clone();
                let live = live.clone();
                let handle = app_handle.handle().clone();
                std::thread::spawn(move || {
                    host_loop(config, ui_rx, queue, snapshot, live, Some(handle), ready_tx)
                });
                ready_rx.recv().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "host worker stopped before startup",
                    )
                })?;
            }
            match control::serve(control_port, queue.clone()) {
                Ok(port) => println!("control socket on 127.0.0.1:{port}"),
                Err(err) => {
                    eprintln!("control socket unavailable on 127.0.0.1:{control_port}: {err}")
                }
            }
            // CODEX_MICRO_VENDOR=1 opens the app's own page straight away, which
            // is how the port is checked while the bridge is being written.
            if std::env::var("CODEX_MICRO_VENDOR").is_ok() && vendor_port.is_some() {
                let state = app_handle.state::<App>();
                if let Ok(url) = open_vendor_page(app_handle.handle().clone(), state) {
                    println!("opened {url}");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Micro");
}

/// Dry run by default: nothing reaches another window until the user says so.
fn performer(live: bool) -> Box<dyn Performer> {
    if live {
        #[cfg(windows)]
        {
            return Box::new(codex_micro_backend::performer::WindowsPerformer);
        }
    }
    Box::new(LoggingPerformer)
}

#[cfg(windows)]
fn host_loop(
    config: Config,
    ui: Receiver<UiMessage>,
    queue: Queue,
    snapshot: Arc<Mutex<Snapshot>>,
    live: Arc<Mutex<bool>>,
    events: Option<tauri::AppHandle>,
    ready: mpsc::SyncSender<()>,
) {
    let mut host = codex_micro_backend::host::Host::new(
        codex_micro_backend::device::Device::new(
            codex_micro_backend::hid_windows::Opener,
            config.layout.clone(),
            config.lighting(),
        ),
        config.bindings.clone(),
        performer(*live.lock().unwrap()),
        config.brightness_percent,
        config.lighting(),
    );
    // the control port answers `activation` polls from this slot
    host.share_activation(queue.activation());
    host.share_events(queue.events());
    let _ = ready.send(());

    // Agent keys follow whatever the plugins push; "off" mutes them.
    let mut agent_keys = config.harness != "off";

    loop {
        for message in ui.try_iter() {
            match message {
                UiMessage::Apply(command) => {
                    host.apply(command);
                }
                UiMessage::Save(config) => {
                    agent_keys = config.harness != "off";
                    host.set_bindings(config.bindings.clone());
                    host.device.set_layout(config.layout.clone());
                    host.set_lighting(config.lighting(), config.brightness_percent);
                }
                UiMessage::Live(on) => host.set_performer(performer(on)),
            }
        }
        for job in queue.drain() {
            let reply = job.run(|command| {
                // "off" mutes the agent keys: answer the caller rather than
                // leaving it waiting for a reply that never comes
                if !agent_keys && matches!(&command, Command::Agent { .. }) {
                    return "err: agent keys are off".to_string();
                }
                host.apply(command)
            });
            println!("[ctl] {reply}");
        }
        let now = Instant::now();
        // only enumerate the USB tree when the host would actually use the answer
        let candidate = if host.scan_due(now) {
            codex_micro_backend::hid_windows::scan()
        } else {
            None
        };
        host.pump(now, POLL_TIMEOUT, candidate);
        let next = host.snapshot();
        let mut current = snapshot.lock().unwrap();
        let changed = *current != next;
        if changed {
            *current = next.clone();
        }
        drop(current);
        if changed {
            // the webview reacts to this instead of polling every 500 ms; the
            // command/status handlers still answer reads directly
            if let Some(handle) = &events {
                use tauri::Emitter;
                let _ = handle.emit("codex-micro://snapshot", &next);
            }
        }
        if !host.device.is_connected() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(not(windows))]
fn host_loop(
    _config: Config,
    _ui: Receiver<UiMessage>,
    _queue: Queue,
    _snapshot: Arc<Mutex<Snapshot>>,
    _live: Arc<Mutex<bool>>,
    _events: Option<tauri::AppHandle>,
    ready: mpsc::SyncSender<()>,
) {
    let _ = ready.send(());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
