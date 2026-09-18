#![deny(unsafe_code)]
#![warn(clippy::unwrap_used)]
//! The localSpace desktop app (architecture v2 §2, §6.2).
//!
//! Core and the API server run in this process on a loopback port chosen at
//! start, and the web client — the same bundle `localspace serve` gives a
//! browser — loads in the system webview from that port, signed in with the
//! token the server generated. There is one client and one code path to Core.
//!
//! What a person double-clicking an icon needs on top of that (the desktop
//! answers of 2026-09-12): a second launch shows the window that is already
//! open instead of fighting it for the database; a start that fails says so
//! in a dialog instead of opening nothing; and closing the window takes the
//! model's sidecar with it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use localspace_server::{Running, ServerConfig};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// A log this size is moved aside at the next start, so it never grows
/// without bound on a machine nobody looks at.
const LOG_LIMIT_BYTES: u64 = 5 * 1024 * 1024;

/// The build a log belongs to: packaging sets `LOCALSPACE_BUILD_ID` to the
/// commit, as it does for the command line.
const BUILD_ID: &str = match option_env!("LOCALSPACE_BUILD_ID") {
    Some(id) => id,
    None => "dev",
};

fn main() -> anyhow::Result<()> {
    let cfg = config();
    let log = start_logging(cfg.data.as_deref());
    tracing::info!("localSpace {} ({BUILD_ID})", env!("CARGO_PKG_VERSION"));

    // Core and the server live on their own runtime; Tauri owns the main
    // thread for the lifetime of the window.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let running: Arc<Mutex<Option<Running>>> = Arc::default();

    let app = tauri::Builder::default()
        // First, so that a second launch ends here, before it opens anything.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup({
            let running = running.clone();
            let handle = runtime.handle().clone();
            move |app| {
                match handle.block_on(start(cfg)) {
                    Ok(started) => {
                        let url = format!("http://{}/?token={}", started.addr, started.token);
                        tracing::info!("web client at http://{}/", started.addr);
                        if let Ok(mut held) = running.lock() {
                            *held = Some(started);
                        }
                        match open_main_window(app, &url) {
                            Ok(()) => tracing::info!("the window is open"),
                            Err(e) => {
                                tracing::error!("the window could not be opened: {e:#}");
                                fail(app, no_window(&e, log.as_deref()));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("could not start: {e:#}");
                        fail(app, no_start(&e, log.as_deref()));
                    }
                }
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    app.run(move |_app, event| {
        if let tauri::RunEvent::Exit = event {
            let started = running.lock().ok().and_then(|mut held| held.take());
            if let Some(started) = started {
                runtime.block_on(started.shutdown());
            }
        }
    });
    Ok(())
}

/// The server, and Core behind it. Core is made on first use; here that is
/// now, so a data directory that cannot be opened is a dialog at launch and
/// not an error on the first message.
async fn start(cfg: ServerConfig) -> anyhow::Result<Running> {
    let started = localspace_server::start(cfg).await?;
    started.server.core().await?;
    Ok(started)
}

fn open_main_window(app: &tauri::App, url: &str) -> anyhow::Result<()> {
    tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(url.parse()?))
        .title("localSpace")
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .build()?;
    Ok(())
}

/// A second launch: the window that is already open comes to the front.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Say what went wrong where the person can see it, then leave. The dialog
/// needs the event loop, so this returns and the exit is the button's.
fn fail(app: &tauri::App, message: String) {
    app.dialog()
        .message(message)
        .title("localSpace")
        .kind(MessageDialogKind::Error)
        .show(|_| std::process::exit(1));
}

fn no_start(error: &anyhow::Error, log: Option<&Path>) -> String {
    format!(
        "localSpace could not start.\n\n{error:#}{}",
        where_the_log_is(log)
    )
}

fn no_window(error: &anyhow::Error, log: Option<&Path>) -> String {
    let hint = if cfg!(windows) {
        "\n\nlocalSpace shows its window with Microsoft Edge WebView2, which is part of \
         Windows 11 and of an up-to-date Windows 10. If it is missing from this computer, \
         install \"WebView2 Runtime\" from Microsoft and start localSpace again."
    } else {
        ""
    };
    format!(
        "localSpace could not open its window.\n\n{error:#}{hint}{}",
        where_the_log_is(log)
    )
}

fn where_the_log_is(log: Option<&Path>) -> String {
    log.map(|path| format!("\n\nThe details are in {}", path.display()))
        .unwrap_or_default()
}

/// Where the log goes: a release build has no console on Windows, so it
/// writes `<data>/logs/app.log` and says where that is when something fails;
/// a debug build keeps the terminal it was started from.
fn start_logging(data: Option<&Path>) -> Option<PathBuf> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "localspace=info".into());
    let file = if cfg!(debug_assertions) {
        None
    } else {
        data.and_then(open_log)
    };
    match file {
        Some((path, file)) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
            Some(path)
        }
        None => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
            None
        }
    }
}

