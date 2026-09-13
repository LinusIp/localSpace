//! The operator's commands beside `serve`: `doctor`, `bench`, `evals`,
//! `call` and `audit verify`. Each reads the same settings file as `serve`
//! (deployment §3.3); `bench` and `evals` run on a scratch Core in memory
//! with the catalogs the file names, so a measurement or a test never
//! touches the data; `call` acts on the data the way the agent would.

use crate::output::{err, out};
use crate::settings;
use anyhow::{Context, Result, bail};
use localspace_core::{Core, planner, profile};
use localspace_proto as proto;
use localspace_server::{ServerConfig, core_config};
use std::path::PathBuf;

/// What every command takes: the settings file.
#[derive(clap::Args)]
pub struct Common {
    /// The settings file (deployment §3.3). Without it, /etc/localspace/localspace.toml when
    /// that exists, else the defaults.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    common: Common,
    /// Report as `serve --allow-below-floor` would run.
    #[arg(long)]
    allow_below_floor: bool,
}

#[derive(clap::Args)]
pub struct EvalsArgs {
    /// The harness, by id, from the catalogs the settings name.
    harness: String,
    #[command(flatten)]
    common: Common,
}

#[derive(clap::Args)]
pub struct CallArgs {
    /// The tool, fully qualified: `canvas.add_sticky`.
    tool: String,
    /// Its parameters as JSON; `{}` when absent.
    params: Option<String>,
    #[command(flatten)]
    common: Common,
    /// Act in the workstation's personal mode rather than as the organisation's operator.
    #[arg(long)]
    personal: bool,
}

#[derive(clap::Args)]
pub struct AuditArgs {
    #[command(subcommand)]
    command: AuditCommand,
    /// The settings file, which names the data directory (deployment §3.3).
    #[arg(long, value_name = "FILE", conflicts_with = "data", global = true)]
    config: Option<PathBuf>,
    /// The data directory itself, when there is no settings file.
    #[arg(long, value_name = "DIR", global = true)]
    data: Option<PathBuf>,
}

#[derive(clap::Subcommand)]
enum AuditCommand {
    /// Walk the audit log's hash chain across every file, oldest first.
    Verify,
}

fn load(common: &Common) -> Result<ServerConfig> {
    Ok(settings::load(common.config.as_deref())?.config)
}

/// A Core on nothing but the catalogs: for measurements and tests.
fn scratch_core(cfg: &ServerConfig) -> Result<Core> {
    let mut core_cfg = core_config(cfg);
    core_cfg.data_dir = None;
    Core::new(core_cfg)
}

/// Whether an error is the database held by a running server, which every
/// command that opens the data meets the same way.
pub fn held_by_server(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_lowercase();
    text.contains("already open")
        || text.contains("lock")
        || text.contains("being used by another process")
}

