//! `localspace.toml` (deployment §3.3): the keys this release honours, under
//! the specification's names, and nothing else. A key or value the release
//! does not honour is refused at start with its name and the reason, never
//! ignored: an operator who sets something believes it is in force (the
//! answers of 2026-09-13, `docs/DECISIONS.md`).

use anyhow::{Context, Result, bail};
use localspace_core::gateway::GatewayConfig;
use localspace_proto as proto;
use localspace_server::{Cidr, ServerConfig, Tls};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where the file is looked for when `--config` is not given.
pub fn default_path() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("localSpace")
            .join("localspace.toml")
    } else {
        PathBuf::from("/etc/localspace/localspace.toml")
    }
}

pub struct Loaded {
    pub config: ServerConfig,
    /// The file the settings came from; `None` for the defaults.
    pub source: Option<PathBuf>,
}

/// The settings: from the file named, from the default path when a file is
/// there, or the defaults.
pub fn load(explicit: Option<&Path>) -> Result<Loaded> {
    let path = match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => Some(default_path()).filter(|p| p.exists()),
    };
    let Some(path) = path else {
        return Ok(Loaded {
            config: ServerConfig::default(),
            source: None,
        });
    };
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let config = parse(&text, &path)?;
    Ok(Loaded {
        config,
        source: Some(path),
    })
}

/// The web client bundle: beside the executable in an installed copy,
/// `web/dist` in a checkout, or nothing (the server answers with a note).
pub fn web_root() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("web")));
    match beside {
        Some(dir) if dir.join("index.html").exists() => Some(dir),
        _ => Some(PathBuf::from("web/dist")).filter(|dir| dir.join("index.html").exists()),
    }
}

// ---------------------------------------------------------------------------
// The keys
// ---------------------------------------------------------------------------

/// Every key this release honours, by section.
const HONOURED: &[(&str, &[&str])] = &[
    (
        "server",
        &[
            "bind",
            "public_url",
            "tls",
            "trusted_proxies",
            "max_upload_mb",
            "surface_hosts",
        ],
    ),
    ("storage", &["root", "encryption"]),
    ("auth", &["provider", "session_ttl"]),
    ("models", &["dir"]),
    (
        "network",
        &["mode_ceiling", "allowlist", "blocklist", "search"],
    ),
    ("harnesses", &["registry", "catalogs"]),
    ("audit", &["sink"]),
    ("organisation", &["name"]),
];

/// Keys the specification has that this release does not honour, with the
/// reason; refused by name so nothing is believed to be in force that is not.
const NOT_IN_THIS_RELEASE: &[(&str, &str)] = &[
    (
        "server.clamav",
        "the upload scanner is not built in this release",
    ),
    (
        "secrets",
        "encryption at rest, and with it the master key, is scheduled before the first \
         enterprise pilot (deployment §9.1 as amended)",
    ),
    ("auth.issuer", "OIDC sign-in comes in Phase C"),
    ("auth.client_id", "OIDC sign-in comes in Phase C"),
    ("auth.client_secret", "OIDC sign-in comes in Phase C"),
    ("auth.group_claim", "OIDC sign-in comes in Phase C"),
    ("auth.admin_group", "OIDC sign-in comes in Phase C"),
    ("auth.scim", "SCIM is out of Pilot 1's scope"),
    (
        "models.default",
        "the model is chosen in Settings → Model in this release",
    ),
    ("models.embedding", "retrieval comes in Phase B"),
    ("models.vision", "a vision model is not in Pilot 1"),
    (
        "models.worker",
        "model workers beyond the one engine are not in Pilot 1",
    ),
    ("scheduler", "the scheduler's settings are not in Pilot 1"),
    (
        "network.proxy",
        "an outbound proxy is not built in this release",
    ),
    (
        "network.ca_bundle",
        "an outbound proxy is not built in this release",
    ),
    ("harnesses.tier_b", "Tier B is out of Pilot 1's scope"),
    ("harnesses.gpus", "Tier B is out of Pilot 1's scope"),
    ("harnesses.per_process", "Tier B is out of Pilot 1's scope"),
    (
        "harnesses.exclusive_queue_max",
        "Tier B is out of Pilot 1's scope",
    ),
    (
        "audit.retention_days",
        "retention is not enforced in this release",
    ),
    ("limits", "storage limits are not enforced in this release"),
];

