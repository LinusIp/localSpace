#![deny(unsafe_code)]
//! `localspace-desktop` — the egui Client and Core in one process over the `InProcess` transport.
//!
//! The operator's commands — `doctor`, `bench`, `evals`, `call`, `audit` —
//! live in the `localspace` binary (the `localspace-cli` crate).

mod output;

use anyhow::Result;
use localspace_core::{Config, Core, transport};
use localspace_proto as proto;
use output::{err, out};
use std::path::PathBuf;
use std::sync::Arc;

struct Adapter(transport::InProcess);

impl localspace_client::Backend for Adapter {
    fn request(&self, req: proto::Request) -> u64 {
        use localspace_core::transport::Backend as _;
        self.0.request(req)
    }

    fn set_wake(&self, wake: localspace_client::Wake) {
        use localspace_core::transport::Backend as _;
        self.0.set_wake(wake)
    }

    fn poll(&self) -> Vec<localspace_client::Incoming> {
        use localspace_core::transport::Backend as _;
        self.0
            .poll()
            .into_iter()
            .map(|m| match m {
                transport::Incoming::Response { id, response } => {
                    localspace_client::Incoming::Response { id, response }
                }
                transport::Incoming::Event(e) => localspace_client::Incoming::Event(e),
            })
            .collect()
    }
}

struct Args {
    command: Command,
    harnesses: Option<PathBuf>,
    data: Option<PathBuf>,
    registry: Vec<PathBuf>,
    models: Option<PathBuf>,
    llama_server: Option<PathBuf>,
    user: String,
    organisation: bool,
}

enum Command {
    Run,
    Help,
}

fn parse_args() -> Args {
    let mut args = Args {
        command: Command::Run,
        harnesses: None,
        data: None,
        registry: Vec::new(),
        models: None,
        llama_server: None,
        user: whoami(),
        organisation: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" | "help" => args.command = Command::Help,
            "--harnesses" => args.harnesses = it.next().map(PathBuf::from),
            "--data" => args.data = it.next().map(PathBuf::from),
            "--registry" => args.registry.extend(it.next().map(PathBuf::from)),
            "--models" => args.models = it.next().map(PathBuf::from),
            "--llama-server" => args.llama_server = it.next().map(PathBuf::from),
            "--user" => args.user = it.next().unwrap_or_else(whoami),
            "--organisation" | "--organization" => args.organisation = true,
            other => err!("ignoring unknown argument `{other}`"),
        }
    }
    args
}

fn whoami() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "local".into())
}

fn config(args: &Args) -> Config {
    let mut cfg = if args.organisation {
        Config::organisation(&args.user)
    } else {
        Config::personal(&args.user)
    };
    cfg.harness_dir = args
        .harnesses
        .clone()
        .or_else(|| default_harness_dir().filter(|p| p.exists()));
    cfg.data_dir = args.data.clone();
    cfg.catalog_dirs = if args.registry.is_empty() {
        // An offline bundle sitting next to the installed set is the common case.
        vec![PathBuf::from("registry")]
            .into_iter()
            .filter(|p| p.exists())
            .collect()
    } else {
        args.registry.clone()
    };
    cfg.models_dir = args
        .models
        .clone()
        .or_else(|| Some(PathBuf::from("models")).filter(|p| p.exists()));
    cfg.llama_server = args.llama_server.clone();
    cfg
}

fn default_harness_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("harnesses")))
        .or_else(|| Some(PathBuf::from("harnesses")))
}

fn main() -> Result<()> {
    localspace_client::perf::mark_start();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localspace=info".into()),
        )
        .init();

    let args = parse_args();
    match &args.command {
        Command::Help => {
            print_help();
            Ok(())
        }
        Command::Run => run_gui(args),
    }
}

/// `localspace audit verify --data <dir>` — walks every file of the audit log
/// in order and checks each record's hash against the one before it, across
/// files (deployment §10.1). Exit 1 at the first broken or unreadable link.
fn print_help() {
    out!(
        "localspace-desktop — the egui client, with Core in the same process

USAGE:
    localspace-desktop [OPTIONS]         open the egui Client

    doctor, bench, evals, call and audit are commands of the `localspace` binary.

OPTIONS:
    --harnesses <dir>     directory of harness packages to install at start
    --data <dir>          persist the database, blobs and audit log here (default: memory only)
    --registry <dir>      a catalog the Marketplace lists: a registry, or an offline bundle
    --user <name>         the environment's user
    --organisation        apply organisation policy (Tier B off by default)
"
    );
}

fn run_gui(args: Args) -> Result<()> {
    let cfg = config(&args);
    let machine = cfg.machine.clone();
    tracing::info!("machine: {}", machine.describe());

    let core = Core::new(cfg)?;
    let backend: Arc<dyn localspace_client::Backend> =
        Arc::new(Adapter(transport::InProcess::spawn(core)));

    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("localSpace"),
        ..Default::default()
    };
    // Diagnostics for a slow screen: `LOCALSPACE_PRESENT=immediate|mailbox|fifo|novsync|vsync`
    // picks the swapchain present mode, `LOCALSPACE_FRAME_LATENCY=1|2` the queue depth.
    if let Ok(mode) = std::env::var("LOCALSPACE_PRESENT") {
        options.wgpu_options.surface.present_mode = match mode.as_str() {
            "immediate" => eframe::wgpu::PresentMode::Immediate,
            "mailbox" => eframe::wgpu::PresentMode::Mailbox,
            "fifo" => eframe::wgpu::PresentMode::Fifo,
            "novsync" => eframe::wgpu::PresentMode::AutoNoVsync,
            _ => eframe::wgpu::PresentMode::AutoVsync,
        };
        tracing::info!("present mode: {mode}");
    }
    // `LOCALSPACE_GPU=<substring of an adapter name>` renders on that adapter — for
    // example `basic` for the CPU rasterizer — instead of the most powerful one.
    if let Ok(want) = std::env::var("LOCALSPACE_GPU") {
        let needle = want.to_lowercase();
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup
        {
            setup.native_adapter_selector = Some(std::sync::Arc::new(
                move |adapters: &[eframe::wgpu::Adapter],
                      _surface: Option<&eframe::wgpu::Surface<'_>>| {
                    adapters
                        .iter()
                        .find(|a| a.get_info().name.to_lowercase().contains(&needle))
                        .cloned()
                        .ok_or_else(|| format!("no GPU adapter matches {needle}"))
                },
            ));
        }
        tracing::info!("gpu selector: {want}");
    }
    if let Some(n) = std::env::var("LOCALSPACE_FRAME_LATENCY")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        options.wgpu_options.surface.desired_maximum_frame_latency = Some(n);
    }

    eframe::run_native(
        "localSpace",
        options,
        Box::new(move |cc| Ok(Box::new(localspace_client::App::new(cc, backend)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
