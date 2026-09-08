//! `harness.toml` — the package manifest, and the capability model it declares.
//!
//! Default deny: anything not declared here is unavailable to the harness.

use anyhow::{bail, Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub harness: HarnessMeta,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub resources: Resources,
    pub contributes: Contributes,
    #[serde(default)]
    pub model_hints: ModelHints,
}

// ---------------------------------------------------------------------------
// Resources (spec §1.2): what a harness may cost, enforced by the runtime
// ---------------------------------------------------------------------------

/// `[resources] memory_mb = { logic = 32, surface = 16 }`, `idle_unload = "5m"`.
///
/// The logic limit is a wasm linear-memory ceiling; the surface limit is the
/// surface module's heap ceiling in the Client. A harness over its declaration
/// is killed, restarted, and reported — never silently allowed to grow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    #[serde(default)]
    pub memory_mb: MemoryBudget,
    /// The logic instance is dropped after this long without a call. Its
    /// document stays in the store; the next call re-instantiates it.
    #[serde(default = "default_idle_unload")]
    pub idle_unload: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBudget {
    #[serde(default = "default_logic_mb")]
    pub logic: u32,
    #[serde(default = "default_surface_mb")]
    pub surface: u32,
}

fn default_logic_mb() -> u32 {
    32
}

fn default_surface_mb() -> u32 {
    16
}

fn default_idle_unload() -> String {
    "5m".into()
}

impl Default for MemoryBudget {
    fn default() -> Self {
        MemoryBudget {
            logic: default_logic_mb(),
            surface: default_surface_mb(),
        }
    }
}

impl Default for Resources {
    fn default() -> Self {
        Resources {
            memory_mb: MemoryBudget::default(),
            idle_unload: default_idle_unload(),
        }
    }
}

impl Resources {
    pub fn idle_unload_duration(&self) -> std::time::Duration {
        parse_duration(&self.idle_unload).unwrap_or(std::time::Duration::from_secs(300))
    }

    pub fn summary(&self) -> proto::ResourceSummary {
        proto::ResourceSummary {
            logic_mb: self.memory_mb.logic,
            surface_mb: self.memory_mb.surface,
            idle_unload_secs: self.idle_unload_duration().as_secs(),
        }
    }
}

/// `30s`, `5m`, `2h`, or a bare number of seconds.
pub fn parse_duration(s: &str) -> Result<std::time::Duration> {
    let s = s.trim();
    let (num, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&s[..i], &s[i..]),
        None => (s, "s"),
    };
    let n: u64 = num
        .parse()
        .with_context(|| format!("`{s}` is not a duration like 30s, 5m or 2h"))?;
    let secs = match unit.trim() {
        "s" | "sec" | "secs" => n,
        "m" | "min" | "mins" => n * 60,
        "h" | "hr" | "hrs" => n * 3600,
        other => bail!("`{other}` is not a duration unit (use s, m or h)"),
    };
    Ok(std::time::Duration::from_secs(secs))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessMeta {
    /// Reverse-DNS, immutable across versions.
    pub id: String,
    pub version: String,
    /// Host `harness-api` semver range this package was built against.
    pub api: String,
    pub title: String,
    pub publisher: String,
    /// One or two sentences for the store page. Optional; the catalog falls back
    /// to the front-door tool summaries, which say what the harness is for anyway.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_tier")]
    pub tier: Tier,
    /// Required when `tier = "native"`; shown verbatim in the install dialog.
    #[serde(default)]
    pub native_reason: Option<String>,
}

fn default_tier() -> Tier {
    Tier::Wasm
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Wasm,
    Native,
}

impl From<Tier> for proto::Tier {
    fn from(t: Tier) -> Self {
        match t {
            Tier::Wasm => proto::Tier::Wasm,
            Tier::Native => proto::Tier::Native,
        }
    }
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub fs: FsCap,
    #[serde(default)]
    pub net: NetCap,
    #[serde(default)]
    pub gpu: GpuCap,
    #[serde(default)]
    pub spawn: bool,
    #[serde(default)]
    pub clipboard: ClipboardCap,
    #[serde(default)]
    pub docs: DocsCap,
    #[serde(default)]
    pub model: Vec<ModelCap>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            fs: FsCap::None,
            net: NetCap::default(),
            gpu: GpuCap::default(),
            spawn: false,
            clipboard: ClipboardCap::None,
            docs: DocsCap::None,
            model: Vec::new(),
        }
    }
}