fn open_log(data: &Path) -> Option<(PathBuf, std::fs::File)> {
    let dir = data.join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("app.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_LIMIT_BYTES) {
        let _ = std::fs::rename(&path, dir.join("app.log.1"));
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some((path, file))
}

/// `localspace-app [--harnesses <dir>] [--registry <dir>] [--data <dir>] [--web <dir>] [--user <name>]`
fn config() -> ServerConfig {
    let mut cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        personal: true,
        ..ServerConfig::default()
    };
    // The bundle: beside the executable in an installed app, `web/dist` in a
    // checkout.
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("web")));
    cfg.web_root = match beside {
        Some(dir) if dir.join("index.html").exists() => Some(dir),
        _ => Some(PathBuf::from("web/dist")),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--harnesses" => cfg.harnesses = it.next().map(PathBuf::from),
            "--data" => cfg.data = it.next().map(PathBuf::from),
            "--registry" => cfg.registry.extend(it.next().map(PathBuf::from)),
            "--models" => cfg.models = it.next().map(PathBuf::from),
            "--llama-server" => cfg.llama_server = it.next().map(PathBuf::from),
            "--web" => cfg.web_root = it.next().map(PathBuf::from),
            "--user" => cfg.user = it.next().unwrap_or(cfg.user),
            other => eprintln!("ignoring unknown argument `{other}`"),
        }
    }
    // Only chat ships in the box (v2 §1): without flags nothing is installed,
    // the catalog is what lies beside the executable or in the checkout, and
    // what the user installs from it persists under the user's data directory.
    if cfg.registry.is_empty() {
        let beside = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("registry")))
            .filter(|dir| dir.is_dir());
        // An installed app offers its own catalog and nothing else: started
        // from a shortcut its working directory is the install directory, and
        // the same catalog must not be offered twice under two spellings.
        cfg.registry = match beside {
            Some(dir) => vec![dir],
            None => [PathBuf::from("registry"), PathBuf::from("harnesses")]
                .into_iter()
                .filter(|dir| dir.is_dir())
                .collect(),
        };
    }
    if cfg.data.is_none() {
        cfg.data = default_data_dir();
    }
    cfg
}

/// `%LOCALAPPDATA%\io.localspace.app\data` on Windows,
/// `$XDG_DATA_HOME/localspace` or `~/.localspace` elsewhere: where the user's
/// environment lives when the command line says nothing. On Windows that is
/// the application's own data folder, beside the webview's: the installer
/// puts the program in `%LOCALAPPDATA%\localSpace`, which must not also hold
/// the data, and its uninstaller's "delete the application data" removes
/// exactly this folder and nothing else.
fn default_data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        return Some(PathBuf::from(dir).join("io.localspace.app").join("data"));
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Some(PathBuf::from(dir).join("localspace"));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".localspace"))
}
