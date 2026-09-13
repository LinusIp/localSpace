#![deny(unsafe_code)]
//! `localspace` — the one binary (deployment §3.1): the organisation server,
//! and the commands an operator runs beside it. Settings come from
//! `localspace.toml` (deployment §3.3); the command line carries only what
//! is decided at the moment of running.

mod admin;
mod settings;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use localspace_core::profile;
use std::path::PathBuf;

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
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => serve(args),
        Command::Admin(args) => admin::run(args),
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
