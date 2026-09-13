//! `localspace admin`: the two operations that run with the service stopped
//! (the fourth answer of 2026-09-13, `docs/DECISIONS.md`). Both open the
//! database directly, so a running server, which holds it, is refused by
//! name; there is no admin channel into a running server.

use crate::settings;
use anyhow::{Context, Result, bail};
use localspace_core::audit::{Actor, AuditLog, Scope};
use localspace_core::identity::{self, Directory};
use localspace_core::store::Store;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct AdminArgs {
    /// The settings file, which names the data directory (deployment §3.3).
    #[arg(long, value_name = "FILE", conflicts_with = "data", global = true)]
    config: Option<PathBuf>,
    /// The data directory itself, when there is no settings file.
    #[arg(long, value_name = "DIR", global = true)]
    data: Option<PathBuf>,
    #[command(subcommand)]
    command: AdminCommand,
}

#[derive(clap::Subcommand)]
enum AdminCommand {
    /// Mint the first administrator's one-time link, while there are no accounts. Service stopped.
    Bootstrap,
    /// Forget a user's password, end their sessions and print a new one-time link. Service stopped.
    ResetPassword {
        /// The account's email address.
        #[arg(long, value_name = "EMAIL")]
        email: String,
    },
}

/// Where the data is, the address users open, and how long sessions live.
struct Where {
    root: PathBuf,
    public_url: Option<String>,
    session_ttl_ms: u64,
}

fn locate(args: &AdminArgs) -> Result<Where> {
    if let Some(dir) = &args.data {
        return Ok(Where {
            root: dir.clone(),
            public_url: None,
            session_ttl_ms: localspace_server::ServerConfig::default().session_ttl_ms,
        });
    }
    let loaded = settings::load(args.config.as_deref())?;
    let cfg = loaded.config;
    let Some(root) = cfg.data else {
        bail!(
            "no data directory: set [storage] root in {} or pass --data <dir>",
            loaded
                .source
                .as_deref()
                .unwrap_or(&settings::default_path())
                .display()
        );
    };
    Ok(Where {
        root,
        public_url: cfg.public_url,
        session_ttl_ms: cfg.session_ttl_ms,
    })
}

/// The database, or the one reason it cannot be had: a running server.
fn open_store(root: &Path) -> Result<Store> {
    match Store::open(root) {
        Ok(store) => Ok(store),
        Err(e) => {
            let text = format!("{e:#}").to_lowercase();
            if text.contains("lock")
                || text.contains("already open")
                || text.contains("being used by another process")
                || text.contains("os error 32")
                || text.contains("os error 33")
            {
                bail!(
                    "the database under {} is held by a running server. Stop the service first \
                     (`systemctl stop localspace`), run this again, then start it.",
                    root.display()
                );
            }
            Err(e)
        }
    }
}

fn operator() -> Actor {
    Actor {
        user: "operator".into(),
        session: "cli".into(),
        ip: String::new(),
        role: "operator".into(),
        break_glass: None,
    }
}

fn link_for(public_url: Option<&str>, token: &str) -> String {
    match public_url {
        Some(base) => format!("{}/invite/{token}", base.trim_end_matches('/')),
        None => format!("/invite/{token}"),
    }
}

pub fn run(args: AdminArgs) -> Result<()> {
    let at = locate(&args)?;
    let store = open_store(&at.root)?;
    let directory = Directory::new(store.clone(), at.session_ttl_ms);
    let audit = AuditLog::open(&at.root.join("audit")).context("opening the audit log")?;
    let now = localspace_core::dag::now_ms();
    match args.command {
        AdminCommand::Bootstrap => {
            let accounts = directory.users()?.len();
            if accounts > 0 {
                bail!(
                    "this server has {accounts} account(s) already; the first administrator's \
                     link is only for a server with none. To recover an administrator, run \
                     `localspace admin reset-password --email <their email>`."
                );
            }
            let token = directory.bootstrap_invite(now)?;
            let link = link_for(at.public_url.as_deref(), &token);
            let path = identity::write_first_admin_link(&at.root, &link)?;
            let _ = audit.append(
                operator(),
                Scope::default(),
                "user.first_admin_link",
                serde_json::json!({"by": "localspace admin bootstrap"}),
                "ok",
            );
            audit.flush();
            println!("First administrator: open this link within 24 hours.");
            println!();
            println!("  {link}");
            println!();
            if at.public_url.is_none() {
                println!(
                    "(The settings file was not read, so the link has no address in front of it: put your server's address before it.)"
                );
            }
            println!(
                "Also written to {}; it is deleted when the link is used.",
                path.display()
            );
            Ok(())
        }
        AdminCommand::ResetPassword { email } => {
            let wanted = identity::normalise_email(&email);
            let Some(user) = store.user_by_email(&wanted)? else {
                bail!("there is no account for {wanted}");
            };
            let token = directory.reset_password(&user.id, now)?;
            directory.unlock(&user.id)?;
            let link = link_for(at.public_url.as_deref(), &token);
            let _ = audit.append(
                operator(),
                Scope::default(),
                "user.password_reset",
                serde_json::json!({"user": user.id, "email": user.email, "by": "localspace admin reset-password"}),
                "ok",
            );
            audit.flush();
            println!(
                "{}'s password is forgotten and every session of theirs is ended. Give them this link; it sets a new password and is good for 24 hours:",
                user.email
            );
            println!();
            println!("  {link}");
            if at.public_url.is_none() {
                println!();
                println!(
                    "(The settings file was not read, so the link has no address in front of it: put your server's address before it.)"
                );
            }
            Ok(())
        }
    }
}