/// `none` | `workspace` | `scoped:<subpath>`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FsCap {
    #[default]
    None,
    Workspace,
    Scoped(String),
}

impl FsCap {
    pub fn describe(&self) -> String {
        match self {
            FsCap::None => "none".into(),
            FsCap::Workspace => "workspace".into(),
            FsCap::Scoped(s) => format!("scoped:{s}"),
        }
    }

    pub fn parse(s: &str) -> Result<FsCap> {
        match s {
            "none" => Ok(FsCap::None),
            "workspace" => Ok(FsCap::Workspace),
            other => match other.strip_prefix("scoped:") {
                Some(p) if !p.is_empty() => Ok(FsCap::Scoped(p.to_string())),
                _ => bail!("fs capability `{other}` is not none | workspace | scoped:<subpath>"),
            },
        }
    }
}

impl Serialize for FsCap {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.describe())
    }
}

impl<'de> Deserialize<'de> for FsCap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        FsCap::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// `gpu = false`, or `gpu = { vram_gb = 24, exclusive = false }`.
///
/// A native harness that asks for a GPU is assigned one from the pool reserved
/// for harnesses — never a model worker's GPU. Where there is only one GPU, the
/// grant becomes a VRAM reservation in the placement plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GpuCap {
    Off(bool),
    Request {
        vram_gb: u32,
        #[serde(default)]
        exclusive: bool,
    },
}

impl Default for GpuCap {
    fn default() -> Self {
        GpuCap::Off(false)
    }
}

impl GpuCap {
    pub fn wanted(self) -> bool {
        !matches!(self, GpuCap::Off(false))
    }

    pub fn vram_gb(self) -> u32 {
        match self {
            GpuCap::Off(_) => 0,
            GpuCap::Request { vram_gb, .. } => vram_gb,
        }
    }

    pub fn exclusive(self) -> bool {
        matches!(self, GpuCap::Request { exclusive: true, .. })
    }

    pub fn describe(self) -> String {
        match self {
            GpuCap::Off(false) => "none".into(),
            GpuCap::Off(true) => "yes".into(),
            GpuCap::Request {
                vram_gb,
                exclusive: true,
            } => format!("{vram_gb} GB, exclusive"),
            GpuCap::Request { vram_gb, .. } => format!("{vram_gb} GB, shared"),
        }
    }
}

/// `net = "none"` (the default and expected value) or an explicit allowlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NetCap {
    Simple(String),
    Allowlist {
        allowlist: Vec<String>,
        #[serde(default)]
        reason: String,
    },
}

impl Default for NetCap {
    fn default() -> Self {
        NetCap::Simple("none".into())
    }
}

impl NetCap {
    pub fn hosts(&self) -> &[String] {
        match self {
            NetCap::Simple(_) => &[],
            NetCap::Allowlist { allowlist, .. } => allowlist,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, NetCap::Simple(s) if s == "none")
    }

