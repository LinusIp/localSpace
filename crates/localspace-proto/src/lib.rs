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
pub const SHAPE_SCHEMA: u32 = 1;

/// A JSON value that can cross a non-self-describing wire.
///
/// `postcard` cannot deserialize `serde_json::Value` (it has no `deserialize_any`),
/// so every JSON payload in `proto` travels as its compact string form and is
/// re-parsed on arrival. Same type on both transports; no desktop-only shortcut.
#[derive(Debug, Clone, PartialEq)]
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
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        serde_json::from_str(&raw)
            .map(Json)
            .map_err(serde::de::Error::custom)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub running: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Topology {
    Personal,
    Organisation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Wasm,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Crdt,
    Blob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Widgets,
    Egui,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Main,
    Side,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewDesc {
    pub id: ViewId,
    pub kind: SurfaceKind,
    pub placement: Placement,
    pub title: String,
}

/// What a harness declared it may cost (spec §1.2). Enforced by the runtime,
/// shown in the store and the details panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSummary {
    /// Linear-memory limit for the logic component.
    pub logic_mb: u32,
    /// Heap limit for the surface module.
    pub surface_mb: u32,
    /// The logic instance is dropped after this long without a call.
    pub idle_unload_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Read,
    Write,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confirm {
    Never,
    Destructive,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostHint {
    Instant,
    Seconds,
    Long,
}

/// A tool as the model sees it, after Core has namespaced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureReason {
    Focused,
    Pinned,
    Touched,
    CoreBuiltin,
}

/// What Core computed for this turn — surfaced in the trace so tool exposure is auditable.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// Present on assistant messages that called tools.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub tool: String,
    pub params: Json,
    pub outcome: ToolOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    pub harness: HarnessId,
    /// Text serialization of harness state, sized to the budget.
    pub text: String,
    pub tokens: usize,
    /// True when the provider elided detail and a `zoom` tool can expand it.
    pub expandable: bool,
}

// ---------------------------------------------------------------------------
// Version DAG
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    User,
    Agent,
    Harness,
}

// ---------------------------------------------------------------------------
// Widgets surfaces (declarative tree rendered by the Client in host theme)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Neutral,
    Good,
    Warn,
    Bad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetEvent {
    pub id: String,
    pub value: WidgetValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Automerge sync message from the surface replica.
    DocSync {
        doc: DocId,
        message: Vec<u8>,
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
    FindCapability {
        need: String,
    },
    RunEvals {
        harness: HarnessId,
    },
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    Error {
        message: String,
    },
}

/// One package on the store page.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityHit {
    pub harness: HarnessId,
    pub tool: String,
    pub summary: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub harness: HarnessId,
    pub model: String,
    pub passed: usize,
    pub total: usize,
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub prompt: String,
    pub passed: bool,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Events (Core -> Client)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    EnvironmentChanged(EnvironmentState),
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
    /// Automerge sync message for a surface replica.
    DocPatch {
        doc: DocId,
        message: Vec<u8>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    ToolConfirm,
    Egress,
    Capability,
    NativeTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    pub body: Body,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    if s.is_empty() {
        0
    } else {
        s.len().div_ceil(4)
    }
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