fn explain(error: anyhow::Error, root: Option<&std::path::Path>) -> anyhow::Error {
    if held_by_server(&error) {
        return anyhow::anyhow!(
            "the database under {} is held by a running server. Stop the service first \
             (`systemctl stop localspace`), run this again, then start it.",
            root.map(|p| p.display().to_string())
                .unwrap_or_else(|| "the data directory".into())
        );
    }
    error
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

pub fn doctor(args: DoctorArgs) -> Result<()> {
    let loaded = settings::load(args.common.config.as_deref())?;
    let machine = profile::Machine::detect();
    let tier = machine.tier();
    let model_profile = profile::ModelProfile::for_tier(tier);

    match &loaded.source {
        Some(path) => out!("settings  {}", path.display()),
        None => out!("settings  none (the defaults)"),
    }
    out!("machine   {}", machine.describe());
    out!("profile   {}", model_profile.name);
    out!(
        "budgets   tools {} tokens, working set {} tokens, {} prompt tokens per agent step",
        model_profile.tool_budget_tokens,
        model_profile.working_set_tokens,
        model_profile.prompt_tokens_per_step
    );
    out!();

    out!("reference models on this machine:");
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
        out!("  {:<28} {}", map.model_id, plan.summary());
        for note in &plan.notes {
            out!("      - {note}");
        }
    }
    out!();

    match tier {
        profile::HardwareTier::BelowFloor => {
            out!("verdict   below the supported floor (W32: 32 GB VRAM, 64 GB RAM, 16 cores).");
            out!("          The client and every harness still run; a large local model will not.");
            if !args.allow_below_floor {
                out!("          `serve` would refuse here without --allow-below-floor.");
            }
        }
        profile::HardwareTier::W32 | profile::HardwareTier::W96 => {
            out!("verdict   workstation profile. `serve` runs in team mode (<= 10 users).");
        }
        profile::HardwareTier::S => {
            out!("verdict   server profile. `serve` is supported here.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// bench
// ---------------------------------------------------------------------------

/// Every harness the catalogs offer, installed into a scratch Core.
fn install_catalog(core: &mut Core) -> Result<Vec<String>> {
    let entries = match core.handle(proto::Request::ListCatalog) {
        proto::Response::Catalog { entries } => entries,
        other => bail!("the catalog could not be read: {other:?}"),
    };
    let mut installed = Vec::new();
    for entry in entries
        .iter()
        .filter(|e| e.kind == "harness" && e.blocked.is_none() && !e.installed)
    {
        if install(core, &entry.id, &entry.path)? {
            installed.push(entry.id.clone());
        }
    }
    Ok(installed)
}

/// Install one package into a scratch Core. Widened capabilities are
/// approved: nothing here outlives the command.
fn install(core: &mut Core, id: &str, path: &str) -> Result<bool> {
    match core.handle(proto::Request::InstallHarness { path: path.into() }) {
        proto::Response::Ok => Ok(true),
        proto::Response::InstallPrompt { token, diff, .. } => {
            out!(
                "  `{id}` asks for {} (approved for this run only: nothing here is kept)",
                diff.join(", ")
            );
            match core.handle(proto::Request::ApproveInstall {
                harness: id.to_string(),
                token,
            }) {
                proto::Response::Ok => Ok(true),
                other => bail!("`{id}` could not be installed: {other:?}"),
            }
        }
        proto::Response::Error { message } => {
            err!("  `{id}` could not be installed: {message}");
            Ok(false)
        }
        other => bail!("`{id}` could not be installed: {other:?}"),
    }
}

pub fn bench(common: Common) -> Result<()> {
    let cfg = load(&common)?;
    let mut core = scratch_core(&cfg)?;
    let machine = profile::Machine::detect();
    let installed = install_catalog(&mut core)?;
    let started = std::time::Instant::now();

    // Exercise the paths the budgets in §16.5 are about, without a model: the
    // active set, the grammar cache, the context providers and a DAG commit.
    let active = core.active_set();
    let blocks = core.context_blocks();
    let elapsed = started.elapsed();

    out!("machine                       {}", machine.describe());
    out!(
        "harnesses installed           {} (from the catalogs, for this run)",
        installed.len()
    );
    out!(
        "active tool set               {} tools, ~{} of {} tokens",
        active.tools.len(),
        active.token_estimate,
        active.budget
    );
    out!("grammar                       {}", active.grammar_hash);
    out!("context blocks                {}", blocks.len());
    out!(
        "provider cache hit rate       {:.0}%",
        core.provider_cache_hit_rate() * 100.0
    );
    out!(
        "turn assembly                 {:.2} ms",
        elapsed.as_secs_f32() * 1000.0
    );

    // §1.2: Core is budgeted at 50 MB private RSS. This process is Core with
    // the harnesses instantiated, so it is the honest upper bound — and it is
    // printed whether or not it fits.
    let footprint = localspace_core::footprint::Footprint::measure();
    out!("footprint (this process)      {}", footprint.describe());
    let resident = core
        .environment()
        .harnesses
        .iter()
        .filter(|h| h.loaded)
        .count();
    out!(
        "harness logic resident        {resident} of {} (idle instances unload after their declared idle_unload)",
        core.environment().harnesses.len()
    );

    // Run it again: everything below should now be served from cache.
    let started = std::time::Instant::now();
    core.active_set();
    core.context_blocks();
    out!(
        "turn assembly (cached)        {:.2} ms, provider hit rate {:.0}%",
        started.elapsed().as_secs_f32() * 1000.0,
        core.provider_cache_hit_rate() * 100.0
    );
    out!();
    out!("Budgets that need a loaded model (prompt-cache hit rate, decode tok/s,");
    out!("draft acceptance, utility-model share) are reported by `serve`'s /metrics.");
    Ok(())
}

// ---------------------------------------------------------------------------
// evals
// ---------------------------------------------------------------------------

pub fn evals(args: EvalsArgs) -> Result<()> {
    let cfg = load(&args.common)?;
    let mut core = scratch_core(&cfg)?;
    let entries = match core.handle(proto::Request::ListCatalog) {
        proto::Response::Catalog { entries } => entries,
        other => bail!("the catalog could not be read: {other:?}"),
    };
    let Some(entry) = entries.iter().find(|e| e.id == args.harness) else {
        let known: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        bail!(
            "no package `{}` in the catalogs the settings name ([harnesses] catalogs); \
             there: {}",
            args.harness,
            if known.is_empty() {
                "nothing".to_string()
            } else {
                known.join(", ")
            }
        );
    };
    if let Some(why) = &entry.blocked {
        bail!("`{}` cannot be installed here: {why}", entry.id);
    }
    if !install(&mut core, &entry.id, &entry.path)? {
        std::process::exit(1);
    }
    match core.handle(proto::Request::RunEvals {
        harness: args.harness.clone(),
    }) {
        proto::Response::Evals(report) => {
            out!(
                "{}: {}/{} passed on {}",
                report.harness,
                report.passed,
                report.total,
                report.model
            );
            for case in &report.cases {
                out!(
                    "  [{}] {} — {}",
                    if case.passed { "pass" } else { "FAIL" },
                    case.name,
                    case.detail
                );
            }
            if report.model.contains("no model") {
                out!("No model is loaded in this run, so only the checks that need none can pass.");
            }
            if report.passed < report.total {
                std::process::exit(1);
            }
            Ok(())
        }
        proto::Response::Error { message } => {
            err!("{message}");
            std::process::exit(1);
        }
        other => {
            err!("unexpected response: {other:?}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// call
// ---------------------------------------------------------------------------

/// `localspace call <tool> '<json>'` — the same path the agent takes, so a
/// script and an agent cannot diverge: permission check, schema validation,
/// confirmation gate, commit. On the data the settings name.
pub fn call(args: CallArgs) -> Result<()> {
    let params: serde_json::Value = serde_json::from_str(args.params.as_deref().unwrap_or("{}"))
        .map_err(|e| anyhow::anyhow!("params must be JSON: {e}"))?;
    let mut cfg = load(&args.common)?;
    cfg.personal = args.personal;
    if args.personal {
        cfg.user = localspace_server::whoami();
    }
    let root = cfg.data.clone();
    let mut core = Core::new(core_config(&cfg)).map_err(|e| explain(e, root.as_deref()))?;
    match core.handle(proto::Request::CallTool {
        tool: args.tool.clone(),
        params: proto::Json(params),
    }) {
        proto::Response::ToolResult(outcome) => match outcome {
            proto::ToolOutcome::Ok {
                diff_summary,
                commit,
                ..
            } => {
                out!("ok: {diff_summary}");
                if let Some(c) = commit {
                    out!("commit {c}");
                }
                Ok(())
            }
            other => {
                err!("{other:?}");
                std::process::exit(1);
            }
        },
        other => {
            err!("{other:?}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// audit verify
// ---------------------------------------------------------------------------

/// `localspace audit verify` — walks every file of the audit log in order
/// and checks each record's hash against the one before it, across files
/// (deployment §10.1). Exit 1 at the first broken or unreadable link.
pub fn audit(args: AuditArgs) -> Result<()> {
    let root = match &args.data {
        Some(dir) => dir.clone(),
        None => {
            let loaded = settings::load(args.config.as_deref())?;
            loaded.config.data.with_context(|| {
                format!(
                    "no data directory: set [storage] root in {} or pass --data <dir>",
                    loaded
                        .source
                        .as_deref()
                        .unwrap_or(&settings::default_path())
                        .display()
                )
            })?
        }
    };
    let dir = root.join("audit");
    match args.command {
        AuditCommand::Verify => match localspace_core::audit::verify_dir(&dir) {
            Ok(report) => {
                match (&report.first_day, &report.last_day) {
                    (Some(first), Some(last)) if report.records > 0 => out!(
                        "audit: {} record(s) in {} file(s), {first} to {last}; the chain is intact",
                        report.records,
                        report.files
                    ),
                    _ => out!("audit: no records in {}", dir.display()),
                }
                Ok(())
            }
            Err(e) => {
                err!("audit: {e}");
                std::process::exit(1);
            }
        },
    }
}