    pub fn describe(&self) -> String {
        match self {
            NetCap::Simple(s) => s.clone(),
            NetCap::Allowlist { allowlist, .. } => format!("allowlist[{}]", allowlist.join(", ")),
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            NetCap::Simple(_) => None,
            NetCap::Allowlist { reason, .. } => Some(reason.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardCap {
    #[default]
    None,
    OnUserAction,
    Always,
}

impl ClipboardCap {
    pub fn describe(self) -> String {
        match self {
            ClipboardCap::None => "none",
            ClipboardCap::OnUserAction => "on-user-action",
            ClipboardCap::Always => "always",
        }
        .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocsCap {
    #[default]
    None,
    /// Retrieval via Core only, ACL enforced. Never gives the harness the index.
    Acl,
}

impl DocsCap {
    pub fn describe(self) -> String {
        match self {
            DocsCap::None => "none",
            DocsCap::Acl => "acl",
        }
        .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCap {
    Complete,
    Structured,
    Embed,
}

impl ModelCap {
    pub fn describe(self) -> &'static str {
        match self {
            ModelCap::Complete => "complete",
            ModelCap::Structured => "structured",
            ModelCap::Embed => "embed",
        }
    }
}

impl Capabilities {
    pub fn summary(&self) -> proto::CapabilitySummary {
        proto::CapabilitySummary {
            fs: self.fs.describe(),
            net: self.net.describe(),
            gpu: self.gpu.describe(),
            spawn: self.spawn,
            clipboard: self.clipboard.describe(),
            docs: self.docs.describe(),
            model: self.model.iter().map(|m| m.describe().to_string()).collect(),
        }
    }

    /// Human-readable lines describing what this package would gain over `prev`.
    /// An update that widens capabilities does not auto-install; this is the diff shown.
    pub fn widening_over(&self, prev: &Capabilities) -> Vec<String> {
        let mut out = Vec::new();
        if self.fs != prev.fs && self.fs != FsCap::None {
            out.push(format!(
                "filesystem: {} -> {}",
                prev.fs.describe(),
                self.fs.describe()
            ));
        }
        let (old_hosts, new_hosts) = (prev.net.hosts(), self.net.hosts());
        let added: Vec<&String> = new_hosts.iter().filter(|h| !old_hosts.contains(h)).collect();
        if !added.is_empty() {
            out.push(format!(
                "network: adds {}",
                added
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if self.gpu.wanted() && (!prev.gpu.wanted() || self.gpu.vram_gb() > prev.gpu.vram_gb()) {
            out.push(format!("gpu: {} -> {}", prev.gpu.describe(), self.gpu.describe()));
        }
        if self.spawn && !prev.spawn {
            out.push("spawn subprocesses: no -> yes".into());
        }
        if self.clipboard != prev.clipboard && self.clipboard != ClipboardCap::None {
            out.push(format!(
                "clipboard: {} -> {}",
                prev.clipboard.describe(),
                self.clipboard.describe()
            ));
        }
        if self.docs != prev.docs && self.docs != DocsCap::None {
            out.push("documents: gains ACL-filtered retrieval".into());
        }
        for m in &self.model {
            if !prev.model.contains(m) {
                out.push(format!("inference: adds model.{}", m.describe()));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Contributions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub views: Vec<ViewDecl>,
    /// Path to the tools file, relative to package root.
    #[serde(default)]
    pub tools: Option<String>,
    #[serde(default)]
    pub context_provider: bool,
    #[serde(default = "default_doc")]
    pub doc: DocKind,
    #[serde(default)]
    pub file_types: Vec<String>,
    /// Path to the logic module (wasm component) or native binary.
    #[serde(default)]
    pub logic: Option<String>,
}

fn default_doc() -> DocKind {
    DocKind::Crdt
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Crdt,
    Blob,
}

impl From<DocKind> for proto::DocKind {
    fn from(d: DocKind) -> Self {
        match d {
            DocKind::Crdt => proto::DocKind::Crdt,
            DocKind::Blob => proto::DocKind::Blob,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewDecl {
    pub id: String,
    pub kind: SurfaceKind,
    /// Required for `kind = "egui"`: path to the surface wasm module.
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default = "default_placement")]
    pub placement: Placement,
    #[serde(default)]
    pub title: Option<String>,
}

fn default_placement() -> Placement {
    Placement::Main
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Widgets,
    Egui,
    Stream,
}

impl From<SurfaceKind> for proto::SurfaceKind {
    fn from(k: SurfaceKind) -> Self {
        match k {
            SurfaceKind::Widgets => proto::SurfaceKind::Widgets,
            SurfaceKind::Egui => proto::SurfaceKind::Egui,
            SurfaceKind::Stream => proto::SurfaceKind::Stream,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Main,
    Side,
    Bottom,
}

impl From<Placement> for proto::Placement {
    fn from(p: Placement) -> Self {
        match p {
            Placement::Main => proto::Placement::Main,
            Placement::Side => proto::Placement::Side,
            Placement::Bottom => proto::Placement::Bottom,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelHints {
    #[serde(default)]
    pub prefers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Parsing and validation
// ---------------------------------------------------------------------------

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest> {
        let m: Manifest = toml::from_str(text).context("harness.toml is not valid TOML")?;
        m.validate()?;
        Ok(m)
    }

    pub fn load(dir: &Path) -> Result<Manifest> {
        let path = dir.join("harness.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Manifest::parse(&text)
    }

    /// Install-time gate. Core rejects a package here rather than at first use.
    pub fn validate(&self) -> Result<()> {
        if !self.harness.id.contains('.') {
            bail!(
                "harness id `{}` is not reverse-DNS (expected e.g. io.localspace.whiteboard)",
                self.harness.id
            );
        }
        if !api_range_accepts(&self.harness.api, proto::HARNESS_API) {
            bail!(
                "package needs harness-api {}, host provides {}",
                self.harness.api,
                proto::HARNESS_API
            );
        }
        if self.harness.tier == Tier::Native {
            match self.harness.native_reason.as_deref() {
                Some(r) if !r.trim().is_empty() => {}
                _ => bail!("tier = \"native\" requires a `native_reason` shown in the install dialog"),
            }
        }
        if self.harness.tier == Tier::Wasm && self.capabilities.gpu.wanted() {
            bail!("gpu capability requires tier = \"native\"");
        }
        if self.harness.tier == Tier::Wasm && self.capabilities.spawn {
            bail!("spawn capability requires tier = \"native\"");
        }
        for v in &self.contributes.views {
            if v.kind == SurfaceKind::Egui && v.module.is_none() {
                bail!("view `{}` is kind = \"egui\" but declares no module", v.id);
            }
            if v.kind == SurfaceKind::Stream && self.harness.tier != Tier::Native {
                bail!(
                    "view `{}` is kind = \"stream\", which only a native-tier harness can render",
                    v.id
                );
            }
        }
        if self.resources.memory_mb.logic == 0 || self.resources.memory_mb.surface == 0 {
            bail!("[resources] memory_mb must be positive for both logic and surface");
        }
        if self.resources.memory_mb.logic > 4096 {
            bail!(
                "[resources] memory_mb.logic = {} exceeds the 4 GB a wasm32 component can address",
                self.resources.memory_mb.logic
            );
        }
        parse_duration(&self.resources.idle_unload).context("[resources] idle_unload")?;
        if let NetCap::Allowlist { allowlist, reason } = &self.capabilities.net {
            if allowlist.is_empty() {
                bail!("net allowlist is empty; use net = \"none\"");
            }
            if reason.trim().is_empty() {
                bail!("net allowlist requires a `reason` shown at install");
            }
        }
        Ok(())
    }

    pub fn view(&self, id: &str) -> Option<&ViewDecl> {
        self.contributes.views.iter().find(|v| v.id == id)
    }
}

/// Minimal semver range check for the `api = "^1.2"` field.
/// Supports `^X.Y`, `~X.Y`, `X.Y`, and `*`.
pub fn api_range_accepts(range: &str, version: &str) -> bool {
    let v = parse_semver(version);
    let range = range.trim();
    if range == "*" {
        return true;
    }
    let (op, rest) = match range.chars().next() {
        Some('^') => ('^', &range[1..]),
        Some('~') => ('~', &range[1..]),
        Some('=') => ('=', &range[1..]),
        _ => ('^', range),
    };
    let r = parse_semver(rest);
    match op {
        '^' => v.0 == r.0 && (v.1, v.2) >= (r.1, r.2),
        '~' => v.0 == r.0 && v.1 == r.1 && v.2 >= r.2,
        _ => v == r,
    }
}

fn parse_semver(s: &str) -> (u32, u32, u32) {
    let mut it = s.trim().split('.');
    let a = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    let b = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    let c = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    (a, b, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITEBOARD: &str = r#"
[harness]
id = "io.localspace.whiteboard"
version = "1.4.0"
api = "^1.0"
title = "Whiteboard"
publisher = "localSpace"
tier = "wasm"

[capabilities]
fs = "workspace"
net = "none"
gpu = false
docs = "acl"
model = ["complete", "embed"]

[contributes]
views = [
  { id = "board", kind = "egui", module = "ui/board.wasm", placement = "main" },
  { id = "settings", kind = "widgets", placement = "side" },
]
tools = "tools.json"
context_provider = true
doc = "crdt"
file_types = [".lsboard"]
"#;

    #[test]
    fn parses_the_spec_manifest() {
        let m = Manifest::parse(WHITEBOARD).unwrap();
        assert_eq!(m.harness.id, "io.localspace.whiteboard");
        assert_eq!(m.harness.tier, Tier::Wasm);
        assert_eq!(m.capabilities.fs, FsCap::Workspace);
        assert!(m.capabilities.net.is_none());
        assert_eq!(m.contributes.views.len(), 2);
        assert_eq!(m.contributes.doc, DocKind::Crdt);
    }

    #[test]
    fn default_deny_when_capabilities_absent() {
        let text = r#"
[harness]
id = "io.localspace.notes"
version = "0.1.0"
api = "^1.0"
title = "Notes"
publisher = "x"

[contributes]
tools = "tools.json"
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.capabilities.fs, FsCap::None);
        assert!(m.capabilities.net.is_none());
        assert!(!m.capabilities.gpu.wanted());
        assert!(m.capabilities.model.is_empty());
        assert_eq!(m.capabilities.docs, DocsCap::None);
    }

    #[test]
    fn native_tier_must_justify_itself() {
        let text = r#"
[harness]
id = "io.localspace.physics"
version = "0.1.0"
api = "^1.0"
title = "Physics"
publisher = "x"
tier = "native"

[contributes]
tools = "tools.json"
"#;
        let err = Manifest::parse(text).unwrap_err().to_string();
        assert!(err.contains("native_reason"), "got: {err}");
    }

    #[test]
    fn gpu_requires_native_tier() {
        let text = r#"
[harness]
id = "io.localspace.render"
version = "0.1.0"
api = "^1.0"
title = "Render"
publisher = "x"
tier = "wasm"

[capabilities]
gpu = true

[contributes]
tools = "tools.json"
"#;
        let err = Manifest::parse(text).unwrap_err().to_string();
        assert!(err.contains("gpu"), "got: {err}");
    }

    #[test]
    fn net_allowlist_needs_a_reason() {
        let text = r#"
[harness]
id = "io.localspace.maps"
version = "0.1.0"
api = "^1.0"
title = "Maps"
publisher = "x"

[capabilities]
net = { allowlist = ["tiles.example.com"] }

[contributes]
tools = "tools.json"
"#;
        let err = Manifest::parse(text).unwrap_err().to_string();
        assert!(err.contains("reason"), "got: {err}");
    }

    #[test]
    fn net_allowlist_parses_when_complete() {
        let text = r#"
[harness]
id = "io.localspace.maps"
version = "0.1.0"
api = "^1.0"
title = "Maps"
publisher = "x"

[capabilities]
net = { allowlist = ["tiles.example.com"], reason = "map tiles for the site plan" }

[contributes]
tools = "tools.json"
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.capabilities.net.hosts(), &["tiles.example.com".to_string()]);
        assert_eq!(m.capabilities.net.reason(), Some("map tiles for the site plan"));
    }

    #[test]
    fn resources_default_to_the_spec_s_numbers() {
        let m = Manifest::parse(WHITEBOARD).unwrap();
        assert_eq!(m.resources.memory_mb.logic, 32);
        assert_eq!(m.resources.memory_mb.surface, 16);
        assert_eq!(m.resources.idle_unload_duration().as_secs(), 300);
    }

    #[test]
    fn resources_parse_and_are_bounded() {
        let text = r#"
[harness]
id = "io.localspace.big"
version = "0.1.0"
api = "^1.0"
title = "Big"
publisher = "x"

[resources]
memory_mb = { logic = 64, surface = 24 }
idle_unload = "90s"

[contributes]
tools = "tools.json"
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.resources.memory_mb.logic, 64);
        assert_eq!(m.resources.memory_mb.surface, 24);
        assert_eq!(m.resources.idle_unload_duration().as_secs(), 90);

        let zero = text.replace("logic = 64", "logic = 0");
        assert!(Manifest::parse(&zero).unwrap_err().to_string().contains("positive"));

        let bad = text.replace("\"90s\"", "\"soon\"");
        assert!(Manifest::parse(&bad).unwrap_err().to_string().contains("idle_unload"));
    }

    #[test]
    fn durations_read_the_way_people_write_them() {
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
        assert_eq!(parse_duration("5m").unwrap().as_secs(), 300);
        assert_eq!(parse_duration("2h").unwrap().as_secs(), 7200);
        assert_eq!(parse_duration("45").unwrap().as_secs(), 45);
        assert!(parse_duration("5 fortnights").is_err());
    }

    #[test]
    fn api_range_is_checked_against_the_host() {
        assert!(api_range_accepts("^1.0", "1.0.0"));
        assert!(api_range_accepts("^1.2", "1.4.0"));
        assert!(!api_range_accepts("^1.2", "1.1.0"));
        assert!(!api_range_accepts("^2.0", "1.9.9"));
        assert!(api_range_accepts("*", "3.1.4"));
    }

    #[test]
    fn capability_diff_lists_only_widenings() {
        let old = Capabilities::default();
        let new = Capabilities {
            fs: FsCap::Workspace,
            gpu: GpuCap::Request { vram_gb: 24, exclusive: false },
            model: vec![ModelCap::Complete],
            ..Capabilities::default()
        };
        let diff = new.widening_over(&old);
        assert!(diff.iter().any(|l| l.contains("filesystem")));
        assert!(diff.iter().any(|l| l.contains("gpu")));
        assert!(diff.iter().any(|l| l.contains("model.complete")));
        // Narrowing produces nothing to approve.
        assert!(old.widening_over(&new).is_empty());
    }
}
