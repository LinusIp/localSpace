//! `localspace` — Core and Client in one process over the `InProcess` transport.
//!
//! The same binary also carries the two commands an operator needs before a GUI
//! is any use: `doctor`, which identifies the hardware profile and says what will
//! run on it, and `bench`, which reports the efficiency budgets for this machine.

use anyhow::Result;
use localspace_core::{planner, profile, transport, Config, Core};
use localspace_proto as proto;
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
    allow_below_floor: bool,
}

enum Command {
    Run,
    Doctor,
    Bench,
    Evals(String),
    /// Invoke one tool by name, the same way the agent would.
    Call(String, String),
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
        allow_below_floor: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "doctor" => args.command = Command::Doctor,
            "bench" => args.command = Command::Bench,
            "evals" => args.command = Command::Evals(it.next().unwrap_or_default()),
            "call" => {
                let tool = it.next().unwrap_or_default();
                let params = it.next().unwrap_or_else(|| "{}".into());
                args.command = Command::Call(tool, params);
            }
            "-h" | "--help" | "help" => args.command = Command::Help,
            "--harnesses" => args.harnesses = it.next().map(PathBuf::from),
            "--data" => args.data = it.next().map(PathBuf::from),
            "--registry" => args.registry.extend(it.next().map(PathBuf::from)),
            "--models" => args.models = it.next().map(PathBuf::from),
            "--llama-server" => args.llama_server = it.next().map(PathBuf::from),
            "--user" => args.user = it.next().unwrap_or_else(whoami),
            "--organisation" | "--organization" => args.organisation = true,
            "--allow-below-floor" => args.allow_below_floor = true,
            other => eprintln!("ignoring unknown argument `{other}`"),
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
        vec![PathBuf::from("registry")].into_iter().filter(|p| p.exists()).collect()
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
        Command::Doctor => doctor(&args),
        Command::Bench => bench(&args),
        Command::Evals(harness) => evals(&args, harness),
        Command::Call(tool, params) => call(&args, tool, params),
        Command::Run => run_gui(args),
    }
}

