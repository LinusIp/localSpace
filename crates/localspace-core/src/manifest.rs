//! `harness.toml` — the package manifest, and the capability model it declares.
//!
//! Default deny: anything not declared here is unavailable to the harness.

use anyhow::{Context, Result, bail};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub harness: HarnessMeta,
    #[serde(default)]
    pub package: Package,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub resources: Resources,
    #[serde(default)]
    pub contributes: Contributes,
    #[serde(default)]
    pub dependencies: std::collections::BTreeMap<String, DepSpec>,
    #[serde(default)]
    pub provides: Provides,
    #[serde(default)]
    pub model_hints: ModelHints,
}

// ---------------------------------------------------------------------------
// Package management (spec §17)
// ---------------------------------------------------------------------------

/// `[package] kind = "harness"` — what this package is. Only a harness has a
/// surface and tools; a library or types package is linked, not run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Package {
    #[serde(default)]
    pub kind: PackageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    #[default]
    Harness,
    Library,
    Types,
    Template,
    ModelPack,
    Skill,
    Theme,
}

impl PackageKind {
    pub fn label(self) -> &'static str {
        match self {
            PackageKind::Harness => "harness",
            PackageKind::Library => "library",
            PackageKind::Types => "types",
            PackageKind::Template => "template",
            PackageKind::ModelPack => "model-pack",
            PackageKind::Skill => "skill",
            PackageKind::Theme => "theme",
        }
    }

    pub fn is_harness(self) -> bool {
        self == PackageKind::Harness
    }
}

/// One `[dependencies]` entry:
/// `"io.localspace.types.geometry" = "^1.2"`, or
/// `"io.localspace.mesh-viewer" = { version = "^2", optional = true }`, or
/// `"localspace.geometry.v1" = { interface = true }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DepSpec {
    Version(String),
    Detailed {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        optional: bool,
        #[serde(default)]
        interface: bool,
    },
}

impl DepSpec {
    pub fn version(&self) -> Option<&str> {
        match self {
            DepSpec::Version(v) => Some(v.as_str()),
            DepSpec::Detailed { version, .. } => version.as_deref(),
        }
    }

    pub fn optional(&self) -> bool {
        matches!(self, DepSpec::Detailed { optional: true, .. })
    }