fn check_keys(table: &toml::Table, path: &Path) -> Result<()> {
    let sections: Vec<&str> = HONOURED.iter().map(|(s, _)| *s).collect();
    for (section, value) in table {
        let Some((_, keys)) = HONOURED.iter().find(|(s, _)| s == section) else {
            if let Some((_, why)) = NOT_IN_THIS_RELEASE.iter().find(|(k, _)| k == section) {
                bail!(
                    "{}: `[{section}]` is in the specification (deployment §3.3) but {why}; \
                     remove it — nothing is honoured by being ignored",
                    path.display()
                );
            }
            bail!(
                "{}: `{section}` is not a section; the sections are: {}",
                path.display(),
                sections.join(", ")
            );
        };
        let Some(entries) = value.as_table() else {
            bail!(
                "{}: `{section}` must be a section, `[{section}]`, not a value",
                path.display()
            );
        };
        for key in entries.keys() {
            if keys.contains(&key.as_str()) {
                continue;
            }
            let dotted = format!("{section}.{key}");
            if let Some((_, why)) = NOT_IN_THIS_RELEASE.iter().find(|(k, _)| *k == dotted) {
                bail!(
                    "{}: `{dotted}` is in the specification (deployment §3.3) but {why}; \
                     remove it — nothing is honoured by being ignored",
                    path.display()
                );
            }
            bail!(
                "{}: `{dotted}` is not a setting; the keys under [{section}] are: {}",
                path.display(),
                keys.join(", ")
            );
        }
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    server: Server,
    #[serde(default)]
    storage: Storage,
    #[serde(default)]
    auth: Auth,
    #[serde(default)]
    models: Models,
    #[serde(default)]
    network: Network,
    #[serde(default)]
    harnesses: Harnesses,
    #[serde(default)]
    audit: Audit,
    #[serde(default)]
    organisation: Organisation,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Server {
    bind: Option<String>,
    public_url: Option<String>,
    tls: Option<TlsValue>,
    #[serde(default)]
    trusted_proxies: Vec<String>,
    max_upload_mb: Option<u64>,
    surface_hosts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TlsValue {
    Mode(String),
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Storage {
    root: Option<PathBuf>,
    encryption: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Auth {
    provider: Option<String>,
    session_ttl: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Models {
    dir: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Network {
    mode_ceiling: Option<String>,
    #[serde(default)]
    allowlist: Vec<String>,
    #[serde(default)]
    blocklist: Vec<String>,
    search: Option<Search>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    backend: String,
    url: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Harnesses {
    registry: Option<String>,
    #[serde(default)]
    catalogs: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Audit {
    sink: Option<Vec<String>>,
}

/// `[organisation] name`: shown on the sign-in page, in the tab title and
/// on invitations. Set at install; when absent the pages leave the
/// organisation unnamed rather than guess.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Organisation {
    name: Option<String>,
}

/// The file's text to a server configuration, or the one reason it is not.
pub fn parse(text: &str, path: &Path) -> Result<ServerConfig> {
    let table: toml::Table = text
        .parse()
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    check_keys(&table, path)?;
    let file: File =
        toml::from_str(text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    apply(file, path)
}

fn apply(file: File, path: &Path) -> Result<ServerConfig> {
    let mut cfg = ServerConfig::default();
    let at = path.display();

    // [organisation]
    if let Some(name) = file.organisation.name {
        let name = name.trim();
        if name.is_empty() {
            bail!("{at}: [organisation] name is empty; leave the key out until the name is known");
        }
        if name.chars().count() > 80 {
            bail!("{at}: [organisation] name is longer than 80 characters");
        }
        cfg.organisation = Some(name.to_string());
    }

    // [server]
    if let Some(bind) = file.server.bind {
        if bind.trim().is_empty() {
            bail!("{at}: [server] bind is empty");
        }
        cfg.bind = bind;
    }
    if let Some(url) = file.server.public_url {
        let url = url.trim().trim_end_matches('/').to_string();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            bail!("{at}: [server] public_url must start with https:// or http://");
        }
        cfg.public_url = Some(url);
    }
    cfg.tls = match file.server.tls {
        None => Tls::None,
        Some(TlsValue::Mode(mode)) if mode == "behind-proxy" => Tls::BehindProxy,
        Some(TlsValue::Mode(other)) => bail!(
            "{at}: [server] tls = \"{other}\" is not a mode; the one mode is \"behind-proxy\", \
             or a certificate and key as {{ cert = ..., key = ... }}"
        ),
        Some(TlsValue::Files { cert, key }) => bail!(
            "{at}: [server] tls names a certificate ({}) and a key ({}). TLS termination is not \
             built in yet; put localSpace behind a reverse proxy and set tls = \"behind-proxy\" \
             and trusted_proxies.",
            cert.display(),
            key.display()
        ),
    };
    for range in &file.server.trusted_proxies {
        let cidr: Cidr = range
            .parse()
            .map_err(|e| anyhow::anyhow!("{at}: [server] trusted_proxies: {e}"))?;
        if cidr.is_everything() {
            bail!(
                "{at}: [server] trusted_proxies: `{range}` trusts every address, so any client \
                 could name its own; list the proxies' own ranges"
            );
        }
        cfg.trusted_proxies.push(cidr);
    }
    if let Some(mb) = file.server.max_upload_mb {
        if mb == 0 || mb > 64 * 1024 {
            bail!("{at}: [server] max_upload_mb must be between 1 and 65536");
        }
        cfg.max_upload_mb = mb;
    }
    if let Some(hosts) = file.server.surface_hosts {
        if !hosts.contains("{slug}") {
            bail!(
                "{at}: [server] surface_hosts must contain {{slug}}, where the harness's name \
                 goes, like h-{{slug}}.apps.example.com"
            );
        }
        cfg.surface_hosts = hosts;
    }

    // [storage]
    if let Some(root) = file.storage.root {
        cfg.data = Some(root);
    }
    match file.storage.encryption.as_deref() {
        None | Some("off") => {}
        Some("at-rest") => bail!(
            "{at}: [storage] encryption = \"at-rest\" is scheduled before the first enterprise \
             pilot and is not built in this release (deployment §9.1 as amended); encrypt the \
             volume and set it to \"off\" or remove it"
        ),
        Some(other) => {
            bail!("{at}: [storage] encryption = \"{other}\" is not a value; use \"off\"")
        }
    }

    // [auth]
    match file.auth.provider.as_deref() {
        None | Some("local") => {}
        Some("oidc") | Some("saml") => bail!(
            "{at}: [auth] provider = \"{}\" comes in Phase C; this release has local accounts \
             only: set provider = \"local\" or remove the key",
            file.auth.provider.as_deref().unwrap_or_default()
        ),
        Some(other) => {
            bail!("{at}: [auth] provider = \"{other}\" is not a provider; use \"local\"")
        }
    }
    if let Some(ttl) = file.auth.session_ttl {
        let ms = parse_duration_ms(&ttl).ok_or_else(|| {
            anyhow::anyhow!(
                "{at}: [auth] session_ttl = \"{ttl}\" is not a duration like 12h, 90m or 7d"
            )
        })?;
        if !(60_000..=30 * 24 * 60 * 60 * 1000).contains(&ms) {
            bail!("{at}: [auth] session_ttl must be between 1m and 30d");
        }
        cfg.session_ttl_ms = ms;
    }

    // [models]
    if let Some(dir) = file.models.dir {
        cfg.models = Some(dir);
    }

    // [network]
    let mut gateway = GatewayConfig::default();
    if let Some(ceiling) = file.network.mode_ceiling {
        let mode = match ceiling.as_str() {
            "airgapped" => proto::NetworkMode::Airgapped,
            "ask" => proto::NetworkMode::Ask,
            "online" => proto::NetworkMode::Online,
            other => bail!(
                "{at}: [network] mode_ceiling = \"{other}\" is not a mode; airgapped, ask or online"
            ),
        };
        gateway.ceiling = mode;
        gateway.mode = mode;
    }
    gateway.allowlist = file.network.allowlist;
    gateway.blocklist = file.network.blocklist;
    if let Some(search) = file.network.search {
        if search.backend != "searxng" {
            bail!(
                "{at}: [network] search.backend = \"{}\" is not built in; the one backend is \
                 \"searxng\"",
                search.backend
            );
        }
        if !(search.url.starts_with("http://") || search.url.starts_with("https://")) {
            bail!("{at}: [network] search.url must start with http:// or https://");
        }
        gateway.search_url = Some(search.url);
    }
    cfg.gateway = gateway;

    // [harnesses]
    match file.harnesses.registry.as_deref() {
        None | Some("offline") => {}
        Some(other) => bail!(
            "{at}: [harnesses] registry = \"{other}\": an online registry is not built in this \
             release; set it to \"offline\" and ship the catalog as a directory in catalogs"
        ),
    }
    for dir in &file.harnesses.catalogs {
        if !dir.is_dir() {
            bail!(
                "{at}: [harnesses] catalogs names {}, which is not a directory",
                dir.display()
            );
        }
    }
    cfg.registry = file.harnesses.catalogs;

    // [audit]
    if let Some(sinks) = file.audit.sink
        && sinks.iter().any(|s| s != "local")
    {
        bail!(
            "{at}: [audit] sink: only the local sink is built in this release; SIEM export is \
             not in Pilot 1. Use [\"local\"] or remove the key"
        );
    }

    Ok(cfg)
}

/// `12h`, `90m`, `45s`, `7d` to milliseconds.
pub fn parse_duration_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(split);
    let n: u64 = number.parse().ok()?;
    let per_unit: u64 = match unit.trim() {
        "s" => 1000,
        "m" => 60 * 1000,
        "h" => 60 * 60 * 1000,
        "d" => 24 * 60 * 60 * 1000,
        _ => return None,
    };
    n.checked_mul(per_unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Result<ServerConfig> {
        parse(text, Path::new("/etc/localspace/localspace.toml"))
    }

    fn refused(text: &str) -> String {
        match parsed(text) {
            Ok(_) => panic!("accepted: {text}"),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn an_empty_file_is_the_defaults() {
        let cfg = parsed("").unwrap();
        let defaults = ServerConfig::default();
        assert_eq!(cfg.bind, defaults.bind);
        assert_eq!(cfg.tls, Tls::None);
        assert!(cfg.trusted_proxies.is_empty());
        assert_eq!(cfg.session_ttl_ms, defaults.session_ttl_ms);
        assert_eq!(cfg.max_upload_mb, 200);
    }

    #[test]
    fn the_pilot_s_file_loads_under_the_specification_s_names() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = dir.path().join("registry");
        std::fs::create_dir_all(&catalog).unwrap();
        let text = format!(
            r#"
[server]
bind = "0.0.0.0:8443"
public_url = "https://ai.corp.example/"
tls = "behind-proxy"
trusted_proxies = ["10.0.0.0/8", "127.0.0.1"]
max_upload_mb = 500
surface_hosts = "h-{{slug}}.ai.corp.example"

[storage]
root = "/var/lib/localspace"
encryption = "off"

[auth]
provider = "local"
session_ttl = "8h"

[models]
dir = "/var/lib/localspace/models"

[network]
mode_ceiling = "airgapped"
allowlist = ["*.wikipedia.org"]
blocklist = ["example.net"]
search = {{ backend = "searxng", url = "http://searxng.corp.example:8080" }}

[harnesses]
registry = "offline"
catalogs = [{:?}]

[audit]
sink = ["local"]
"#,
            catalog.to_string_lossy()
        );
        let cfg = parsed(&text).unwrap();
        assert_eq!(cfg.bind, "0.0.0.0:8443");
        assert_eq!(cfg.public_url.as_deref(), Some("https://ai.corp.example"));
        assert_eq!(cfg.tls, Tls::BehindProxy);
        assert_eq!(cfg.trusted_proxies.len(), 2);
        assert!(cfg.trusted_proxies[0].contains("10.20.30.40".parse().unwrap()));
        assert_eq!(cfg.max_upload_mb, 500);
        assert_eq!(cfg.surface_hosts, "h-{slug}.ai.corp.example");
        assert_eq!(cfg.data.as_deref(), Some(Path::new("/var/lib/localspace")));
        assert_eq!(cfg.session_ttl_ms, 8 * 60 * 60 * 1000);
        assert_eq!(
            cfg.models.as_deref(),
            Some(Path::new("/var/lib/localspace/models"))
        );
        assert_eq!(cfg.gateway.ceiling, proto::NetworkMode::Airgapped);
        assert_eq!(cfg.gateway.mode, proto::NetworkMode::Airgapped);
        assert_eq!(cfg.gateway.allowlist, vec!["*.wikipedia.org".to_string()]);
        assert_eq!(cfg.gateway.blocklist, vec!["example.net".to_string()]);
        assert_eq!(
            cfg.gateway.search_url.as_deref(),
            Some("http://searxng.corp.example:8080")
        );
        assert_eq!(cfg.registry, vec![catalog]);
    }

    #[test]
    fn the_organisation_s_name_is_taken_trimmed_and_an_empty_one_is_refused() {
        let cfg = parse(
            "[organisation]\nname = \" Meridian Bank \"\n",
            Path::new("t.toml"),
        )
        .unwrap();
        assert_eq!(cfg.organisation.as_deref(), Some("Meridian Bank"));
        assert_eq!(parse("", Path::new("t.toml")).unwrap().organisation, None);
        assert!(refused("[organisation]\nname = \"  \"\n").contains("leave the key out"));
        assert!(
            refused("[organisation]\nnam = \"x\"\n")
                .contains("`organisation.nam` is not a setting")
        );
    }

    #[test]
    fn a_proxy_range_that_is_every_address_is_refused() {
        assert!(refused("[server]\ntrusted_proxies = [\"0.0.0.0/0\"]\n").contains("every address"));
        assert!(refused("[server]\ntrusted_proxies = [\"::/0\"]\n").contains("every address"));
    }

    #[test]
    fn native_tls_is_refused_with_the_supported_path_named() {
        let why = refused(
            r#"
[server]
tls = { cert = "/etc/localspace/tls/fullchain.pem", key = "/etc/localspace/tls/privkey.pem" }
"#,
        );
        assert!(why.contains("not built in yet"), "{why}");
        assert!(why.contains("behind-proxy"), "{why}");
        assert!(why.contains("trusted_proxies"), "{why}");
    }

    #[test]
    fn a_key_the_release_does_not_honour_is_refused_by_name_with_the_reason() {
        let why = refused("[server]\nclamav = \"tcp://127.0.0.1:3310\"\n");
        assert!(why.contains("`server.clamav`"), "{why}");
        assert!(why.contains("not built in this release"), "{why}");
        assert!(
            why.contains("nothing is honoured by being ignored"),
            "{why}"
        );

        let why = refused("[models]\ndefault = \"llama\"\n");
        assert!(why.contains("`models.default`"), "{why}");

        let why = refused("[audit]\nretention_days = 730\n");
        assert!(why.contains("`audit.retention_days`"), "{why}");
        assert!(why.contains("not enforced"), "{why}");

        let why = refused("[limits]\nper_user_storage_gb = 20\n");
        assert!(why.contains("`[limits]`"), "{why}");

        let why = refused("[secrets]\nmaster_key = \"file:/etc/localspace/master.key\"\n");
        assert!(why.contains("`[secrets]`"), "{why}");
        assert!(why.contains("§9.1"), "{why}");
    }

    #[test]
    fn a_misspelt_key_is_refused_with_the_keys_that_exist() {
        let why = refused("[server]\nbnd = \"127.0.0.1:8443\"\n");
        assert!(why.contains("`server.bnd` is not a setting"), "{why}");
        assert!(why.contains("bind, public_url, tls"), "{why}");

        let why = refused("[sever]\nbind = \"127.0.0.1:8443\"\n");
        assert!(why.contains("`sever` is not a section"), "{why}");
        assert!(why.contains("server, storage, auth"), "{why}");
    }

    #[test]
    fn values_the_release_cannot_honour_are_refused() {
        assert!(refused("[auth]\nprovider = \"oidc\"\n").contains("Phase C"));
        assert!(
            refused("[storage]\nencryption = \"at-rest\"\n").contains("not built in this release")
        );
        assert!(
            refused("[audit]\nsink = [\"local\", \"syslog://siem:6514\"]\n").contains("local sink")
        );
        assert!(
            refused("[harnesses]\nregistry = \"https://registry.localspace.io\"\n")
                .contains("online registry")
        );
        assert!(
            refused("[network]\nsearch = { backend = \"bing\", url = \"https://x\" }\n")
                .contains("searxng")
        );
        assert!(
            refused("[network]\nmode_ceiling = \"open\"\n").contains("airgapped, ask or online")
        );
        assert!(refused("[server]\ntls = \"native\"\n").contains("behind-proxy"));
        assert!(refused("[server]\ntrusted_proxies = [\"10.0.0.0/40\"]\n").contains("prefix"));
        assert!(refused("[server]\nsurface_hosts = \"apps.example.com\"\n").contains("{slug}"));
        assert!(
            refused("[harnesses]\ncatalogs = [\"/nowhere/at/all\"]\n").contains("not a directory")
        );
    }

    #[test]
    fn durations_are_read_and_bounded() {
        assert_eq!(parse_duration_ms("12h"), Some(12 * 60 * 60 * 1000));
        assert_eq!(parse_duration_ms("90m"), Some(90 * 60 * 1000));
        assert_eq!(parse_duration_ms("45s"), Some(45_000));
        assert_eq!(parse_duration_ms("7d"), Some(7 * 24 * 60 * 60 * 1000));
        assert_eq!(parse_duration_ms(" 2 h "), Some(2 * 60 * 60 * 1000));
        assert_eq!(parse_duration_ms("12"), None);
        assert_eq!(parse_duration_ms("h"), None);
        assert_eq!(parse_duration_ms("12 hours"), None);
        assert!(refused("[auth]\nsession_ttl = \"31d\"\n").contains("between 1m and 30d"));
        assert!(refused("[auth]\nsession_ttl = \"soon\"\n").contains("not a duration"));
    }

    #[test]
    fn a_public_url_is_kept_without_its_trailing_slash_and_needs_a_scheme() {
        let cfg = parsed("[server]\npublic_url = \"https://ai.corp.example/\"\n").unwrap();
        assert_eq!(cfg.public_url.as_deref(), Some("https://ai.corp.example"));
        assert!(refused("[server]\npublic_url = \"ai.corp.example\"\n").contains("https://"));
    }
}