/// `localspace call <tool> '<json>'` — the same path the agent takes, so a script
/// and an agent cannot diverge: permission check, schema validation, confirmation
/// gate, DAG commit.
fn call(args: &Args, tool: &str, params: &str) -> Result<()> {
    let params: serde_json::Value = serde_json::from_str(params)
        .map_err(|e| anyhow::anyhow!("params must be JSON: {e}"))?;
    let mut core = Core::new(config(args))?;
    match core.handle(proto::Request::CallTool {
        tool: tool.to_string(),
        params: proto::Json(params),
    }) {
        proto::Response::ToolResult(outcome) => match outcome {
            proto::ToolOutcome::Ok {
                diff_summary,
                commit,
                ..
            } => {
                println!("ok: {diff_summary}");
                if let Some(c) = commit {
                    println!("commit {c}");
                }
                Ok(())
            }
            other => {
                eprintln!("{other:?}");
                std::process::exit(1);
            }
        },
        other => {
            eprintln!("{other:?}");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "localspace — a local agent workspace built from harnesses

USAGE:
    localspace [OPTIONS]                 open the desktop Client
    localspace doctor                    report the hardware profile and what will run on it
    localspace bench                     report the efficiency budgets for this machine
    localspace evals <harness id>        run a harness's agent-compatibility suite
    localspace call <tool> '<json>'      invoke one tool, the same way the agent would

OPTIONS:
    --harnesses <dir>     directory of harness packages to install at start
    --data <dir>          persist the DAG, blobs and audit log here (default: memory only)
    --registry <dir>      a catalog the Marketplace lists: a registry, or an offline bundle
    --user <name>         the environment's user
    --organisation        apply organisation policy (Tier B off by default)
    --allow-below-floor   proceed on hardware under the supported floor
"
    );
}

fn doctor(args: &Args) -> Result<()> {
    let machine = profile::Machine::detect();
    let tier = machine.tier();
    let model_profile = profile::ModelProfile::for_tier(tier);

    println!("machine   {}", machine.describe());
    println!("profile   {}", model_profile.name);
    println!(
        "budgets   tools {} tokens, working set {} tokens, {} prompt tokens per agent step",
        model_profile.tool_budget_tokens,
        model_profile.working_set_tokens,
        model_profile.prompt_tokens_per_step
    );
    println!();

    println!("reference models on this machine:");
    for map in [
        planner::reference_moe_100b_q4(),
        planner::reference_dense_70b_fp8(),
    ] {
        let plan = planner::plan(
            &map,
            &machine,
            &planner::PlanRequest {
                context_len: model_profile.working_set_tokens as u32,
                reservations: vec![
                    planner::Reservation::gb("harness pool", 6.0),
                    planner::Reservation::gb("draft model", 2.0),
                    planner::Reservation::gb("utility model", 2.5),
                ],
                ..Default::default()
            },
        );
        println!("  {:<28} {}", map.model_id, plan.summary());
        for note in &plan.notes {
            println!("      - {note}");
        }
    }
    println!();

    match tier {
        profile::HardwareTier::BelowFloor => {
            println!("verdict   below the supported floor (W32: 32 GB VRAM, 64 GB RAM, 16 cores).");
            println!("          The Client and every harness still run; a large local model will not.");
            println!("          Point the model picker at any OpenAI-compatible endpoint instead.");
            if !args.allow_below_floor {
                println!("          `serve` would refuse here without --allow-below-floor.");
            }
        }
        profile::HardwareTier::W32 | profile::HardwareTier::W96 => {
            println!("verdict   workstation profile. `serve` runs in team mode (<= 10 users).");
        }
        profile::HardwareTier::S => {
            println!("verdict   server profile. `serve` is supported here.");
        }
    }
    Ok(())
}

fn bench(args: &Args) -> Result<()> {
    let mut core = Core::new(config(args))?;
    let machine = profile::Machine::detect();
    let started = std::time::Instant::now();

    // Exercise the paths the budgets in §16.5 are about, without a model: the
    // active set, the grammar cache, the context providers and a DAG commit.
    let active = core.active_set();
    let blocks = core.context_blocks();
    let elapsed = started.elapsed();

    println!("machine                       {}", machine.describe());
    println!("harnesses installed           {}", core.environment().harnesses.len());
    println!(
        "active tool set               {} tools, ~{} of {} tokens",
        active.tools.len(),
        active.token_estimate,
        active.budget
    );
    println!("grammar                       {}", active.grammar_hash);
    println!("context blocks                {}", blocks.len());
    println!(
        "provider cache hit rate       {:.0}%",
        core.provider_cache_hit_rate() * 100.0
    );
    println!("turn assembly                 {:.2} ms", elapsed.as_secs_f32() * 1000.0);

    // §1.2: the app itself is budgeted at 50 MB private RSS, Client and Core
    // each. This process is both, with the harnesses instantiated, so it is the
    // honest upper bound — and it is printed whether or not it fits.
    let footprint = localspace_core::footprint::Footprint::measure();
    println!("app footprint (this process)  {}", footprint.describe());
    let resident = core
        .environment()
        .harnesses
        .iter()
        .filter(|h| h.loaded)
        .count();
    println!(
        "harness logic resident        {resident} of {} (idle instances unload after their declared idle_unload)",
        core.environment().harnesses.len()
    );

    // Run it again: everything below should now be served from cache.
    let started = std::time::Instant::now();
    core.active_set();
    core.context_blocks();
    println!(
        "turn assembly (cached)        {:.2} ms, provider hit rate {:.0}%",
        started.elapsed().as_secs_f32() * 1000.0,
        core.provider_cache_hit_rate() * 100.0
    );
    println!();
    println!("Budgets that need a loaded model (prompt-cache hit rate, decode tok/s,");
    println!("draft acceptance, utility-model share) are reported by `serve`'s /metrics.");
    Ok(())
}

fn evals(args: &Args, harness: &str) -> Result<()> {
    let mut core = Core::new(config(args))?;
    match core.handle(proto::Request::RunEvals {
        harness: harness.to_string(),
    }) {
        proto::Response::Evals(report) => {
            println!(
                "{}: {}/{} passed on {}",
                report.harness, report.passed, report.total, report.model
            );
            for case in &report.cases {
                println!(
                    "  [{}] {} — {}",
                    if case.passed { "pass" } else { "FAIL" },
                    case.name,
                    case.detail
                );
            }
            if report.passed < report.total {
                std::process::exit(1);
            }
            Ok(())
        }
        proto::Response::Error { message } => {
            eprintln!("{message}");
            std::process::exit(1);
        }
        other => {
            eprintln!("unexpected response: {other:?}");
            std::process::exit(1);
        }
    }
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
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
            setup.native_adapter_selector = Some(std::sync::Arc::new(
                move |adapters: &[eframe::wgpu::Adapter], _surface: Option<&eframe::wgpu::Surface<'_>>| {
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
    if let Some(n) = std::env::var("LOCALSPACE_FRAME_LATENCY").ok().and_then(|s| s.parse::<u32>().ok()) {
        options.wgpu_options.surface.desired_maximum_frame_latency = Some(n);
    }

    eframe::run_native(
        "localSpace",
        options,
        Box::new(move |cc| Ok(Box::new(localspace_client::App::new(cc, backend)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
