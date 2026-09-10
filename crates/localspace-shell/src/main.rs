//! The localSpace desktop app (architecture v2 §2, §6.2).
//!
//! Core and the API server run in this process on a loopback port chosen at
//! start, and the web client — the same bundle `localspace serve` gives a
//! browser — loads in the system webview from that port, signed in with the
//! token the server generated. There is one client and one code path to Core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use localspace_server::ServerConfig;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localspace=info".into()),
        )
        .init();

    let cfg = config();
    // Core and the server live on their own runtime; Tauri owns the main
    // thread for the lifetime of the window.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let running = runtime.block_on(localspace_server::start(cfg))?;
    let url = format!("http://{}/?token={}", running.addr, running.token);
    tracing::info!("web client at http://{}/", running.addr);

    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External(url.parse()?),
            )
            .title("localSpace")
            .inner_size(1440.0, 900.0)
            .min_inner_size(900.0, 600.0)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    drop(running);
    drop(runtime);
    Ok(())
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
            .and_then(|p| p.parent().map(|d| d.join("registry")));
        for dir in [beside, Some(PathBuf::from("registry")), Some(PathBuf::from("harnesses"))]
            .into_iter()
            .flatten()
        {
            if dir.is_dir() && !cfg.registry.contains(&dir) {
                cfg.registry.push(dir);
            }
        }
    }
    if cfg.data.is_none() {
        cfg.data = default_data_dir();
    }
    cfg
}

/// `%LOCALAPPDATA%\localSpace` on Windows, `$XDG_DATA_HOME/localspace` or
/// `~/.localspace` elsewhere: where the user's environment lives when the
/// command line says nothing.
fn default_data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        return Some(PathBuf::from(dir).join("localSpace"));
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return Some(PathBuf::from(dir).join("localspace"));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".localspace"))
}