    /// An interface dependency names a WIT interface, not a vendor: any
    /// installed package whose `[provides]` lists it satisfies it.
    pub fn is_interface(&self) -> bool {
        matches!(
            self,
            DepSpec::Detailed {
                interface: true,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provides {
    #[serde(default)]
    pub interfaces: Vec<String>,
}

impl Manifest {
    /// Dependencies in the resolver's shape.
    pub fn dependencies(&self) -> Vec<crate::deps::Dependency> {
        self.dependencies
            .iter()
            .map(|(id, spec)| crate::deps::Dependency {
                id: id.clone(),
                req: spec.version().map(|s| s.to_string()),
                optional: spec.optional(),
                interface: spec.is_interface(),
            })
            .collect()
    }
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
        matches!(
            self,
            GpuCap::Request {
                exclusive: true,
                ..
            }
        )
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
            model: self
                .model
                .iter()
                .map(|m| m.describe().to_string())
                .collect(),
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
        let added: Vec<&String> = new_hosts
            .iter()
            .filter(|h| !old_hosts.contains(h))
            .collect();
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
            out.push(format!(
                "gpu: {} -> {}",
                prev.gpu.describe(),
                self.gpu.describe()
            ));
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
    /// Interchange types this harness can import (spec §18.3), e.g. `outline.v1`.
    /// Core validates every handoff against this before the harness sees it.
    #[serde(default)]
    pub accepts: Vec<String>,
    /// Interchange types this harness can export. A tool result may register
    /// an artifact only of a kind listed here.
    #[serde(default)]
    pub produces: Vec<String>,
    /// `kind = "types"` only: the file that declares this package's
    /// interchange types (spec §18.3), normally `types.toml`.
    #[serde(default)]
    pub types: Option<String>,
}

/// `name.vN` — lower-case name, a version suffix. The strict form is what lets
/// Core match a producer to a consumer without guessing.
pub fn valid_interchange_kind(kind: &str) -> bool {
    let Some((name, version)) = kind.rsplit_once(".v") else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && !version.is_empty()
        && version.chars().all(|c| c.is_ascii_digit())
}

fn default_doc() -> DocKind {
    DocKind::Crdt
}

impl Default for Contributes {
    fn default() -> Self {
        Contributes {
            views: Vec::new(),
            tools: None,
            context_provider: false,
            doc: default_doc(),
            file_types: Vec::new(),
            logic: None,
            accepts: Vec::new(),
            produces: Vec::new(),
            types: None,
        }
    }
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
    /// Required for `kind = "egui"` (the surface wasm module) and for
    /// `kind = "web"` (the entry ES module, `ui/index.js`, whose directory is
    /// what the harness's origin serves).
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
    Web,
    Stream,
    /// Reserved (v2 §6.3): parses, refused at validation until step 8.
    Native,
}

impl From<SurfaceKind> for proto::SurfaceKind {
    fn from(k: SurfaceKind) -> Self {
        match k {
            SurfaceKind::Widgets => proto::SurfaceKind::Widgets,
            SurfaceKind::Egui => proto::SurfaceKind::Egui,
            SurfaceKind::Web => proto::SurfaceKind::Web,
            SurfaceKind::Stream => proto::SurfaceKind::Stream,
            SurfaceKind::Native => proto::SurfaceKind::Native,
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
        if !is_package_id(&self.harness.id) {
            bail!(
                "harness id `{}` is not reverse-DNS: lower-case letters, digits and dashes in at \
                 least two dot-separated parts (e.g. io.localspace.whiteboard)",
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
                _ => bail!(
                    "tier = \"native\" requires a `native_reason` shown in the install dialog"
                ),
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
            if v.kind == SurfaceKind::Web {
                match &v.module {
                    None => bail!("view `{}` is kind = \"web\" but declares no module", v.id),
                    Some(m) if !(m == "index.js" || m.ends_with("/index.js")) => bail!(
                        "view `{}` is kind = \"web\" but its module `{m}` is not an `index.js`: \
                         the entry module of a web view is `index.js` in the directory the harness's origin serves",
                        v.id
                    ),
                    Some(m) if m.starts_with('/') || m.contains("..") => bail!(
                        "view `{}`: module `{m}` must be a path inside the package",
                        v.id
                    ),
                    _ => {}
                }
            }
            if v.kind == SurfaceKind::Native {
                bail!(
                    "view `{}` is kind = \"native\": reserved for the Tier B runtime (architecture v2 §13 step 8) and not supported by this host, harness-api {}",
                    v.id,
                    proto::HARNESS_API
                );
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
        for kind in self
            .contributes
            .accepts
            .iter()
            .chain(self.contributes.produces.iter())
        {
            if !valid_interchange_kind(kind) {
                bail!(
                    "`{kind}` is not an interchange type; expected the form `name.vN`, e.g. outline.v1"
                );
            }
        }
        // Package kinds (spec §17.1): only a harness runs and has tools; a
        // library or types package is linked into others and must not pretend.
        if !self.package.kind.is_harness() {
            if !self.contributes.views.is_empty() || self.contributes.tools.is_some() {
                bail!(
                    "a `{}` package may not declare views or tools",
                    self.package.kind.label()
                );
            }
        } else if self.contributes.tools.is_none() {
            bail!("a harness needs `[contributes] tools`; without tools it is not agent-usable");
        }
        // A types package is its declarations (spec §18.3); nothing else has any.
        match (self.package.kind, &self.contributes.types) {
            (PackageKind::Types, None) => bail!(
                "a `types` package needs `[contributes] types = \"types.toml\"`, the file that declares its interchange types"
            ),
            (kind, Some(_)) if kind != PackageKind::Types => bail!(
                "only a `types` package may declare `[contributes] types`; this one is a `{}`",
                kind.label()
            ),
            _ => {}
        }
        for (id, spec) in &self.dependencies {
            if spec.is_interface() {
                if !id.contains('.') {
                    bail!("interface dependency `{id}` should be a dotted interface name");
                }
            } else {
                if !id.contains('.') && !id.contains('/') {
                    bail!(
                        "dependency `{id}` is not a package id (reverse-DNS, or <org-domain>/name)"
                    );
                }
                let Some(req) = spec.version() else {
                    bail!("dependency `{id}` needs a version requirement, e.g. \"^1.2\"");
                };
                if !crate::deps::valid_requirement(req) {
                    bail!("dependency `{id}` has an unreadable version requirement `{req}`");
                }
            }
        }
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

/// A package id: reverse-DNS, and nothing a path could be made of.
pub fn is_package_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_id_is_reverse_dns_and_never_a_path() {
        assert!(is_package_id("io.localspace.whiteboard"));
        assert!(is_package_id("acme-corp.tools.v2"));
        for bad in [
            "..",
            "../..",
            "whiteboard",
            "io.localspace.White",
            "io..x",
            "a/b.c",
            ".io.x",
            "io.x.",
        ] {
            assert!(!is_package_id(bad), "{bad}");
        }
        let escaped = WHITEBOARD.replace("io.localspace.whiteboard", "../..");
        let why = Manifest::parse(&escaped)
            .and_then(|m| m.validate())
            .expect_err("an id that is a path is refused")
            .to_string();
        assert!(why.contains("reverse-DNS"), "{why}");
    }

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
    fn a_types_package_is_its_declarations_and_nothing_else_has_any() {
        let types = r#"
[harness]
id = "io.localspace.types"
version = "1.0.0"
api = "^1.0"
title = "Interchange types"
publisher = "localSpace"

[package]
kind = "types"

[contributes]
types = "types.toml"
"#;
        let m = Manifest::parse(types).unwrap();
        assert_eq!(m.package.kind, PackageKind::Types);
        assert_eq!(m.contributes.types.as_deref(), Some("types.toml"));

        let without = types.replace("types = \"types.toml\"", "");
        let err = Manifest::parse(&without).unwrap_err().to_string();
        assert!(err.contains("types.toml"), "got: {err}");

        let harness = WHITEBOARD.replace(
            "tools = \"tools.json\"",
            "tools = \"tools.json\"\ntypes = \"types.toml\"",
        );
        let err = Manifest::parse(&harness).unwrap_err().to_string();
        assert!(err.contains("only a `types` package"), "got: {err}");
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
        assert_eq!(
            m.capabilities.net.hosts(),
            &["tiles.example.com".to_string()]
        );
        assert_eq!(
            m.capabilities.net.reason(),
            Some("map tiles for the site plan")
        );
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
        assert!(
            Manifest::parse(&zero)
                .unwrap_err()
                .to_string()
                .contains("positive")
        );

        let bad = text.replace("\"90s\"", "\"soon\"");
        assert!(
            Manifest::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("idle_unload")
        );
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
            gpu: GpuCap::Request {
                vram_gb: 24,
                exclusive: false,
            },
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

#[cfg(test)]
mod web_view_tests {
    use super::*;

    fn with_view(view: &str) -> Result<Manifest> {
        Manifest::parse(&format!(
            r#"
[harness]
id = "io.t.web"
version = "1.0.0"
api = "^1.0"
title = "T"
publisher = "p"

[contributes]
tools = "tools.json"
views = [{view}]
"#
        ))
        .and_then(|m| m.validate().map(|_| m))
    }

    #[test]
    fn a_web_view_needs_an_index_js_inside_the_package() {
        assert!(with_view(r#"{ id = "w", kind = "web", module = "ui/web/index.js", placement = "main", title = "W" }"#).is_ok());
        assert!(with_view(r#"{ id = "w", kind = "web", module = "index.js", placement = "main", title = "W" }"#).is_ok());
        for bad in [
            r#"{ id = "w", kind = "web", placement = "main", title = "W" }"#,
            r#"{ id = "w", kind = "web", module = "ui/web/app.js", placement = "main", title = "W" }"#,
            r#"{ id = "w", kind = "web", module = "../elsewhere/index.js", placement = "main", title = "W" }"#,
            r#"{ id = "w", kind = "web", module = "/abs/index.js", placement = "main", title = "W" }"#,
        ] {
            assert!(with_view(bad).is_err(), "{bad} must be refused");
        }
    }
}

#[cfg(test)]
mod native_view_tests {
    use super::*;

    #[test]
    fn a_native_view_parses_and_is_refused_with_a_reason() {
        let text = r#"
[harness]
id = "io.t.native"
version = "1.0.0"
api = "^1.0"
title = "T"
publisher = "p"
tier = "native"
native_reason = "a GPU solver"

[contributes]
tools = "tools.json"
views = [{ id = "scene", kind = "native", placement = "main", title = "Scene" }]
"#;
        // The kind is known to the parser: no "unknown variant" at the TOML level.
        let parsed: Manifest = toml::from_str(text).expect("a reserved kind must parse");
        assert_eq!(parsed.contributes.views[0].kind, SurfaceKind::Native);
        // What installation sees is the reason, from validation.
        let err = Manifest::parse(text).expect_err("and must be refused at validation");
        let message = format!("{err:#}");
        assert!(
            message.contains("reserved for the Tier B runtime"),
            "{message}"
        );
        assert!(
            message.contains(proto::HARNESS_API),
            "names the host's harness-api: {message}"
        );
        assert!(!message.contains("unknown variant"), "{message}");
    }
}
