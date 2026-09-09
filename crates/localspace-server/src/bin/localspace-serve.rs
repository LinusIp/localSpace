//! `localspace serve` — the organisation server, and the same binary a team
//! runs on one workstation.

use localspace_core::profile;
use localspace_server::ServerConfig;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localspace=info,tower_http=info".into()),
        )
        .init();

    let cfg = parse_args();
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
            eprintln!(
                "localspace serve refuses to start below the supported floor.\n\
                 Detected: {}\n\
                 Run `localspace doctor` for the details, or pass --allow-below-floor \
                 (logged, and shown permanently in the console).",
                machine.describe()
            );
            std::process::exit(2);
        } else {
            tracing::warn!(
                "running below the supported hardware floor because --allow-below-floor was passed"
            );
        }
    }

    let running = localspace_server::start(cfg).await?;
    tracing::info!(
        "sign in with the token {} (also written to <data>/token)",
        running.token
    );
    running.task.await?;
    Ok(())
}

fn parse_args() -> ServerConfig {
    let mut cfg = ServerConfig::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bind" => cfg.bind = it.next().unwrap_or(cfg.bind),
            "--harnesses" => cfg.harnesses = it.next().map(PathBuf::from),
            "--data" => cfg.data = it.next().map(PathBuf::from),
            "--registry" => cfg.registry.extend(it.next().map(PathBuf::from)),
            "--web" => cfg.web_root = it.next().map(PathBuf::from),
            "--token" => cfg.token = it.next(),
            "--user" => cfg.user = it.next().unwrap_or(cfg.user),
            "--personal" => cfg.personal = true,
            "--secure-cookies" => cfg.secure_cookies = true,
            "--allow-below-floor" => cfg.allow_below_floor = true,
            other => eprintln!("ignoring unknown argument `{other}`"),
        }
    }
    cfg
}
