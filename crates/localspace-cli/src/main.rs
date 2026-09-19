#![deny(unsafe_code)]
//! `localspace` — the one binary (deployment §3.1): the organisation server,
//! and the commands an operator runs beside it. Settings come from
//! `localspace.toml` (deployment §3.3); the command line carries only what
//! is decided at the moment of running.

mod admin;
mod ops;
mod output;
mod settings;

use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use localspace_core::profile;
use std::path::PathBuf;

/// The build behind `--version`: packaging sets `LOCALSPACE_BUILD_ID` to
/// the commit it built from; a build without it says so.
const BUILD_ID: &str = match option_env!("LOCALSPACE_BUILD_ID") {
    Some(id) => id,
    None => "local build",
};

#[derive(Parser)]
#[command(
    name = "localspace",
    version,
    about = "localSpace: a self-hosted, offline-first AI workstation",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server: an organisation's, or one workstation's with --personal.
    Serve(ServeArgs),
    /// The operations that run with the service stopped: bootstrap, reset-password.
    Admin(admin::AdminArgs),
    /// Report the hardware profile and what will run on it.
    Doctor(ops::DoctorArgs),
    /// Report the efficiency budgets for this machine, on a scratch Core.
    Bench(ops::Common),
    /// Run a harness's agent-compatibility suite, on a scratch Core.
    Evals(ops::EvalsArgs),
    /// Invoke one tool on the data, the same way the agent would.
    Call(ops::CallArgs),
    /// The audit log: verify its hash chain across every file.
    Audit(ops::AuditArgs),
}

#[derive(Args)]
struct ServeArgs {
    /// The settings file (deployment §3.3). Without it, /etc/localspace/localspace.toml when
    /// that exists, else the defaults.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Serve plaintext off loopback anyway. Logged loudly; never for a company's documents.
    #[arg(long)]
    insecure: bool,
    /// Start on hardware under the supported floor (logged).
    #[arg(long)]
    allow_below_floor: bool,
    /// One user and no accounts: a workstation's server, signed in with a token.
    #[arg(long)]
    personal: bool,
    /// The token personal mode accepts; generated when absent.
    #[arg(long, value_name = "TOKEN", requires = "personal")]
    token: Option<String>,
}

fn main() -> Result<()> {
    // `--version` names the build as well as the version, once per process.
    let long_version: &'static str =
        Box::leak(format!("{} ({BUILD_ID})", env!("CARGO_PKG_VERSION")).into_boxed_str());
    let matches = Cli::command().long_version(long_version).get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    match cli.command {
        Command::Serve(args) => serve(args),
        Command::Admin(args) => admin::run(args),
        Command::Doctor(args) => ops::doctor(args),
        Command::Bench(common) => ops::bench(common),
        Command::Evals(args) => ops::evals(args),
        Command::Call(args) => ops::call(args),
        Command::Audit(args) => ops::audit(args),
    }
}

/// Anything wrong before the socket opens ends here: the one message, and
/// exit code 2, the usage-and-settings code.
fn refuse(message: impl std::fmt::Display) -> ! {
    eprintln!("localspace serve: {message}");
    std::process::exit(2)
}

fn serve(args: ServeArgs) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localspace=info,tower_http=info".into()),
        )
        .init();

    let loaded = match settings::load(args.config.as_deref()) {
        Ok(loaded) => loaded,
        Err(e) => refuse(format!("{e:#}")),
    };
    let mut cfg = loaded.config;
    match &loaded.source {
        Some(path) => tracing::info!("settings from {}", path.display()),
        None => tracing::info!("no settings file; running with the defaults"),
    }
    // The flags: what is decided at the moment of running, over the file.
    cfg.personal = args.personal;
    if args.personal {
        cfg.user = localspace_server::whoami();
        cfg.token = args.token;
    }
    cfg.allow_below_floor = args.allow_below_floor;
    cfg.insecure = args.insecure;
    if cfg
        .web_root
        .as_ref()
        .is_none_or(|dir| !dir.join("index.html").exists())
    {
        cfg.web_root = settings::web_root();
    }

    let machine = profile::Machine::detect();
    let tier = machine.tier();
    tracing::info!("machine: {}", machine.describe());
    if !tier.may_serve() {
        if tier.team_mode() {
            tracing::warn!(
                "team mode: this is a workstation profile, not a server. Expect one interactive \
                 stream and graceful queuing beyond that."
            );
        } else if cfg.personal {
            // One person's own computer is never refused (the answers of
            // 2026-09-18): it runs a model sized to it, and says which.
            tracing::info!(
                "this computer is smaller than a server: localSpace runs a model sized to it. \
                 `localspace doctor` says what to expect."
            );
        } else if !cfg.allow_below_floor {
            refuse(format!(
                "refusing to start below the supported floor.\nDetected: {}\nRun `localspace \
                 doctor` for the details, or pass --allow-below-floor (logged).",
                machine.describe()
            ));
        } else {
            tracing::warn!(
                "running below the supported hardware floor because --allow-below-floor was passed"
            );
        }
    }

    if let Err(refusal) = localspace_server::preflight(&cfg) {
        refuse(refusal);
    }
    let personal = cfg.personal;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let running = localspace_server::start(cfg).await?;
        if personal {
            tracing::info!(
                "sign in with the token {} (also written to <root>/token)",
                running.token
            );
        }
        running.task.await?;
        Ok(())
    })
}
