#![deny(unsafe_code)]
//! `localspace-proto` — the entire Client<->Core API.
//!
//! Rule from the spec: if a feature needs a call that is not in `proto`, it is added
//! to `proto` — never as a desktop-only shortcut, or the server mode rots.
//!
//! Wire format is `postcard` for the WebSocket transport and plain moves for the
//! in-process transport. Both use exactly these types.

use serde::{Deserialize, Serialize};

pub const HARNESS_API: &str = "1.0.0";
/// Bumped whenever the epaint shape schema crossing the surface ABI changes.
pub const SHAPE_SCHEMA: u32 = 2;

/// A JSON value that can cross a non-self-describing wire.
///
/// `postcard` cannot deserialize `serde_json::Value` (it has no `deserialize_any`),
/// so every JSON payload in `proto` travels as its compact string form and is
/// re-parsed on arrival. Same type on both transports; no desktop-only shortcut.
#[derive(Debug, Clone, PartialEq, schemars::JsonSchema, ts_rs::TS)]
pub struct Json(pub serde_json::Value);

impl Json {
    pub fn null() -> Self {
        Json(serde_json::Value::Null)
    }
    pub fn object() -> Self {
        Json(serde_json::Value::Object(Default::default()))
    }
    pub fn into_inner(self) -> serde_json::Value {
        self.0
    }
}

impl Default for Json {
    fn default() -> Self {
        Json::null()
    }
}

impl From<serde_json::Value> for Json {
    fn from(v: serde_json::Value) -> Self {
        Json(v)
    }
}

impl From<Json> for serde_json::Value {
    fn from(j: Json) -> Self {
        j.0
    }
}

impl std::ops::Deref for Json {
    type Target = serde_json::Value;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Json {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // A JSON wire carries the value itself. A binary wire such as postcard,
        // which cannot describe a JSON value, carries its text and re-parses it.
        if s.is_human_readable() {
            self.0.serialize(s)
        } else {
            s.serialize_str(&self.0.to_string())
        }
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            serde_json::Value::deserialize(d).map(Json)
        } else {
            let raw = String::deserialize(d)?;
            serde_json::from_str(&raw)
                .map(Json)
                .map_err(serde::de::Error::custom)
        }
    }
}

/// An organisation role (deployment §4.3, the three of Pilot 1). What a
/// user may do to a document is the document's level (§6.1), not the role.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Member,
    Viewer,
}

impl UserRole {
    pub fn label(self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Member => "member",
            UserRole::Viewer => "viewer",
        }
    }
}

pub type HarnessId = String;
pub type ViewId = String;
pub type DocId = String;
pub type CommitId = String;
pub type JobId = String;

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No egress at all. `web.*` tools are absent from the agent's tool set.
    Airgapped,
    /// `web.*` exist; first use per domain per session raises an inline approval.
    Ask,
    /// Egress allowed without prompting, still allowlisted and logged.
    Online,
}

impl NetworkMode {
    pub fn label(self) -> &'static str {
        match self {
            NetworkMode::Airgapped => "airgapped",
            NetworkMode::Ask => "ask",
            NetworkMode::Online => "online",
        }
    }

    /// Order used to clamp a user choice under the admin ceiling.
    pub fn rank(self) -> u8 {
        match self {
            NetworkMode::Airgapped => 0,
            NetworkMode::Ask => 1,
            NetworkMode::Online => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct EnvironmentState {
    pub user: String,
    pub network: NetworkMode,
    /// Admin ceiling; the user may only choose a mode at or below this rank.
    pub network_ceiling: NetworkMode,
    pub model: Option<ModelInfo>,
    pub harnesses: Vec<HarnessSummary>,
    /// Harness whose surface has focus (or whose tool was called last).
    pub focus: Option<HarnessId>,
    pub pinned: Vec<HarnessId>,
    pub tier_b_permitted: bool,
    pub topology: Topology,
    /// Workspace this environment belongs to — the unit of access control,
    /// quota, retrieval scope and audit scope.
    pub workspace: String,
    /// One line describing the hardware, as `localspace doctor` reports it.
    pub machine: String,
    /// The model profile every budget in this environment comes from.
    pub profile: String,
    /// Whether a local inference engine is resident, and what it is.
    pub engine: EngineState,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct EngineState {
    pub running: bool,
    pub detail: String,
    /// The catalog id of the model the sidecar serves, when one is loaded or loading.
    pub model: Option<String>,
    /// A sidecar has been started and is not yet answering.
    pub loading: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Topology {
    Personal,
    Organisation,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct HarnessSummary {
    pub id: HarnessId,
    pub title: String,
    pub version: String,
    pub publisher: String,
    pub tier: Tier,
    pub views: Vec<ViewDesc>,
    pub tool_count: usize,
    pub front_door: Vec<String>,
    pub has_context_provider: bool,
    pub doc_kind: DocKind,
    pub capabilities: CapabilitySummary,
    pub enabled: bool,
    /// Set when the harness declared a capability that org policy refused.
    pub degraded: Option<String>,
    pub resources: ResourceSummary,
    /// True while the logic instance is resident. It is instantiated on first
    /// call and dropped after `idle_unload`; the document stays either way.
    pub loaded: bool,
    /// Interchange types this harness can import (spec §18.3), e.g. `outline.v1`.
    pub accepts: Vec<String>,
    /// Interchange types this harness can export.
    pub produces: Vec<String>,
    /// `harness`, `library`, `types`, … (spec §17.1). Only a harness has tools.
    pub kind: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Wasm,
    Native,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Crdt,
    Blob,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Widgets,
    Egui,
    /// An ES module bundle in a sandboxed iframe on its own origin (v2 §6.3).
    Web,
    Stream,
    /// Reserved for the Tier B runtime (v2 §6.3, §13 step 8): a manifest
    /// declaring it parses, and installation refuses it until then.
    Native,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Main,
    Side,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ViewDesc {
    pub id: ViewId,
    pub kind: SurfaceKind,
    pub placement: Placement,
    pub title: String,
}

/// What a harness declared it may cost (spec §1.2). Enforced by the runtime,
/// shown in the store and the details panel.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
pub struct ResourceSummary {
    /// Linear-memory limit for the logic component.
    pub logic_mb: u32,
    /// Heap limit for the surface module.
    pub surface_mb: u32,
    /// The logic instance is dropped after this long without a call.
    pub idle_unload_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct CapabilitySummary {
    pub fs: String,
    pub net: String,
    pub gpu: String,
    pub spawn: bool,
    pub clipboard: String,
    pub docs: String,
    pub model: Vec<String>,
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// One model in the catalog (architecture v2 §4.4), with the planner's verdict
/// for this machine and what is on disk.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub title: String,
    pub family: String,
    pub params_b: f32,
    /// Parameters active per token; equals `params_b` for a dense model.
    pub active_params_b: f32,
    pub quant: String,
    pub license: String,
    pub license_url: String,
    /// Approximate size on disk of all files.
    pub bytes: u64,
    pub context_len: u32,
    /// `hf:<repo>` for the Hugging Face catalog, `import` for a file the user brought.
    pub source: String,
    pub files: Vec<String>,
    pub installed: bool,
    pub loaded: bool,
    pub download: Option<DownloadState>,
    /// `resident`, `hybrid`, `streaming`, `does not fit`, or `unknown`.
    pub verdict: String,
    pub estimated_tok_s: f32,
    pub first_token_ms: f32,
    pub plan_summary: String,
    pub plan_notes: Vec<String>,
    pub supports_tools: bool,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct DownloadState {
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// `downloading`, `verifying`, `done`, or `failed: <why>`.
    pub stage: String,
}

/// One conversation in the list (v2 §8).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ModelInfo {
    pub id: String,
    pub backend: String,
    pub context_len: u32,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub loaded: bool,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Read,
    Write,
    Compute,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Confirm {
    Never,
    Destructive,
    Always,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum CostHint {
    Instant,
    Seconds,
    Long,
}

/// A tool as the model sees it, after Core has namespaced it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ExposedTool {
    pub harness: HarnessId,
    /// Fully qualified, e.g. `canvas.add_shape`.
    pub name: String,
    pub summary: String,
    pub params: Json,
    pub kind: ToolKind,
    pub confirm: Confirm,
    pub cost_hint: CostHint,
    pub undoable: bool,
    /// Why this tool is in context this turn.
    pub reason: ExposureReason,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ExposureReason {
    Focused,
    Pinned,
    Touched,
    CoreBuiltin,
}

/// What Core computed for this turn — surfaced in the trace so tool exposure is auditable.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ActiveSet {
    pub tools: Vec<ExposedTool>,
    pub token_estimate: usize,
    pub budget: usize,
    pub dropped: Vec<HarnessId>,
    pub grammar_hash: String,
}

// ---------------------------------------------------------------------------
// Conversation / agent loop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Present on assistant messages that called tools.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ToolCallRecord {
    pub id: String,
    pub tool: String,
    pub params: Json,
    pub outcome: ToolOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Ok {
        /// What the model sees — "added 3 shapes, moved 1", never the whole document.
        diff_summary: String,
        result: Json,
        commit: Option<CommitId>,
    },
    Denied {
        reason: String,
    },
    AwaitingConfirm {
        prompt: String,
    },
    Error {
        message: String,
    },
    Queued {
        job: JobId,
    },
}

// ---------------------------------------------------------------------------
// Context providers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ContextBlock {
    pub harness: HarnessId,
    /// Text serialization of harness state, sized to the budget.
    pub text: String,
    pub tokens: usize,
    /// True when the provider elided detail and a `zoom` tool can expand it.
    pub expandable: bool,
}

// ---------------------------------------------------------------------------
// The task ledger (spec §18.1) — shared context across harnesses
// ---------------------------------------------------------------------------

/// Who the caller is, as `GET /api/v1/me` answers: the account, its roles,
/// the mode and the version. Never a token, never an endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Me {
    pub user: String,
    pub email: String,
    pub name: String,
    pub roles: Vec<UserRole>,
    pub provider: String,
    pub topology: Topology,
    pub version: String,
    pub harness_api: String,
}

/// A user of an organisation server as the admin pages see them (deployment
/// §4): never the password hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub roles: Vec<UserRole>,
    /// `local` for an admin-made account; `oidc` once an identity provider is bound.
    pub provider: String,
    pub disabled: bool,
    /// Whether the user has set a password; until then only their one-time link signs them in.
    pub has_password: bool,
    pub created_ms: u64,
    pub last_login_ms: Option<u64>,
    /// Set while repeated failed logins keep the account locked.
    pub locked_until_ms: Option<u64>,
}

/// A one-time link's token, single-use, expiring in 24 hours. The shell
/// makes the link from its own origin and shows it to the admin once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Invite {
    pub user: String,
    pub email: String,
    pub token: String,
    pub expires_ms: u64,
}

/// One agent run's ledger. Rendered into every prompt regardless of which
/// harness is focused: the tools change with focus, the ledger does not.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Task {
    pub id: String,
    /// What the user asked.
    pub goal: String,
    pub plan: Vec<Step>,
    /// Typed, versioned results. A DAG reference, not a copy.
    pub artifacts: Vec<Artifact>,
    /// Agent scratch: decisions, open questions.
    pub notes: Vec<String>,
    pub citations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Step {
    pub harness: HarnessId,
    pub intent: String,
    pub status: StepStatus,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Active,
    Done,
    Failed,
}

/// A typed artifact pinned to an exact document version.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Artifact {
    /// `art_1`, `art_2`, … — what the agent passes between tools.
    pub id: String,
    /// Interchange type, e.g. `outline.v1`.
    pub kind: String,
    pub doc: DocId,
    /// The commit the artifact is pinned to; empty for a document with no commits.
    pub commit: CommitId,
    /// The producing harness's own words for it.
    pub summary: String,
    pub produced_by: HarnessId,
    /// The fields its type requires (plugin spec §18.3): for a rendering,
    /// the `document` and `commit` it was made from. An object; `{}` when
    /// the type requires none.
    #[serde(default = "Json::object")]
    pub fields: Json,
    /// Set when the artifact is a file — an export, later an upload — whose
    /// bytes `GET /api/v1/documents/{doc}/content` serves.
    #[serde(default)]
    pub file: Option<ArtifactFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct ArtifactFile {
    pub name: String,
    pub mime: String,
    pub bytes: u64,
}

/// A document as the Data page lists it: a harness's, or a file of its own
/// such as an export (deployment §5: "harness docs, uploaded files, ingested
/// sources, web cache").
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct DocumentInfo {
    pub id: DocId,
    pub title: String,
    pub kind: DocKind,
    /// The content's media type: a harness document's JSON projection, or
    /// the file's own.
    pub mime: String,
    /// The file's size; a harness document has none to give.
    pub bytes: Option<u64>,
    /// blake3 of the content at the head commit; empty before any commit.
    pub hash: String,
    pub head: Option<CommitId>,
    pub source: DocumentSource,
    /// When a file document was created; a harness document is as old as
    /// its harness's install.
    pub created_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum DocumentSource {
    /// The one document a harness edits.
    Harness { harness: HarnessId },
    /// A surface's rendering of `document` at `commit`.
    Export {
        harness: HarnessId,
        document: DocId,
        commit: CommitId,
    },
}

// ---------------------------------------------------------------------------
// Version DAG
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Commit {
    pub id: CommitId,
    pub parent: Option<CommitId>,
    pub doc: DocId,
    pub harness: HarnessId,
    pub tool: String,
    pub params: Json,
    pub doc_hash: String,
    pub diff_summary: String,
    pub author: Author,
    pub at_ms: u64,
    /// Agent runs are one branch; rejecting them is a branch drop.
    pub run: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    User,
    Agent,
    Harness,
}

// ---------------------------------------------------------------------------
// Widgets surfaces (declarative tree rendered by the Client in host theme)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Column {
        children: Vec<Widget>,
    },
    Row {
        children: Vec<Widget>,
    },
    Text {
        text: String,
        #[serde(default)]
        strong: bool,
        #[serde(default)]
        muted: bool,
    },
    Heading {
        text: String,
    },
    Separator,
    Space {
        size: f32,
    },
    Button {
        id: String,
        label: String,
        #[serde(default)]
        enabled: bool,
    },
    Input {
        id: String,
        label: String,
        value: String,
        #[serde(default)]
        multiline: bool,
    },
    Checkbox {
        id: String,
        label: String,
        value: bool,
    },
    Select {
        id: String,
        label: String,
        value: String,
        options: Vec<String>,
    },
    Slider {
        id: String,
        label: String,
        value: f64,
        min: f64,
        max: f64,
    },
    List {
        items: Vec<String>,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Badge {
        text: String,
        tone: Tone,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Neutral,
    Good,
    Warn,
    Bad,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct WidgetEvent {
    pub id: String,
    pub value: WidgetValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum WidgetValue {
    Clicked,
    Text(String),
    Bool(bool),
    Number(f64),
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    // --- environment ---
    GetEnvironment,
    SetNetworkMode {
        mode: NetworkMode,
    },
    SetFocus {
        harness: Option<HarnessId>,
    },
    SetPinned {
        harness: HarnessId,
        pinned: bool,
    },
    SetHarnessEnabled {
        harness: HarnessId,
        enabled: bool,
    },

    // --- install / packaging ---
    InstallHarness {
        path: String,
    },
    /// Second leg of an install that widened capabilities: the user saw the diff.
    ApproveInstall {
        harness: HarnessId,
        token: String,
    },
    UninstallHarness {
        harness: HarnessId,
    },

    // --- conversation ---
    SendMessage {
        text: String,
    },
    CancelTurn,
    GetTranscript,
    /// Answer an inline approval (tool confirm, egress domain, capability grant).
    Approve {
        id: String,
        granted: bool,
    },

    // --- direct tool invocation (the Client's own affordances use the same path) ---
    CallTool {
        tool: String,
        params: Json,
    },

    // --- surfaces ---
    /// Bytes of a `kind = "egui"` surface module, for the Client's SurfaceRunner.
    GetSurfaceModule {
        harness: HarnessId,
        view: ViewId,
    },
    /// One file of a `web` view, relative to its entry module's directory.
    /// The server hands these out on the harness's own origin.
    GetSurfaceFile {
        harness: HarnessId,
        view: ViewId,
        path: String,
    },
    /// Declarative tree of a `kind = "widgets"` view.
    GetWidgetView {
        harness: HarnessId,
        view: ViewId,
    },
    WidgetEvent {
        harness: HarnessId,
        view: ViewId,
        event: WidgetEvent,
    },
    /// Opaque command from a surface to its own logic (<= 64 KB).
    HarnessEvent {
        harness: HarnessId,
        view: ViewId,
        payload: Vec<u8>,
    },

    // --- documents ---
    OpenDoc {
        harness: HarnessId,
    },
    /// The JSON projection of a harness document — what an `egui` surface reads.
    /// The Automerge snapshot in `OpenDoc` is for a replica; this is for a surface.
    GetDocJson {
        harness: HarnessId,
    },
    /// A file a surface rendered from its harness's document — a PNG or an
    /// SVG of a board — kept as a document of its own and registered as a
    /// typed artifact pinned to it (plugin spec §18.3). `fields` carries what
    /// the kind requires, the source `document` and `commit` for a rendering;
    /// `mime` must be the kind's. The bytes arrive through
    /// `POST /api/v1/artifacts`, whose body they are.
    ProduceArtifact {
        harness: HarnessId,
        view: ViewId,
        kind: String,
        name: String,
        mime: String,
        bytes: Vec<u8>,
        fields: Json,
        summary: String,
    },
    /// Every document in the workspace the caller may see.
    ListDocuments,
    /// A file document's bytes at its head, for
    /// `GET /api/v1/documents/{id}/content`.
    GetDocBlob {
        doc: DocId,
    },
    /// A web surface writing its harness's document back as JSON (v2 §6.3).
    /// Core reconciles it field by field against the Automerge document and
    /// commits the difference as the user's edit under `surface:<view>`.
    WriteDoc {
        harness: HarnessId,
        view: ViewId,
        doc: Json,
        /// Whether the write is a commit in the history. A surface sends
        /// `false` for state that is the user's but not an edit, such as the
        /// selection: the document moves and every client sees it, and undo
        /// steps over it.
        #[serde(default = "default_true")]
        commit: bool,
    },
    /// An Automerge sync message from one surface replica. `peer` names the
    /// replica, one per surface connection, chosen by the shell: two frames
    /// on one document keep separate sync states in Core (v2.1 §6.1).
    DocSync {
        doc: DocId,
        peer: String,
        message: Vec<u8>,
    },
    /// The replica `peer` is gone (its frame closed or reloaded): Core drops
    /// its sync state for `doc`.
    DocSyncEnd {
        doc: DocId,
        peer: String,
    },

    // --- identity (deployment §4; Pilot 1, Phase A) ---
    /// Administrators only: the accounts of this server.
    ListUsers,
    CreateUser {
        email: String,
        name: String,
        roles: Vec<UserRole>,
    },
    /// A role change ends the user's live sessions.
    SetUserRoles {
        user: String,
        roles: Vec<UserRole>,
    },
    /// Disabling ends the user's live sessions.
    DisableUser {
        user: String,
        disabled: bool,
    },
    /// Forget the password, end every session, and give a new one-time link.
    ResetPassword {
        user: String,
    },
    /// Clear the lock repeated failed logins earned.
    UnlockUser {
        user: String,
    },
    RevokeSessions {
        user: String,
    },
    /// The server's own: the first administrator of a server with no
    /// accounts, from `localspace admin bootstrap`.
    Bootstrap {
        email: String,
        name: String,
    },
    /// The server's own requests, never a client's: signing in and out.
    Login {
        email: String,
        password: String,
        ip: String,
        user_agent: String,
    },
    Logout {
        session: String,
    },
    /// Spend a one-time link on a password, and sign in.
    SetPassword {
        token: String,
        password: String,
        ip: String,
        user_agent: String,
    },
    InviteStatus {
        token: String,
    },

    // --- DAG ---
    GetHistory {
        limit: usize,
    },
    Undo,
    Redo,
    DropRun {
        run: String,
    },

    /// Everything the catalog offers: the org's approved set connected, or the
    /// contents of an offline bundle. Air-gapped import is a first-class path.
    ListCatalog,

    // --- models ---
    ListModels,
    SelectModel {
        id: String,
    },

    // --- introspection / dev tools ---
    /// What the model will actually see this turn.
    PreviewContext {
        budget: usize,
    },
    GetActiveSet,
    /// The current agent run's ledger: goal, plan, artifacts, notes.
    GetTask,
    /// `environment.lock` (spec §17.2): every installed package, exact version,
    /// content hash, source and interface bindings. A document in the DAG.
    GetLock,
    FindCapability {
        need: String,
    },
    RunEvals {
        harness: HarnessId,
    },
    /// The model catalog with the planner's verdict per entry (v2 §4.4).
    ListModelCatalog,
    /// Fetch a catalog model's files from Hugging Face: provisioning egress.
    DownloadModel {
        id: String,
    },
    /// Start the inference sidecar on a downloaded or imported model.
    LoadModel {
        id: String,
    },
    UnloadModel,
    /// Bring a GGUF file the user already has into the catalog, in place.
    ImportModel {
        path: String,
    },
    /// The last lines of the sidecar's log.
    EngineLog {
        lines: usize,
    },
    /// The chat harness keeps several conversations (v2 §8); the transcript is the current one.
    ListConversations,
    NewConversation,
    SelectConversation {
        id: String,
    },
    DeleteConversation {
        id: String,
    },
    RenameConversation {
        id: String,
        title: String,
    },
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Ok,
    Environment(EnvironmentState),
    Transcript {
        messages: Vec<ChatMessage>,
    },
    ToolResult(ToolOutcome),
    SurfaceModule {
        bytes: Vec<u8>,
        shape_schema: u32,
        /// The heap limit the Client must enforce on this surface.
        memory_mb: u32,
    },
    WidgetView {
        root: Widget,
    },
    SurfaceFile {
        bytes: Vec<u8>,
        mime: String,
    },
    DocOpened {
        doc: DocId,
        snapshot: Vec<u8>,
        kind: DocKind,
    },
    DocJson {
        harness: HarnessId,
        doc: DocId,
        json: Json,
    },
    /// What `ProduceArtifact` registered.
    Artifact(Artifact),
    Documents {
        documents: Vec<DocumentInfo>,
    },
    DocBlob {
        name: String,
        mime: String,
        bytes: Vec<u8>,
    },
    History {
        commits: Vec<Commit>,
    },
    Models {
        models: Vec<ModelInfo>,
    },
    Context {
        blocks: Vec<ContextBlock>,
        prompt_preview: String,
    },
    Active(ActiveSet),
    Task(Task),
    /// `environment.lock` as JSON: `{"packages": [{id, version, kind, hash, source, interfaces}]}`.
    Lock {
        json: Json,
    },
    Capabilities {
        hits: Vec<CapabilityHit>,
    },
    Catalog {
        entries: Vec<CatalogEntry>,
    },
    Evals(EvalReport),
    InstallPrompt {
        harness: HarnessId,
        token: String,
        diff: Vec<String>,
        native_reason: Option<String>,
    },
    ModelCatalog {
        entries: Vec<ModelCatalogEntry>,
    },
    EngineLog {
        lines: Vec<String>,
    },
    Conversations {
        list: Vec<ConversationSummary>,
        current: String,
    },
    Users(Vec<UserInfo>),
    Invite(Invite),
    SignedIn {
        /// The session id, for the cookie. Only its hash is stored.
        session: String,
        expires_ms: u64,
        user: UserInfo,
    },
    InviteStatus {
        valid: bool,
        email: Option<String>,
        name: Option<String>,
    },
    Error {
        message: String,
    },
}

/// One package on the store page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct CatalogEntry {
    pub id: HarnessId,
    pub title: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub tier: Tier,
    /// Shown verbatim, as the spec requires, whenever `tier = native`.
    pub native_reason: Option<String>,
    pub tool_count: usize,
    pub front_door: Vec<String>,
    pub capabilities: CapabilitySummary,
    /// Capabilities in plain language, for a reader who is not an engineer.
    pub capability_lines: Vec<String>,
    pub doc_kind: DocKind,
    pub has_context_provider: bool,
    pub eval_cases: usize,
    /// Where this package came from: a bundle directory, or a registry.
    pub source: String,
    pub path: String,
    pub installed: bool,
    pub installed_version: Option<String>,
    /// Non-empty when installing this would widen what the harness may do.
    /// An update that widens capabilities does not auto-install.
    pub widens: Vec<String>,
    /// Set when the package cannot be installed here, and why.
    pub blocked: Option<String>,
    pub resources: ResourceSummary,
    /// `harness`, `library`, `types`, `template`, `model-pack`, `skill`, `theme`.
    pub kind: String,
    /// Dependencies as declared, e.g. `io.localspace.types.geometry ^1.2`.
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct CapabilityHit {
    pub harness: HarnessId,
    pub tool: String,
    pub summary: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct EvalReport {
    pub harness: HarnessId,
    pub model: String,
    pub passed: usize,
    pub total: usize,
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct EvalCase {
    pub name: String,
    pub prompt: String,
    pub passed: bool,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Events (Core -> Client)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// The environment as the user it is sent to sees it.
    EnvironmentChanged(EnvironmentState),
    /// Something shared changed — a harness installed, the network mode,
    /// the model — so every user's view of the environment is stale; each
    /// asks for its own again.
    EnvironmentOutdated,
    /// Streamed assistant text.
    AssistantDelta {
        text: String,
    },
    AssistantDone,
    ToolCallStarted {
        id: String,
        tool: String,
        params: Json,
    },
    ToolCallFinished {
        id: String,
        tool: String,
        outcome: ToolOutcome,
    },
    /// An Automerge sync message for one surface replica, `peer`.
    DocPatch {
        doc: DocId,
        peer: String,
        message: Vec<u8>,
    },
    /// A document changed in Core: the cue for whatever reads its JSON
    /// projection (a surface that takes JSON, the egui client) to read it
    /// again. Replicas get their changes in `DocPatch` instead.
    DocChanged {
        doc: DocId,
    },
    /// Opaque message from harness logic to its surface.
    HarnessMessage {
        harness: HarnessId,
        view: ViewId,
        payload: Vec<u8>,
    },
    WidgetViewChanged {
        harness: HarnessId,
        view: ViewId,
        root: Widget,
    },
    /// An inline approval the user must answer.
    ApprovalRequest {
        id: String,
        kind: ApprovalKind,
        prompt: String,
    },
    /// Surface exceeded its frame budget.
    SurfaceSlow {
        harness: HarnessId,
        view: ViewId,
        ms: f32,
    },
    Notice {
        level: NoticeLevel,
        text: String,
    },
    TraceLine {
        text: String,
    },
    /// The ledger changed: a plan was written, an artifact produced, a note added.
    TaskChanged(Task),
    /// A model download moved, finished or failed.
    ModelProgress {
        id: String,
        done_bytes: u64,
        total_bytes: u64,
        stage: String,
    },
    /// The inference sidecar changed state: loading, ready, crashed, stopped.
    EngineChanged(EngineState),
    /// Another client, or this one, switched, created or deleted a conversation.
    ConversationChanged {
        current: String,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    ToolConfirm,
    Egress,
    Capability,
    NativeTier,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub struct Envelope {
    pub id: u64,
    pub body: Body,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
pub enum Body {
    Request(Request),
    Response(Response),
    Event(Event),
}

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("postcard: {0}")]
    Postcard(#[from] postcard::Error),
}

pub fn encode(env: &Envelope) -> Result<Vec<u8>, WireError> {
    Ok(postcard::to_allocvec(env)?)
}

pub fn decode(bytes: &[u8]) -> Result<Envelope, WireError> {
    Ok(postcard::from_bytes(bytes)?)
}

/// Rough token estimate used for every budget in the system.
/// Deliberately one function so the installer lint and the runtime agree.
pub fn estimate_tokens(s: &str) -> usize {
    // ~4 chars per token for English + JSON, floor of 1 for non-empty input.
    if s.is_empty() { 0 } else { s.len().div_ceil(4) }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrips_through_postcard() {
        let env = Envelope {
            id: 7,
            body: Body::Request(Request::SendMessage {
                text: "hello".into(),
            }),
        };
        let bytes = encode(&env).unwrap();
        let back = decode(&bytes).unwrap();
        match back.body {
            Body::Request(Request::SendMessage { text }) => assert_eq!(text, "hello"),
            other => panic!("wrong body: {other:?}"),
        }
        assert_eq!(back.id, 7);
    }

    #[test]
    fn json_payloads_survive_postcard() {
        // The reason `Json` exists: postcard has no `deserialize_any`, so a raw
        // serde_json::Value here would fail at runtime, not at compile time.
        let env = Envelope {
            id: 1,
            body: Body::Request(Request::CallTool {
                tool: "canvas.add_shape".into(),
                params: Json(serde_json::json!({"kind": "rect", "w": 120.0})),
            }),
        };
        let back = decode(&encode(&env).unwrap()).unwrap();
        match back.body {
            Body::Request(Request::CallTool { params, .. }) => {
                assert_eq!(params["kind"], "rect");
                assert_eq!(params["w"], 120.0);
            }
            other => panic!("wrong body: {other:?}"),
        }
    }

    #[test]
    fn events_survive_postcard() {
        let env = Envelope {
            id: 2,
            body: Body::Event(Event::ToolCallFinished {
                id: "t1".into(),
                tool: "canvas.add_shape".into(),
                outcome: ToolOutcome::Ok {
                    diff_summary: "added 1 shape".into(),
                    result: Json(serde_json::json!({"id": "s7"})),
                    commit: Some("c1".into()),
                },
            }),
        };
        let back = decode(&encode(&env).unwrap()).unwrap();
        match back.body {
            Body::Event(Event::ToolCallFinished { outcome, .. }) => match outcome {
                ToolOutcome::Ok { diff_summary, .. } => assert_eq!(diff_summary, "added 1 shape"),
                other => panic!("wrong outcome: {other:?}"),
            },
            other => panic!("wrong body: {other:?}"),
        }
    }

    #[test]
    fn network_mode_ranks_are_ordered() {
        assert!(NetworkMode::Airgapped.rank() < NetworkMode::Ask.rank());
        assert!(NetworkMode::Ask.rank() < NetworkMode::Online.rank());
    }
}
