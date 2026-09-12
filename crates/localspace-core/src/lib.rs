#![deny(unsafe_code)]
//! localSpace Core — the backend. No UI dependencies.
//!
//! One rule governs everything here: the agent never talks to a harness directly.
//! Every call goes through `Core::call_tool`, which enforces permissions, records
//! the mutation in the version DAG, and returns a short result to the model.

pub mod acl;
pub mod agent;
pub mod audit;
pub mod catalog;
pub mod context;
pub mod conversations;
pub mod dag;
pub mod deps;
pub mod docs;
pub mod engine;
pub mod evals;
pub mod exposure;
// The one module that speaks to the operating system's process accounting
// directly (`CLAUDE.md`: unsafe only in isolated, documented modules).
#[allow(unsafe_code)]
pub mod footprint;
pub mod gateway;
pub mod grammar;
pub mod identity;
pub mod lock;
pub mod manifest;
pub mod model;
pub mod models;
pub mod planner;
pub mod profile;
pub mod prompt;
pub mod registry;
pub mod runtime;
pub mod store;
pub mod task;
pub mod tools;
pub mod transport;
pub mod types;
pub mod widgets;

use acl::{AccessControl, Acl, BreakGlass, Identity, Level, Workspace};
use anyhow::Result;
use audit::{Actor, AuditLog, Scope};
use context::ProviderCache;
use dag::Dag;
use docs::DocStore;
use gateway::{Egress, Gateway, GatewayConfig};
use grammar::GrammarCache;
use localspace_proto as proto;
use localspace_proto::Json;
use model::Router;
use profile::{Machine, ModelProfile};
use registry::{Policy, Registry};
use runtime::CoreServices;
use serde_json::Value as J;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

pub const CORE_TOOLS: &[&str] = &[
    "find_capability",
    "web.search",
    "web.fetch",
    "task.plan",
    "task.note",
];

/// How many tool calls one agent turn may make before Core stops it.
pub const MAX_AGENT_STEPS: usize = 24;

/// The largest file a surface may hand Core as an artifact: the spec's default
/// for `max_upload_mb` (deployment §3.3), a constant until `localspace.toml`
/// carries the key.
pub const MAX_ARTIFACT_BYTES: usize = 200 << 20;

#[derive(Debug, Clone)]
pub struct Config {
    pub user: String,
    pub data_dir: Option<PathBuf>,
    pub harness_dir: Option<PathBuf>,
    /// Directories the marketplace lists: a synced registry, or an offline bundle.
    /// The installed set is scanned too, so the catalog can mark what is already here.
    pub catalog_dirs: Vec<PathBuf>,
    pub topology: proto::Topology,
    pub policy: Policy,
    pub gateway: GatewayConfig,
    pub machine: Machine,
    pub profile: ModelProfile,
    /// A directory with an organisation's own `catalog.json` of models, on top
    /// of the built-in catalog. Downloaded files always go under the data dir.
    pub models_dir: Option<PathBuf>,
    /// `llama-server`, when it is not under `<data>/engines` or on PATH.
    pub llama_server: Option<PathBuf>,
    /// How long a signed-in session lives, hard (deployment §3.3
    /// `session_ttl`, 12 hours by default).
    pub session_ttl_ms: u64,
}

impl Config {
    pub fn personal(user: &str) -> Config {
        let machine = Machine::detect();
        let profile = ModelProfile::for_tier(machine.tier());
        Config {
            user: user.to_string(),
            data_dir: None,
            harness_dir: None,
            catalog_dirs: Vec::new(),
            topology: proto::Topology::Personal,
            policy: Policy::default(),
            gateway: GatewayConfig::default(),
            machine,
            profile,
            models_dir: None,
            llama_server: None,
            session_ttl_ms: 12 * 60 * 60 * 1000,
        }
    }

    pub fn organisation(user: &str) -> Config {
        let mut cfg = Config::personal(user);
        cfg.topology = proto::Topology::Organisation;
        cfg.policy = Policy::organisation_default();
        cfg
    }
}

/// A tool call held at a confirmation gate, waiting for the user.
struct Pending {
    tool: String,
    params: J,
    /// Continue the agent turn after the user answers.
    resume_agent: bool,
}

/// Who a request is made as: the server's session, or the local user of a
/// personal Core. Core checks every request against it and routes every
/// event of the handling to it.
#[derive(Debug, Clone)]
pub struct Caller {
    pub user: String,
    pub session: String,
    pub ip: String,
    pub roles: Vec<proto::UserRole>,
    pub groups: Vec<String>,
}

impl Caller {
    /// The one user of a personal Core: the admin of their own machine.
    pub fn local(user: &str) -> Caller {
        Caller {
            user: user.to_string(),
            session: "local".into(),
            ip: String::new(),
            roles: vec![proto::UserRole::Admin],
            groups: Vec::new(),
        }
    }

    /// The server itself, signing users in and out: no state of its own,
    /// no role, and the only caller the identity requests answer.
    pub fn system() -> Caller {
        Caller {
            user: "system".into(),
            session: "system".into(),
            ip: String::new(),
            roles: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn is_system(&self) -> bool {
        self.user == "system" && self.session == "system"
    }

    pub fn is_admin(&self) -> bool {
        self.roles.contains(&proto::UserRole::Admin)
    }

    fn role_label(&self) -> &'static str {
        self.roles.first().map(|r| r.label()).unwrap_or("member")
    }
}

/// Whom an event is for: one user's clients, or everyone's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    User(String),
    All,
}

/// Where Core's events go; the transport delivers each to its user.
pub type Sink = Arc<dyn Fn(To, proto::Event) + Send + Sync>;

/// What is one user's and not another's. Core carries the active caller's
/// in its own fields and parks everyone else's here; `activate` swaps.
struct UserState {
    workspace: String,
    break_glass: Option<BreakGlass>,
    focus: Option<String>,
    pinned: Vec<String>,
    touched: Vec<String>,
    transcript: Vec<proto::ChatMessage>,
    pending: HashMap<String, Pending>,
    next_approval: u64,
    pending_installs: HashMap<String, PathBuf>,
    proposals: Vec<Proposal>,
    run: Option<String>,
    task: proto::Task,
    conversations: conversations::Store,
}

/// An agent run that wrote to a shared document and is awaiting apply/discard.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub run: String,
    pub docs: Vec<String>,
    pub summary: String,
    pub by: String,
}

/// How many replicas of one document Core keeps a sync state for. A frame
/// that closes says so; one that vanishes without a word is dropped once
/// newer ones need the room. A live replica answers every change it is sent,
/// so the one heard from longest ago is the one that has gone.
const MAX_REPLICAS_PER_DOC: usize = 32;

/// One replica's sync state, when Core last heard from it, and whose frame
/// it is, so its patches go to that user alone.
#[derive(Default)]
struct ReplicaSync {
    state: automerge::sync::State,
    seen: u64,
    user: String,
}

pub struct Core {
    cfg: Config,
    registry: Registry,
    docs: DocStore,
    /// The database: the DAG's tables and every other table share it.
    store: store::Store,
    /// Users, sessions, one-time links and lockouts, over the same database.
    directory: identity::Directory,
    dag: Dag,
    /// Shared with harness logic through `CoreServices`; never locks `Core`.
    gateway: Arc<Mutex<Gateway>>,
    router: Arc<RwLock<Router>>,
    audit: AuditLog,
    access: AccessControl,
    grammars: GrammarCache,
    providers: ProviderCache,

    focus: Option<String>,
    pinned: Vec<String>,
    touched: Vec<String>,
    transcript: Vec<proto::ChatMessage>,
    pending: HashMap<String, Pending>,
    /// Installs held at a capability-diff prompt, keyed by the token shown.
    pending_installs: HashMap<String, PathBuf>,
    proposals: Vec<Proposal>,
    run: Option<String>,
    /// The current run's ledger (spec §18.1). Artifacts carry across runs.
    task: proto::Task,
    workspace: String,
    /// Set while an administrator is in a workspace they are not a member
    /// of: the reason they gave (deployment §6.1). Ends when they leave it.
    break_glass: Option<BreakGlass>,
    /// Sync state per document, per replica (v2.1 §6.1): two frames on one
    /// board sync independently.
    sync_states: HashMap<String, HashMap<String, ReplicaSync>>,
    /// Bumped on every sync message; a replica's `seen` is its value then.
    sync_clock: u64,

    events: Option<Sink>,
    /// Whose request is being handled: the user the per-user fields belong to.
    active: Caller,
    /// Every other user's state, parked (`activate`).
    users: HashMap<String, UserState>,
    /// Every conversation of the active user in their workspace; `transcript`
    /// is the current one's messages.
    conversations: conversations::Store,
    /// While evals run, their turns are not recorded as the user's.
    evals_running: bool,
    /// The model catalog, downloads in flight, and the sidecar, if any.
    models: models::Catalog,
    downloads: Arc<Mutex<HashMap<String, proto::DownloadState>>>,
    engine: Option<engine::Engine>,
    next_approval: u64,
}

impl Core {
    /// A Core with no on-disk state. Used by tests, evals and `--ephemeral`.
    pub fn ephemeral(user: &str) -> Result<Core> {
        Core::new(Config::personal(user))
    }

    pub fn new(cfg: Config) -> Result<Core> {
        let store = match &cfg.data_dir {
            Some(dir) => store::Store::open(dir)?,
            None => store::Store::in_memory()?,
        };
        let dag = Dag::with(store.db())?;
        let directory = identity::Directory::new(store.clone(), cfg.session_ttl_ms);
        let audit = match &cfg.data_dir {
            Some(dir) => AuditLog::open(&dir.join("audit").join("audit.jsonl"))?,
            None => AuditLog::in_memory(),
        };

        let gateway = Arc::new(Mutex::new(Gateway::new(cfg.gateway.clone())));
        let router = Arc::new(RwLock::new(Router::default()));

        // Every workspace the database knows, then the local user's personal
        // one if it is not among them yet.
        let mut access = AccessControl::new();
        for ws in store.workspaces().unwrap_or_default() {
            access.add_workspace(ws);
        }
        let workspace = format!("ws_{}", cfg.user);
        if access.workspace(&workspace).is_none() {
            let personal = Workspace::personal(&cfg.user);
            let _ = store.put_workspace(&personal);
            access.add_workspace(personal);
        }

        let models_store = cfg
            .data_dir
            .as_ref()
            .map(|d| d.join("models"))
            .unwrap_or_else(|| std::env::temp_dir().join("localspace").join("models"));
        let models = models::Catalog::load(cfg.models_dir.as_deref(), &models_store);

        let conversations =
            conversations::Store::load(&store, &cfg.user, &workspace, dag::now_ms());
        let transcript = conversations
            .current()
            .map(|c| c.messages.clone())
            .unwrap_or_default();
        // The ledger outlives the process (plugin spec §18.1: artifacts
        // carry across runs), per user and workspace.
        let task = store
            .ledger(&cfg.user, &workspace)
            .unwrap_or_default()
            .unwrap_or_default();

        let mut core = Core {
            conversations,
            evals_running: false,
            models,
            downloads: Arc::new(Mutex::new(HashMap::new())),
            engine: None,
            registry: Registry::new(),
            docs: DocStore::new(),
            store,
            directory,
            dag,
            gateway,
            router,
            audit,
            access,
            grammars: GrammarCache::new(),
            providers: ProviderCache::new(),
            focus: None,
            pinned: Vec::new(),
            touched: Vec::new(),
            transcript,
            pending: HashMap::new(),
            pending_installs: HashMap::new(),
            proposals: Vec::new(),
            run: None,
            task,
            workspace,
            break_glass: None,
            sync_states: HashMap::new(),
            sync_clock: 0,
            events: None,
            next_approval: 1,
            active: Caller::local(&cfg.user),
            users: HashMap::new(),
            cfg,
        };

        let mut loaded = false;
        if let Some(dir) = core.cfg.harness_dir.clone() {
            core.load_harnesses(&dir);
            loaded = true;
        }
        // What the user installed from a catalog, kept under the data
        // directory (v2 §1: only chat ships in the box; the rest is installed).
        if let Some(root) = core.installed_root()
            && root.is_dir()
        {
            core.load_harnesses(&root);
            loaded = true;
        }
        // An older database is brought up to date before any document is
        // assigned, so a board keeps the history it has under its old name.
        core.migrate_if_needed();
        if loaded {
            core.settle_packages();
        }
        core.load_documents();
        Ok(core)
    }

    pub fn set_event_sink(&mut self, sink: Box<dyn Fn(To, proto::Event) + Send + Sync>) {
        self.events = Some(Arc::from(sink));
    }

    /// The sink for Core's own threads — downloads, the engine supervisor —
    /// whose news is everyone's: the model and the engine are shared.
    fn sink(&self) -> engine::EventSink {
        let events = self.events.clone();
        Arc::new(move |ev| {
            if let Some(sink) = &events {
                sink(To::All, ev);
            }
        })
    }

    fn emit_to(&self, to: To, ev: proto::Event) {
        if let Some(sink) = &self.events {
            sink(to, ev);
        }
    }

    /// To the user whose request this is.
    fn emit(&self, ev: proto::Event) {
        self.emit_to(To::User(self.active.user.clone()), ev);
    }

    /// To everyone: a document changed, something shared moved.
    fn emit_all(&self, ev: proto::Event) {
        self.emit_to(To::All, ev);
    }

    fn trace(&self, line: impl Into<String>) {
        self.emit(proto::Event::TraceLine { text: line.into() });
    }

    fn notice(&self, level: proto::NoticeLevel, text: impl Into<String>) {
        self.emit(proto::Event::Notice {
            level,
            text: text.into(),
        });
    }

    // -- services handed to harness logic -----------------------------------

    pub fn services(&self) -> Arc<dyn CoreServices> {
        Arc::new(Services {
            router: self.router.clone(),
            gateway: self.gateway.clone(),
        })
    }

    pub fn router(&self) -> Arc<RwLock<Router>> {
        self.router.clone()
    }

    pub fn machine(&self) -> &Machine {
        &self.cfg.machine
    }

    pub fn profile(&self) -> &ModelProfile {
        &self.cfg.profile
    }

    pub fn audit_log(&self) -> &AuditLog {
        &self.audit
    }

    /// The identity store, for the server to read sessions on every request
    /// without a trip through this thread. Writes go through requests.
    pub fn directory(&self) -> identity::Directory {
        self.directory.clone()
    }

    pub fn provider_cache_hit_rate(&self) -> f32 {
        self.providers.hit_rate()
    }

    fn identity(&self) -> Identity {
        Identity {
            user: self.active.user.clone(),
            groups: self.active.groups.clone(),
            break_glass: self.break_glass.clone(),
        }
    }

    fn actor(&self) -> Actor {
        Actor {
            user: self.active.user.clone(),
            session: self.active.session.clone(),
            ip: self.active.ip.clone(),
            role: self.active.role_label().into(),
            break_glass: self.break_glass.as_ref().map(|g| g.reason.clone()),
        }
    }

    fn scope(&self, document: &str) -> Scope {
        Scope {
            workspace: self.workspace.clone(),
            conversation: self.conversations.current.clone(),
            document: document.to_string(),
        }
    }

    /// Make `caller` the user the per-user fields belong to: the previous
    /// user's are parked, theirs are brought in or made. Nothing moves when
    /// the same user calls again.
    fn activate(&mut self, caller: &Caller) {
        if self.active.user == caller.user {
            self.active = caller.clone();
            return;
        }
        let incoming = match self.users.remove(&caller.user) {
            Some(state) => state,
            None => self.fresh_user_state(&caller.user),
        };
        let parked = UserState {
            workspace: std::mem::replace(&mut self.workspace, incoming.workspace),
            break_glass: std::mem::replace(&mut self.break_glass, incoming.break_glass),
            focus: std::mem::replace(&mut self.focus, incoming.focus),
            pinned: std::mem::replace(&mut self.pinned, incoming.pinned),
            touched: std::mem::replace(&mut self.touched, incoming.touched),
            transcript: std::mem::replace(&mut self.transcript, incoming.transcript),
            pending: std::mem::replace(&mut self.pending, incoming.pending),
            next_approval: std::mem::replace(&mut self.next_approval, incoming.next_approval),
            pending_installs: std::mem::replace(
                &mut self.pending_installs,
                incoming.pending_installs,
            ),
            proposals: std::mem::replace(&mut self.proposals, incoming.proposals),
            run: std::mem::replace(&mut self.run, incoming.run),
            task: std::mem::replace(&mut self.task, incoming.task),
            conversations: std::mem::replace(&mut self.conversations, incoming.conversations),
        };
        let previous = std::mem::replace(&mut self.active, caller.clone());
        self.users.insert(previous.user, parked);
    }

    /// A user seen for the first time: their personal workspace (deployment
    /// §5), their conversations and ledger from the database, and the first
    /// harness in focus, as a fresh Core starts.
    fn fresh_user_state(&mut self, user: &str) -> UserState {
        let personal = format!("ws_{user}");
        if self.access.workspace(&personal).is_none() {
            let ws = Workspace::personal(user);
            if let Err(e) = self.store.put_workspace(&ws) {
                self.trace(format!("workspace {personal}: not written: {e:#}"));
            }
            self.access.add_workspace(ws);
        }
        // The workspace they were last in, if they may still be in it.
        let identity = Identity::user(user);
        let workspace = self
            .store
            .current_workspace(user)
            .unwrap_or_default()
            .filter(|ws| self.access.level_in(ws, &identity).is_some())
            .unwrap_or(personal);
        let conversations =
            conversations::Store::load(&self.store, user, &workspace, dag::now_ms());
        let transcript = conversations
            .current()
            .map(|c| c.messages.clone())
            .unwrap_or_default();
        let task = self
            .store
            .ledger(user, &workspace)
            .unwrap_or_default()
            .unwrap_or_default();
        UserState {
            workspace,
            break_glass: None,
            focus: self
                .registry
                .iter()
                .find(|h| h.manifest.package.kind.is_harness())
                .map(|h| h.id().to_string()),
            pinned: Vec::new(),
            touched: Vec::new(),
            transcript,
            pending: HashMap::new(),
            next_approval: 1,
            pending_installs: HashMap::new(),
            proposals: Vec::new(),
            run: None,
            task,
            conversations,
        }
    }

    /// The active workspace's document for a harness, brought in on first
    /// use: registered with the store and the ACL, and restored from its
    /// history. `None` for a package that is not an installed harness.
    fn doc_for(&mut self, harness: &str) -> Option<proto::DocId> {
        let (title, kind) = {
            let h = self.registry.get(harness)?;
            if !h.manifest.package.kind.is_harness() {
                return None;
            }
            (h.manifest.harness.title.clone(), h.doc_kind())
        };
        let doc_id = self.harness_document(harness, &title, kind);
        if !self.docs.exists(&doc_id) {
            self.docs.ensure(&doc_id, kind);
            let ws = self.workspace.clone();
            self.access.add_document(&doc_id, &ws, &title, None);
            self.restore_from_dag(&doc_id);
        }
        Some(doc_id)
    }

    // -- harnesses ----------------------------------------------------------

    /// Stage and instantiate every package under `dir`. `settle_packages`
    /// follows once every directory is in.
    pub fn load_harnesses(&mut self, dir: &std::path::Path) {
        let services = self.services();
        let failures = self
            .registry
            .load_dir(dir, &self.cfg.policy.clone(), services);
        for (name, err) in failures {
            self.notice(
                proto::NoticeLevel::Error,
                format!("harness `{name}` failed to install: {err:#}"),
            );
        }
    }

    /// After the package directories are loaded. Packages loaded from a
    /// directory arrive without their dependencies resolved, so what they
    /// need is installed from the catalog as an install would (spec §17.2);
    /// a package naming an interchange kind no installed types package
    /// declares is set aside, as an install would refuse it (§18.3); every
    /// harness gets its document; a focus is picked; the lock is written.
    pub fn settle_packages(&mut self) {
        let dependents: Vec<(String, PathBuf, manifest::Manifest)> = self
            .registry
            .iter()
            .filter(|h| !h.manifest.dependencies.is_empty())
            .map(|h| (h.id().to_string(), h.dir.clone(), h.manifest.clone()))
            .collect();
        for (id, dir, manifest) in dependents {
            if let Err(e) = self.install_dependencies(&manifest, &dir) {
                self.notice(
                    proto::NoticeLevel::Error,
                    format!("`{id}` is not available: {e:#}"),
                );
                self.registry.remove(&id);
            }
        }
        let manifests: Vec<(String, manifest::Manifest)> = self
            .registry
            .iter()
            .map(|h| (h.id().to_string(), h.manifest.clone()))
            .collect();
        for (id, manifest) in manifests {
            if let Err(e) = self.interchange_kinds_declared(&manifest) {
                self.notice(
                    proto::NoticeLevel::Error,
                    format!("`{id}` is not available: {e:#}"),
                );
                self.registry.remove(&id);
            }
        }
        // Each harness gets the document this workspace shows for it (spec
        // §10); a types or library package has nothing to edit.
        let harnesses: Vec<String> = self
            .registry
            .iter()
            .filter(|h| h.manifest.package.kind.is_harness())
            .map(|h| h.id().to_string())
            .collect();
        for id in harnesses {
            self.doc_for(&id);
        }
        if self.focus.is_none() {
            self.focus = self
                .registry
                .iter()
                .find(|h| h.manifest.package.kind.is_harness())
                .map(|h| h.id().to_string());
        }
        self.write_lock();
    }

    /// Bring a document back to the state its DAG head records.
    ///
    /// The DAG is the durable store: a document is whatever its head commit's
    /// blob says it is. Without this, `--data` would persist the history and
    /// silently lose the documents it describes.
    fn restore_from_dag(&mut self, doc_id: &str) {
        let Ok(Some(head)) = self.dag.head(doc_id) else {
            return;
        };
        let Ok(Some(commit)) = self.dag.get_commit(&head) else {
            return;
        };
        match self.dag.get_blob(&commit.doc_hash) {
            Ok(Some(bytes)) => {
                if let Err(e) = self.docs.restore(doc_id, Some(&bytes)) {
                    self.notice(
                        proto::NoticeLevel::Error,
                        format!("`{doc_id}` could not be restored from its history: {e:#}"),
                    );
                }
            }
            _ => self.notice(
                proto::NoticeLevel::Warn,
                format!("`{doc_id}` has history but its content is missing from the blob store"),
            ),
        }
    }

    pub fn install(&mut self, dir: &std::path::Path) -> Result<proto::Response> {
        self.install_inner(dir, false)
    }

    /// Every document the database records — a harness's, an export, later
    /// an upload — known to the store and the ACL. A file's bytes are not
    /// loaded; they are read from the DAG when asked for, so a workspace of
    /// exports costs Core nothing at idle.
    fn load_documents(&mut self) {
        let records = match self.store.documents() {
            Ok(records) => records,
            Err(e) => {
                self.notice(
                    proto::NoticeLevel::Warn,
                    format!("the documents table could not be read: {e:#}"),
                );
                return;
            }
        };
        for (id, record) in records {
            // A harness's document is brought in, and restored, when a
            // workspace first asks for it (`doc_for`).
            if record.harness.is_none() {
                self.docs.ensure(&id, record.kind);
            }
            let ws = if record.workspace.is_empty() {
                self.workspace.clone()
            } else {
                record.workspace.clone()
            };
            self.access
                .add_document(&id, &ws, &record.title, record.acl.clone());
        }
    }

    /// The document this workspace shows for a harness: the oldest it holds
    /// for it, made on first use. A workspace may hold several documents per
    /// harness; the UI shows one until a list view is asked for (Pilot 1,
    /// answer 12).
    fn harness_document(
        &mut self,
        harness: &str,
        title: &str,
        kind: proto::DocKind,
    ) -> proto::DocId {
        let ws = self.workspace.clone();
        let existing = self
            .store
            .documents()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, r)| r.workspace == ws && r.harness.as_deref() == Some(harness))
            .min_by(|a, b| {
                a.1.created_ms
                    .cmp(&b.1.created_ms)
                    .then_with(|| a.0.cmp(&b.0))
            })
            .map(|(id, _)| id);
        if let Some(id) = existing {
            return id;
        }
        let id = store::Store::new_document_id();
        let record = store::DocumentRecord {
            title: title.to_string(),
            kind,
            mime: match kind {
                proto::DocKind::Crdt => "application/json".into(),
                proto::DocKind::Blob => "application/octet-stream".into(),
            },
            bytes: 0,
            hash: String::new(),
            source: proto::DocumentSource::Harness {
                harness: harness.to_string(),
            },
            created_ms: dag::now_ms(),
            workspace: ws,
            harness: Some(harness.to_string()),
            created_by: self.active.user.clone(),
            acl: None,
        };
        if let Err(e) = self.store.put_document(&id, &record) {
            self.notice(
                proto::NoticeLevel::Error,
                format!("the document for `{harness}` could not be recorded: {e:#}"),
            );
        }
        id
    }

    /// Bring an older database up to this code's schema, forward only.
    ///
    /// v1 → v2 (2026-09-12): the file is `localspace.redb`, the copy having
    /// been taken by `Store::open`. Every document with history — until now
    /// known only by its harness's name with dots as underscores — gets a
    /// record in the one user's personal workspace, so it stays that
    /// harness's oldest and shown document; the exports of 6.0 get their
    /// workspace and creator.
    fn migrate_if_needed(&mut self) {
        let version = self.store.schema_version().unwrap_or(0);
        if version >= store::SCHEMA_VERSION {
            return;
        }
        if version < 2 {
            let ws = self.workspace.clone();
            let user = self.cfg.user.clone();
            let records = self.store.documents().unwrap_or_default();
            for (id, record) in &records {
                if record.workspace.is_empty() {
                    let mut filled = record.clone();
                    filled.workspace = ws.clone();
                    filled.created_by = user.clone();
                    let _ = self.store.put_document(id, &filled);
                }
            }
            let by_legacy_id: HashMap<String, (String, String, proto::DocKind)> = self
                .registry
                .iter()
                .filter(|h| h.manifest.package.kind.is_harness())
                .map(|h| {
                    (
                        h.id().replace('.', "_"),
                        (
                            h.id().to_string(),
                            h.manifest.harness.title.clone(),
                            h.doc_kind(),
                        ),
                    )
                })
                .collect();
            let history = self.dag.history(usize::MAX).unwrap_or_default();
            let mut seen: Vec<String> = Vec::new();
            for commit in history.iter().rev() {
                let doc = &commit.doc;
                if doc == lock::LOCK_DOC
                    || doc.starts_with("blob:")
                    || seen.contains(doc)
                    || records.iter().any(|(id, _)| id == doc)
                {
                    continue;
                }
                seen.push(doc.clone());
                let (harness, title, kind) = match by_legacy_id.get(doc) {
                    Some((id, title, kind)) => (Some(id.clone()), title.clone(), *kind),
                    None => (None, doc.clone(), proto::DocKind::Crdt),
                };
                let record = store::DocumentRecord {
                    title,
                    kind,
                    mime: match kind {
                        proto::DocKind::Crdt => "application/json".into(),
                        proto::DocKind::Blob => "application/octet-stream".into(),
                    },
                    bytes: 0,
                    hash: String::new(),
                    source: proto::DocumentSource::Harness {
                        harness: harness.clone().unwrap_or_else(|| doc.clone()),
                    },
                    created_ms: commit.at_ms,
                    workspace: ws.clone(),
                    harness,
                    created_by: user.clone(),
                    acl: None,
                };
                let _ = self.store.put_document(doc, &record);
            }
            self.trace(format!(
                "database: migrated to schema 2; {} document(s) recorded",
                seen.len()
            ));
        }
        if version < 3 {
            // v2 → v3: conversations move from `conversations.json` into the
            // database, the one user's, in their personal workspace; the file
            // stays, renamed, as the copy taken before migrating.
            if let Some(dir) = self.cfg.data_dir.clone() {
                let path = dir.join(conversations::LEGACY_FILE);
                if let Some((list, current)) = conversations::read_legacy_file(&path) {
                    let count = list.len();
                    self.conversations.import(list, current);
                    self.transcript = self
                        .conversations
                        .current()
                        .map(|c| c.messages.clone())
                        .unwrap_or_default();
                    let kept = dir.join(format!("{}.imported", conversations::LEGACY_FILE));
                    if let Err(e) = std::fs::rename(&path, &kept) {
                        self.notice(
                            proto::NoticeLevel::Warn,
                            format!(
                                "{} was imported but could not be renamed: {e}",
                                path.display()
                            ),
                        );
                    }
                    self.trace(format!(
                        "database: migrated to schema 3; {count} conversation(s) imported"
                    ));
                }
            }
        }
        if let Err(e) = self.store.set_schema_version(store::SCHEMA_VERSION) {
            self.notice(
                proto::NoticeLevel::Error,
                format!("the database's schema version could not be written: {e:#}"),
            );
        }
    }

    /// Where packages installed from a catalog live: the environment's own
    /// copy, so an install outlives the bundle it came from and a restart.
    fn installed_root(&self) -> Option<PathBuf> {
        self.cfg.data_dir.as_ref().map(|d| d.join("installed"))
    }

    /// Copy a package under the data directory before it is staged. Without
    /// a data directory, or for a package already there, it is used where
    /// it lies, which is what `--harnesses` and the tests rely on.
    fn persist_package(&self, dir: &std::path::Path) -> Result<PathBuf> {
        use anyhow::Context as _;
        let Some(root) = self.installed_root() else {
            return Ok(dir.to_path_buf());
        };
        let src = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let root_c = root.canonicalize().unwrap_or_else(|_| root.clone());
        if src.starts_with(&root_c) || src.starts_with(&root) {
            return Ok(dir.to_path_buf());
        }
        let manifest = manifest::Manifest::load(dir)?;
        let dest = root.join(&manifest.harness.id);
        if dest.exists() {
            std::fs::remove_dir_all(&dest)
                .with_context(|| format!("replacing the installed copy at {}", dest.display()))?;
        }
        copy_dir(&src, &dest)
            .with_context(|| format!("copying the package into {}", dest.display()))?;
        Ok(dest)
    }

    fn install_inner(
        &mut self,
        dir: &std::path::Path,
        capabilities_approved: bool,
    ) -> Result<proto::Response> {
        let policy = self.cfg.policy.clone();
        let persisted = self.persist_package(dir)?;
        let dir = persisted.as_path();
        let mut staged = Registry::stage(dir, &policy)?;

        // Dependencies first (spec §17.2); then every interchange kind the
        // package names must be one an installed types package declares, and
        // a types package may not redeclare another's kind (§18.3).
        self.install_dependencies(&staged.manifest, dir)?;
        self.types_declared_once(&staged)?;
        self.interchange_kinds_declared(&staged.manifest)?;

        // An update that widens capabilities does not auto-install: it re-prompts
        // with a diff, and only proceeds once the user has answered.
        if !capabilities_approved && let Some(existing) = self.registry.get(staged.id()) {
            let diff = staged
                .manifest
                .capabilities
                .widening_over(&existing.manifest.capabilities);
            if !diff.is_empty() {
                let token = format!("t{}", dag::now_ms());
                self.pending_installs
                    .insert(token.clone(), dir.to_path_buf());
                return Ok(proto::Response::InstallPrompt {
                    harness: staged.manifest.harness.id.clone(),
                    token,
                    diff,
                    native_reason: staged.manifest.harness.native_reason.clone(),
                });
            }
        }
        // A Tier B package must show its reason before anything runs.
        if staged.manifest.harness.tier == manifest::Tier::Native {
            let reason = staged
                .manifest
                .harness
                .native_reason
                .clone()
                .unwrap_or_default();
            self.emit(proto::Event::ApprovalRequest {
                id: format!("native:{}", staged.id()),
                kind: proto::ApprovalKind::NativeTier,
                prompt: format!(
                    "`{}` runs as a native process outside the wasm sandbox. Its stated reason: {reason}",
                    staged.manifest.harness.title
                ),
            });
        }

        Registry::instantiate(&mut staged, self.services())?;
        let id = staged.id().to_string();
        let staged_is_harness = staged.manifest.package.kind.is_harness();
        self.registry.insert(staged);

        // The workspace's document for the harness (spec §10); a types or
        // library package has nothing to edit. A package installed again
        // finds its document where its history left it: the database is the
        // durable store, not the package.
        let doc_id = if staged_is_harness {
            self.doc_for(&id).unwrap_or_default()
        } else {
            String::new()
        };

        let _ = self.audit.append(
            self.actor(),
            self.scope(&doc_id),
            "harness.install",
            serde_json::json!({"harness": id}),
            "ok",
        );
        if self.focus.is_none() && staged_is_harness {
            self.focus = Some(id);
        }
        self.write_lock();
        Ok(proto::Response::Ok)
    }

    /// Install what `manifest` depends on and is not installed yet, in
    /// dependency order, from what is installed and what the catalog offers
    /// (spec §17.2): one version per package per environment, interface
    /// dependencies bound to any provider, a conflict named with both
    /// dependents. `dir` is where the package itself lies, for the resolver.
    fn install_dependencies(
        &mut self,
        manifest: &manifest::Manifest,
        dir: &std::path::Path,
    ) -> Result<()> {
        if manifest.dependencies.is_empty() {
            return Ok(());
        }
        let mut candidates = catalog::candidates(&self.catalog_dirs_all(), &self.registry);
        if !candidates.iter().any(|c| c.id == manifest.harness.id) {
            candidates.push(deps::Candidate {
                id: manifest.harness.id.clone(),
                version: manifest.harness.version.clone(),
                kind: manifest.package.kind,
                provides: manifest.provides.interfaces.clone(),
                deps: manifest.dependencies(),
                path: dir.to_path_buf(),
                installed: false,
            });
        }
        let resolution =
            deps::resolve(&manifest.harness.id, &candidates).map_err(|e| anyhow::anyhow!("{e}"))?;
        for (id, version) in &resolution.install {
            if *id == manifest.harness.id {
                continue;
            }
            let Some(dep) = candidates
                .iter()
                .find(|c| c.id == *id && c.version == *version)
            else {
                continue;
            };
            let path = dep.path.clone();
            self.trace(format!("installing dependency `{id}` {version} first"));
            match self.install_inner(&path, false)? {
                proto::Response::Ok => {}
                proto::Response::InstallPrompt { harness, .. } => anyhow::bail!(
                    "dependency `{harness}` widens capabilities; approve it before installing `{}`",
                    manifest.harness.id
                ),
                other => anyhow::bail!("installing dependency `{id}` failed: {other:?}"),
            }
        }
        for (interface, provider) in &resolution.bindings {
            self.trace(format!("`{interface}` is provided by `{provider}`"));
        }
        Ok(())
    }

    /// Every kind a package produces or accepts is declared by an installed
    /// types package (spec §18.3), so no consumer ever guesses at a format.
    fn interchange_kinds_declared(&self, manifest: &manifest::Manifest) -> Result<()> {
        for kind in manifest
            .contributes
            .produces
            .iter()
            .chain(manifest.contributes.accepts.iter())
        {
            if self.registry.type_decl(kind).is_none() {
                anyhow::bail!(
                    "`{kind}` is not declared by any installed types package; add the package that declares it to [dependencies]"
                );
            }
        }
        Ok(())
    }

    /// One types package declares a kind: a second declaring the same name
    /// would leave Core two answers to what an artifact is.
    fn types_declared_once(&self, staged: &registry::Installed) -> Result<()> {
        for kind in staged.types.kinds() {
            if let Some((other, _)) = self.registry.type_decl(kind)
                && other.id() != staged.id()
            {
                anyhow::bail!(
                    "`{kind}` is already declared by `{}`; one types package declares a kind",
                    other.id()
                );
            }
        }
        Ok(())
    }

    // -- conversations (v2 §8) ------------------------------------------------

    /// Write the transcript into the current conversation; the store writes
    /// it through.
    pub(crate) fn record_conversation(&mut self) {
        if self.evals_running {
            return;
        }
        self.conversations.record(&self.transcript, dag::now_ms());
    }

    fn conversations_response(&self) -> proto::Response {
        proto::Response::Conversations {
            list: self.conversations.summaries(),
            current: self.conversations.current.clone(),
        }
    }

    // -- models: the catalog, downloads, the sidecar (v2 §4) -----------------

    fn model_catalog(&self) -> proto::Response {
        let downloads = self.downloads.lock().unwrap().clone();
        proto::Response::ModelCatalog {
            entries: self.models.entries(
                &self.cfg.machine,
                &downloads,
                self.engine.as_ref().map(|e| e.model_id.as_str()),
            ),
        }
    }

    /// Provisioning egress (v2 §4.4): allowed unless the environment is
    /// air-gapped, in which case the answer is an offline import.
    fn download_model(&mut self, id: &str) -> Result<()> {
        let mode = self.gateway.lock().unwrap().config.mode;
        if mode == proto::NetworkMode::Airgapped {
            anyhow::bail!(
                "this environment is air-gapped: bring the file over and import it instead"
            );
        }
        self.models
            .download(id, self.downloads.clone(), self.sink())?;
        self.trace(format!("models: downloading `{id}`"));
        let _ = self.audit.append(
            self.actor(),
            self.scope("models"),
            "model.download",
            serde_json::json!({"model": id}),
            "started",
        );
        Ok(())
    }

    /// Start the sidecar on a model that is here, with the flags its
    /// placement plan calls for.
    fn load_model(&mut self, id: &str) -> Result<()> {
        let binary = engine::find_binary(
            self.cfg.llama_server.as_deref(),
            self.cfg.data_dir.as_deref(),
        )
        .ok_or_else(|| {
            anyhow::anyhow!(
                "llama-server is not installed: put {} under <data>/engines/, pass \
                     --llama-server <path>, or set LOCALSPACE_LLAMA_SERVER",
                engine::binary_name()
            )
        })?;
        let path = self
            .models
            .installed_path(id)
            .ok_or_else(|| anyhow::anyhow!("`{id}` is not downloaded yet"))?;
        let context_len = self
            .models
            .get(id)
            .map(|m| m.context_len.min(16384))
            .unwrap_or(8192);
        let flags = match self.models.placement(id, &self.cfg.machine)? {
            Some((map, plan)) => {
                self.trace(format!("planner: {}", plan.summary()));
                engine::flags(&plan, &map, context_len)
            }
            None => vec![
                "-c".into(),
                context_len.to_string(),
                "-ngl".into(),
                "999".into(),
            ],
        };
        if let Some(old) = self.engine.take() {
            old.stop();
        }
        let log_dir = self
            .cfg
            .data_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("localspace"))
            .join("engines");
        let engine = engine::Engine::start(
            &binary,
            id,
            &path,
            &flags,
            context_len,
            &log_dir,
            self.sink(),
            self.router.clone(),
        )?;
        self.trace(format!(
            "engine: started {} for {id} on 127.0.0.1:{} with {}",
            binary.display(),
            engine.port,
            flags.join(" ")
        ));
        let _ = self.audit.append(
            self.actor(),
            self.scope("models"),
            "model.load",
            serde_json::json!({"model": id, "flags": flags}),
            "started",
        );
        self.engine = Some(engine);
        self.broadcast_environment();
        Ok(())
    }

    // -- environment --------------------------------------------------------

    pub fn environment(&self) -> proto::EnvironmentState {
        let gw = self.gateway.lock().unwrap();
        let model = self.router.read().unwrap().info();
        let engine = match (&self.engine, &model) {
            (Some(engine), _) => engine.state(),
            (None, Some(m)) => proto::EngineState {
                running: true,
                loading: false,
                model: Some(m.id.clone()),
                detail: format!("endpoint: {}", m.backend),
            },
            (None, None) => proto::EngineState {
                running: false,
                loading: false,
                model: None,
                detail: "no model selected".into(),
            },
        };
        proto::EnvironmentState {
            user: self.active.user.clone(),
            network: gw.config.mode,
            network_ceiling: gw.config.ceiling,
            model,
            harnesses: self.registry.iter().map(|h| h.summary()).collect(),
            focus: self.focus.clone(),
            pinned: self.pinned.clone(),
            tier_b_permitted: self.cfg.policy.tier_b_permitted,
            topology: self.cfg.topology,
            workspace: self
                .access
                .workspace(&self.workspace)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| self.workspace.clone()),
            workspace_id: self.workspace.clone(),
            machine: self.cfg.machine.describe(),
            profile: self.cfg.profile.name.clone(),
            engine,
        }
    }

    /// The caller's own view, and a word to everyone else that theirs is
    /// stale: something shared moved.
    fn broadcast_environment(&self) {
        self.emit(proto::Event::EnvironmentChanged(self.environment()));
        self.emit_all(proto::Event::EnvironmentOutdated);
    }

    /// The caller's own view alone: focus and pins are theirs.
    fn emit_environment(&self) {
        self.emit(proto::Event::EnvironmentChanged(self.environment()));
    }

    // -- tool exposure ------------------------------------------------------

    pub fn active_set(&self) -> proto::ActiveSet {
        let network = self.gateway.lock().unwrap().config.mode;
        exposure::Exposure {
            registry: &self.registry,
            profile: &self.cfg.profile,
            focus: self.focus.as_deref(),
            pinned: &self.pinned,
            touched: &self.touched,
            network,
        }
        .active_set()
    }

    pub fn context_blocks(&mut self) -> Vec<proto::ContextBlock> {
        let mut also = self.pinned.clone();
        for t in &self.touched {
            if !also.contains(t) {
                also.push(t.clone());
            }
        }
        let focus = self.focus.clone();

        // Providers run inside the logic instance, so the harnesses about to be
        // asked are brought online first (they may have been idle-unloaded).
        let services = self.services();
        let mut wanted: Vec<String> = focus.iter().cloned().collect();
        wanted.extend(also.iter().cloned());
        for id in wanted {
            if let Some(h) = self.registry.get_mut(&id)
                && h.enabled
                && h.manifest.contributes.context_provider
                && let Err(e) = Registry::ensure_runtime(h, services.clone())
            {
                self.trace(format!(
                    "`{id}` could not start for its context provider: {e:#}"
                ));
            }
        }

        let mut doc_ids: HashMap<String, proto::DocId> = HashMap::new();
        for id in focus.iter().chain(also.iter()) {
            if let Some(doc) = self.doc_for(id) {
                doc_ids.insert(id.clone(), doc);
            }
        }
        context::assemble(
            &mut self.registry,
            &mut self.docs,
            &mut self.providers,
            &self.cfg.profile,
            focus.as_deref(),
            &also,
            &doc_ids,
        )
    }

    // -- the single mutation path -------------------------------------------

    /// Every tool call — from the agent, from the Client, from an eval — comes
    /// through here. Permission check, confirm gate, schema validation, run,
    /// commit, short result.
    pub fn call_tool(
        &mut self,
        tool: &str,
        params: &J,
        author: proto::Author,
    ) -> proto::ToolOutcome {
        if CORE_TOOLS.contains(&tool) {
            return self.call_core_tool(tool, params);
        }

        let Some(owner) = self.registry.owner_of(tool).map(|h| h.id().to_string()) else {
            return proto::ToolOutcome::Error {
                message: format!(
                    "no tool named `{tool}` is installed; call find_capability to look for one"
                ),
            };
        };

        let (decl, enabled) = {
            let h = self.registry.get(&owner).expect("owner exists");
            (h.tools.get(tool).cloned(), h.enabled)
        };
        let Some(doc_id) = self.doc_for(&owner) else {
            return proto::ToolOutcome::Error {
                message: format!("`{owner}` has no document to work on"),
            };
        };
        let Some(decl) = decl else {
            return proto::ToolOutcome::Error {
                message: format!("`{tool}` vanished from its harness"),
            };
        };
        if !enabled {
            return proto::ToolOutcome::Denied {
                reason: format!("`{owner}` is disabled in this environment"),
            };
        }

        // The tool must be in this turn's active set. A tool the model was not
        // shown is not callable, even if it exists.
        let active = self.active_set();
        if author == proto::Author::Agent && !active.tools.iter().any(|t| t.name == tool) {
            return proto::ToolOutcome::Denied {
                reason: format!(
                    "`{tool}` is not in this turn's tool set; call find_capability first"
                ),
            };
        }

        // ACL: a write needs edit on the document, a read needs view.
        let need = if decl.kind == tools::ToolKind::Write {
            Level::Edit
        } else {
            Level::View
        };
        if let Err(denied) = self.access.check(&self.identity(), &doc_id, need) {
            let _ = self.audit.append(
                self.actor(),
                self.scope(&doc_id),
                "tool.call",
                serde_json::json!({"harness": owner, "tool": tool}),
                "denied",
            );
            return proto::ToolOutcome::Denied {
                reason: denied.to_string(),
            };
        }

        // Schema validation before the harness ever sees the call.
        if let Err(e) = tools::validate_params(&decl.params, params) {
            return proto::ToolOutcome::Error {
                message: format!("{e}"),
            };
        }

        // Confirmation gate. `always` and `destructive` cannot be pre-approved
        // by the harness; only the user can answer them.
        let needs_confirm = match decl.confirm {
            tools::Confirm::Always => true,
            tools::Confirm::Destructive => decl.kind == tools::ToolKind::Write,
            tools::Confirm::Never => false,
        };
        if needs_confirm && author == proto::Author::Agent {
            let id = format!("confirm:{}", self.next_approval);
            self.next_approval += 1;
            let prompt = format!("Allow `{tool}` with {params}?");
            self.pending.insert(
                id.clone(),
                Pending {
                    tool: tool.to_string(),
                    params: params.clone(),
                    resume_agent: true,
                },
            );
            self.emit(proto::Event::ApprovalRequest {
                id,
                kind: proto::ApprovalKind::ToolConfirm,
                prompt: prompt.clone(),
            });
            return proto::ToolOutcome::AwaitingConfirm { prompt };
        }

        self.execute(tool, &owner, &doc_id, params, author, &decl)
    }

    /// Run the harness and commit whatever it changed.
    fn execute(
        &mut self,
        tool: &str,
        owner: &str,
        doc_id: &str,
        params: &J,
        author: proto::Author,
        decl: &tools::ToolDecl,
    ) -> proto::ToolOutcome {
        let doc = self.docs.json(doc_id).unwrap_or(J::Null);

        // A handoff (spec §18.2): an `artifact` parameter names something in the
        // ledger. Core resolves it, checks this harness accepts its kind, and
        // hands the pinned content to the call — the harness never guesses.
        let handoff = match self.resolve_handoff(owner, params) {
            Ok(h) => h,
            Err(reason) => return proto::ToolOutcome::Denied { reason },
        };

        // Instances are made on first call and dropped when idle (§1.2), so
        // every call path brings the logic online itself.
        let services = self.services();
        let (out, over_budget) = {
            let Some(h) = self.registry.get_mut(owner) else {
                return proto::ToolOutcome::Error {
                    message: format!("`{owner}` is not installed"),
                };
            };
            if let Err(e) = Registry::ensure_runtime(h, services) {
                return proto::ToolOutcome::Error {
                    message: format!("`{owner}` could not start: {e:#}"),
                };
            }
            let rt = h.runtime.as_mut().expect("ensured above");
            rt.set_artifacts(handoff);
            let out = rt.call(tool, params, &doc);
            rt.set_artifacts(Vec::new());
            (out, rt.over_budget())
        };
        if let Some(asked) = over_budget {
            self.kill_over_budget(owner, asked);
        }

        let out = match out {
            Ok(o) => o,
            Err(e) => {
                let _ = self.audit.append(
                    self.actor(),
                    self.scope(doc_id),
                    "tool.call",
                    serde_json::json!({"harness": owner, "tool": tool}),
                    "error",
                );
                return proto::ToolOutcome::Error {
                    message: format!("{e:#}"),
                };
            }
        };

        for line in &out.logs {
            self.trace(format!("[{owner}] {line}"));
        }

        if !out.ok {
            return proto::ToolOutcome::Error {
                message: out
                    .error
                    .unwrap_or_else(|| "the harness reported a failure".into()),
            };
        }

        // The document is authoritative here, not the harness's own words.
        let mut commit_id = None;
        let mut diff_summary = out
            .diff_summary
            .clone()
            .unwrap_or_else(|| "no change".into());

        if let Some(next) = out.doc {
            match self.docs.apply_json(doc_id, &next) {
                Ok(changes) => {
                    if !changes.is_empty() {
                        diff_summary = out
                            .diff_summary
                            .clone()
                            .unwrap_or_else(|| changes.summary());
                        let snapshot = self.docs.snapshot(doc_id).unwrap_or_default();
                        match self.dag.commit(
                            doc_id,
                            owner,
                            tool,
                            Json(params.clone()),
                            &snapshot,
                            &diff_summary,
                            author,
                            self.run.clone(),
                        ) {
                            Ok(c) => {
                                commit_id = Some(c.id.clone());
                                self.doc_changed(doc_id);
                            }
                            Err(e) => {
                                return proto::ToolOutcome::Error {
                                    message: format!("the change could not be committed: {e:#}"),
                                };
                            }
                        }
                    }
                }
                Err(e) => {
                    return proto::ToolOutcome::Error {
                        message: format!(
                            "the harness returned a document Core could not apply: {e:#}"
                        ),
                    };
                }
            }
        }

        // Focus follows the last tool call.
        self.focus = Some(owner.to_string());
        if !self.touched.iter().any(|t| t == owner) {
            self.touched.push(owner.to_string());
        }

        let _ = self.audit.append(
            self.actor(),
            self.scope(doc_id),
            "tool.call",
            serde_json::json!({
                "harness": owner,
                "tool": tool,
                "confirm": format!("{:?}", decl.confirm).to_lowercase(),
                "commit": commit_id,
            }),
            "ok",
        );

        // A result may register an artifact (spec §18.3): a typed, pinned
        // reference other harnesses can import. Only of a kind this harness
        // declared it `produces`; otherwise it is refused, out loud.
        if let Some(spec) = out.result.get("artifact").cloned() {
            match self.register_artifact(
                owner,
                doc_id,
                commit_id.clone(),
                &spec,
                &diff_summary,
                None,
            ) {
                Ok(id) => {
                    let kind = spec.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
                    diff_summary = format!("{diff_summary} → {id} ({kind})");
                }
                Err(e) => self.trace(format!("[{owner}] artifact refused: {e}")),
            }
        }

        proto::ToolOutcome::Ok {
            diff_summary,
            result: Json(out.result),
            commit: commit_id,
        }
    }

    /// A document changed: tell whatever reads its JSON, and send every
    /// replica of it what that replica lacks.
    fn doc_changed(&mut self, doc_id: &str) {
        self.emit_all(proto::Event::DocChanged {
            doc: doc_id.to_string(),
        });
        let peers: Vec<String> = self
            .sync_states
            .get(doc_id)
            .map(|replicas| replicas.keys().cloned().collect())
            .unwrap_or_default();
        for peer in peers {
            self.sync_replica(doc_id, &peer);
        }
    }

    /// Send one replica what it lacks, if anything.
    fn sync_replica(&mut self, doc_id: &str, peer: &str) {
        let Some(replica) = self
            .sync_states
            .get_mut(doc_id)
            .and_then(|replicas| replicas.get_mut(peer))
        else {
            return;
        };
        let owner = replica.user.clone();
        if let Some(message) = self.docs.sync_message(doc_id, &mut replica.state) {
            self.emit_to(
                To::User(owner),
                proto::Event::DocPatch {
                    doc: doc_id.to_string(),
                    peer: peer.to_string(),
                    message,
                },
            );
        }
    }

    /// A replica's sync state, taken out while a message is applied; a
    /// replica Core has not heard from starts afresh.
    fn take_replica(&mut self, doc_id: &str, peer: &str) -> ReplicaSync {
        self.sync_states
            .get_mut(doc_id)
            .and_then(|replicas| replicas.remove(peer))
            .unwrap_or_default()
    }

    /// Put a replica's state back, marked as just heard from. Past
    /// `MAX_REPLICAS_PER_DOC`, the one heard from longest ago makes room.
    fn put_replica(&mut self, doc_id: &str, peer: &str, mut replica: ReplicaSync) {
        self.sync_clock += 1;
        replica.seen = self.sync_clock;
        replica.user = self.active.user.clone();
        let replicas = self.sync_states.entry(doc_id.to_string()).or_default();
        if replicas.len() >= MAX_REPLICAS_PER_DOC
            && let Some(oldest) = replicas
                .iter()
                .min_by_key(|(_, r)| r.seen)
                .map(|(name, _)| name.clone())
        {
            replicas.remove(&oldest);
        }
        replicas.insert(peer.to_string(), replica);
    }

    // -- Core's own tools ---------------------------------------------------

    fn call_core_tool(&mut self, tool: &str, params: &J) -> proto::ToolOutcome {
        match tool {
            "find_capability" => {
                let need = params.get("need").and_then(|n| n.as_str()).unwrap_or("");
                let hits = exposure::rank_capabilities(&self.registry, need);
                // The matching harness is promoted to focused on the next turn.
                if let Some(top) = hits.first() {
                    self.focus = Some(top.harness.clone());
                    if !self.touched.contains(&top.harness) {
                        self.touched.push(top.harness.clone());
                    }
                }
                let summary = if hits.is_empty() {
                    format!("nothing installed matches `{need}`")
                } else {
                    format!(
                        "{} match(es); `{}` is now focused",
                        hits.len(),
                        hits[0].harness
                    )
                };
                proto::ToolOutcome::Ok {
                    diff_summary: summary,
                    result: Json(serde_json::to_value(&hits).unwrap_or(J::Null)),
                    commit: None,
                }
            }
            "task.plan" => {
                // The agent writes its intended harness per step into the ledger
                // (spec §18.4 step 1). Unknown harnesses are refused by name so
                // a plan never points at something that is not installed.
                let steps = params
                    .get("steps")
                    .and_then(|s| s.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut plan = Vec::new();
                for s in &steps {
                    let harness = s.get("harness").and_then(|h| h.as_str()).unwrap_or("");
                    let intent = s.get("intent").and_then(|i| i.as_str()).unwrap_or("");
                    if self.registry.get(harness).is_none() {
                        return proto::ToolOutcome::Error {
                            message: format!(
                                "`{harness}` is not installed; find_capability lists what is"
                            ),
                        };
                    }
                    plan.push(proto::Step {
                        harness: harness.to_string(),
                        intent: intent.to_string(),
                        status: proto::StepStatus::Pending,
                    });
                }
                let n = plan.len();
                self.task.plan = plan;
                self.emit_task();
                proto::ToolOutcome::Ok {
                    diff_summary: format!("plan written: {n} step(s)"),
                    result: Json(serde_json::json!({"steps": n})),
                    commit: None,
                }
            }
            "task.note" => {
                let text = params
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    return proto::ToolOutcome::Error {
                        message: "a note needs some text".into(),
                    };
                }
                self.task.notes.push(text);
                self.emit_task();
                proto::ToolOutcome::Ok {
                    diff_summary: "noted".into(),
                    result: Json(serde_json::json!({"notes": self.task.notes.len()})),
                    commit: None,
                }
            }
            "web.search" => {
                let query = params.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let site = params.get("site").and_then(|s| s.as_str());
                let mut gw = self.gateway.lock().unwrap();
                match gw.search(query, site) {
                    Ok(hits) => proto::ToolOutcome::Ok {
                        diff_summary: format!("{} result(s)", hits.len()),
                        result: Json(serde_json::to_value(&hits).unwrap_or(J::Null)),
                        commit: None,
                    },
                    Err(e) => proto::ToolOutcome::Error {
                        message: format!("{e:#}"),
                    },
                }
            }
            "web.fetch" => {
                let url = params.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let mode = params
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("text");
                let decision = self.gateway.lock().unwrap().check(url);
                match decision {
                    Egress::Denied(why) => proto::ToolOutcome::Denied { reason: why },
                    Egress::NeedsApproval(domain) => {
                        let id = format!("egress:{}", self.next_approval);
                        self.next_approval += 1;
                        self.pending.insert(
                            id.clone(),
                            Pending {
                                tool: "web.fetch".into(),
                                params: params.clone(),
                                resume_agent: true,
                            },
                        );
                        let prompt = format!("Allow this environment to fetch from `{domain}`?");
                        self.emit(proto::Event::ApprovalRequest {
                            id,
                            kind: proto::ApprovalKind::Egress,
                            prompt: prompt.clone(),
                        });
                        proto::ToolOutcome::AwaitingConfirm { prompt }
                    }
                    Egress::Allowed => {
                        let fetched = self.gateway.lock().unwrap().fetch(url, mode);
                        match fetched {
                            Ok(f) => {
                                let _ = self.audit.append(
                                    self.actor(),
                                    self.scope(""),
                                    "gateway.fetch",
                                    serde_json::json!({"url": f.url, "bytes": f.bytes}),
                                    "ok",
                                );
                                // Fetched content is data. It enters the model's
                                // context wrapped, never as instruction.
                                let body = prompt::untrusted(&f.url, &f.content);
                                proto::ToolOutcome::Ok {
                                    diff_summary: format!(
                                        "fetched {} ({} bytes), cached with a citation",
                                        f.url, f.bytes
                                    ),
                                    result: Json(serde_json::json!({
                                        "url": f.url,
                                        "title": f.title,
                                        "fetched_at": f.fetched_at_ms,
                                        "content": body,
                                    })),
                                    commit: None,
                                }
                            }
                            Err(e) => proto::ToolOutcome::Error {
                                message: format!("{e:#}"),
                            },
                        }
                    }
                }
            }
            _ => proto::ToolOutcome::Error {
                message: format!("`{tool}` is not a Core tool"),
            },
        }
    }

    // -- undo / redo / proposals -------------------------------------------

    fn apply_revert(&mut self, revert: dag::Revert) -> proto::Response {
        if let Err(e) = self.docs.restore(&revert.doc, revert.bytes.as_deref()) {
            return proto::Response::Error {
                message: format!("{e:#}"),
            };
        }
        self.providers = ProviderCache::new();
        self.doc_changed(&revert.doc);
        self.notice(proto::NoticeLevel::Info, revert.summary);
        proto::Response::Ok
    }

    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    // -- package management (spec §17) ---------------------------------------

    /// Every directory the resolver and the marketplace may draw from: the
    /// catalog dirs, plus the installed set's own directory.
    fn catalog_dirs_all(&self) -> Vec<PathBuf> {
        let mut dirs = self.cfg.catalog_dirs.clone();
        if let Some(installed) = &self.cfg.harness_dir
            && !dirs.contains(installed)
        {
            dirs.push(installed.clone());
        }
        dirs
    }

    /// Rewrite `environment.lock` from the installed set. It is a document in
    /// the DAG, so every change to the environment is a commit with a diff.
    fn write_lock(&mut self) {
        let lock = lock::compute(&self.registry);
        self.docs.ensure(lock::LOCK_DOC, proto::DocKind::Crdt);
        match self.docs.apply_json(lock::LOCK_DOC, &lock) {
            Ok(changes) if !changes.is_empty() => {
                let snapshot = self.docs.snapshot(lock::LOCK_DOC).unwrap_or_default();
                let summary = format!(
                    "environment.lock: {} package(s)",
                    lock["packages"].as_array().map(|a| a.len()).unwrap_or(0)
                );
                if let Err(e) = self.dag.commit(
                    lock::LOCK_DOC,
                    "core",
                    "environment.lock",
                    Json(J::Null),
                    &snapshot,
                    &summary,
                    proto::Author::User,
                    None,
                ) {
                    self.trace(format!("environment.lock could not be committed: {e:#}"));
                }
            }
            Ok(_) => {}
            Err(e) => self.trace(format!("environment.lock could not be written: {e:#}")),
        }
    }

    // -- the task ledger (spec §18) ------------------------------------------

    pub(crate) fn emit_task(&self) {
        if let Err(e) = self
            .store
            .put_ledger(&self.active.user, &self.workspace, &self.task)
        {
            self.trace(format!("ledger: not written: {e:#}"));
        }
        self.emit(proto::Event::TaskChanged(self.task.clone()));
    }

    /// Move the plan along as tools run: a call into a step's harness makes
    /// that step active; a refusal or error fails it. Steps settle to done
    /// when the turn ends.
    pub(crate) fn task_progress(&mut self, tool: &str, outcome: &proto::ToolOutcome) {
        let Some(owner) = self.registry.owner_of(tool).map(|h| h.id().to_string()) else {
            return;
        };
        let Some(step) = self.task.plan.iter_mut().find(|s| {
            s.harness == owner
                && matches!(
                    s.status,
                    proto::StepStatus::Pending | proto::StepStatus::Active
                )
        }) else {
            return;
        };
        let next = match outcome {
            proto::ToolOutcome::Error { .. } | proto::ToolOutcome::Denied { .. } => {
                proto::StepStatus::Failed
            }
            _ => proto::StepStatus::Active,
        };
        if step.status != next {
            step.status = next;
            self.emit_task();
        }
    }

    /// If the call names an `artifact`, resolve it and check the handoff.
    /// Returns what to hand the harness: `(id, JSON payload)` pairs.
    fn resolve_handoff(
        &mut self,
        owner: &str,
        params: &J,
    ) -> std::result::Result<Vec<(String, String)>, String> {
        let Some(id) = params.get("artifact").and_then(|a| a.as_str()) else {
            return Ok(Vec::new());
        };
        let Some(art) = self.task.artifacts.iter().find(|a| a.id == id).cloned() else {
            let known: Vec<&str> = self.task.artifacts.iter().map(|a| a.id.as_str()).collect();
            return Err(if known.is_empty() {
                format!("`{id}` is not an artifact in this task; nothing has been produced yet")
            } else {
                format!(
                    "`{id}` is not an artifact in this task; the ledger has {}",
                    known.join(", ")
                )
            });
        };

        let accepts = self
            .registry
            .get(owner)
            .map(|h| h.manifest.contributes.accepts.clone())
            .unwrap_or_default();
        if !accepts.contains(&art.kind) {
            let takers = task::who_accepts(&self.registry, &art.kind);
            return Err(if takers.is_empty() {
                format!(
                    "`{owner}` does not accept {}, and nothing installed does",
                    art.kind
                )
            } else {
                format!(
                    "`{owner}` does not accept {}; {} does",
                    art.kind,
                    takers.join(", ")
                )
            });
        }

        // The content at the pinned version, not whatever the producer's
        // document has become since.
        let kind = self.docs.kind(&art.doc).unwrap_or(proto::DocKind::Crdt);
        let content = if art.commit.is_empty() {
            self.docs.json(&art.doc).unwrap_or(J::Null)
        } else {
            match self.dag.get_commit(&art.commit) {
                Ok(Some(c)) => match self.dag.get_blob(&c.doc_hash) {
                    Ok(Some(bytes)) => DocStore::json_of_snapshot(kind, &bytes).unwrap_or(J::Null),
                    _ => {
                        return Err(format!(
                            "the version {} is pinned to is missing from the DAG",
                            art.id
                        ));
                    }
                },
                _ => return Err(format!("commit {} is not in the DAG", art.commit)),
            }
        };

        let payload = serde_json::json!({
            "id": art.id,
            "kind": art.kind,
            "summary": art.summary,
            "produced-by": art.produced_by,
            "commit": art.commit,
            "content": content,
        });
        let _ = self.audit.append(
            self.actor(),
            self.scope(&art.doc),
            "artifact.handoff",
            serde_json::json!({"artifact": art.id, "kind": art.kind, "to": owner}),
            "ok",
        );
        Ok(vec![(art.id.clone(), payload.to_string())])
    }

    /// Pin a typed artifact to the document's current version and put it in
    /// the ledger. Refused when the harness never declared it `produces` the kind.
    fn register_artifact(
        &mut self,
        owner: &str,
        doc_id: &str,
        commit: Option<String>,
        spec: &J,
        fallback_summary: &str,
        file: Option<proto::ArtifactFile>,
    ) -> std::result::Result<String, String> {
        let kind = spec
            .get("kind")
            .and_then(|k| k.as_str())
            .ok_or("an artifact needs a `kind`")?
            .to_string();
        let produces = self
            .registry
            .get(owner)
            .map(|h| h.manifest.contributes.produces.clone())
            .unwrap_or_default();
        if !produces.contains(&kind) {
            return Err(format!(
                "`{owner}` does not declare that it produces {kind}; add it to [contributes] produces"
            ));
        }
        // The type's declaration says which fields the artifact carries (spec
        // §18.3): a rendering names the document and commit it came from.
        let fields = spec
            .get("fields")
            .and_then(|f| f.as_object())
            .cloned()
            .unwrap_or_default();
        let lacking = match self.registry.type_decl(&kind) {
            Some((package, decl)) => {
                let missing = decl.missing(&fields);
                (!missing.is_empty()).then(|| (package.id().to_string(), missing.join(", ")))
            }
            None => {
                return Err(format!(
                    "`{kind}` is not declared by any installed types package"
                ));
            }
        };
        if let Some((package, missing)) = lacking {
            return Err(format!(
                "an artifact of `{kind}` carries the field(s) {missing}, says `{package}`; this one lacks them"
            ));
        }
        let commit = match commit {
            Some(c) => c,
            None => self.dag.head(doc_id).ok().flatten().unwrap_or_default(),
        };
        let id = task::next_artifact_id(&self.task);
        let summary = spec
            .get("summary")
            .and_then(|s| s.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_summary)
            .to_string();
        self.task.artifacts.push(proto::Artifact {
            id: id.clone(),
            kind: kind.clone(),
            doc: doc_id.to_string(),
            commit: commit.clone(),
            summary,
            produced_by: owner.to_string(),
            fields: proto::Json(J::Object(fields)),
            file,
        });
        let _ = self.audit.append(
            self.actor(),
            self.scope(doc_id),
            "artifact.produced",
            serde_json::json!({"artifact": id, "kind": kind, "commit": commit, "by": owner}),
            "ok",
        );
        self.emit_task();
        Ok(id)
    }

    /// A file a surface rendered from its harness's document — a PNG or an
    /// SVG of a board — kept as a document of its own and registered as a
    /// typed artifact pinned to it (plugin spec §18.3). The export is not a
    /// change to the board: its commit is on the export document, so the
    /// board's history stays the board's. Everything is checked before
    /// anything is written.
    #[allow(clippy::too_many_arguments)]
    fn produce_artifact(
        &mut self,
        harness: &str,
        view: &str,
        kind: &str,
        offered_name: &str,
        mime: &str,
        bytes: Vec<u8>,
        fields: J,
        summary: &str,
    ) -> std::result::Result<proto::Artifact, String> {
        let Some(h) = self.registry.get(harness) else {
            return Err(format!("no harness `{harness}`"));
        };
        if h.manifest.view(view).is_none() {
            return Err(format!("`{harness}` has no view `{view}`"));
        }
        if !h.manifest.contributes.produces.iter().any(|k| k == kind) {
            return Err(format!(
                "`{harness}` does not declare that it produces {kind}; add it to [contributes] produces"
            ));
        }
        let Some(own_doc) = self.doc_for(harness) else {
            return Err(format!("`{harness}` has no document to export"));
        };
        if let Err(denied) = self.access.check(&self.identity(), &own_doc, Level::View) {
            return Err(denied.to_string());
        }
        let Some((package, decl)) = self.registry.type_decl(kind) else {
            return Err(format!(
                "`{kind}` is not declared by any installed types package"
            ));
        };
        if decl.mime != mime {
            return Err(format!(
                "an artifact of `{kind}` is `{}`, says `{}`; this export claims `{mime}`",
                decl.mime,
                package.id()
            ));
        }
        let extension = decl.extension.clone();
        let mut fields = fields.as_object().cloned().unwrap_or_default();
        // A surface knows its board, not Core's history: the document and
        // the head commit are filled in here when it does not name them, and
        // checked against the same when it does.
        let head = self.dag.head(&own_doc).ok().flatten();
        fields
            .entry("document".to_string())
            .or_insert_with(|| J::String(own_doc.clone()));
        fields
            .entry("commit".to_string())
            .or_insert_with(|| J::String(head.clone().unwrap_or_default()));
        let missing = decl.missing(&fields);
        if !missing.is_empty() {
            return Err(format!(
                "an artifact of `{kind}` carries the field(s) {}, says `{}`; this one lacks them",
                missing.join(", "),
                package.id()
            ));
        }
        // Provenance is real or refused: a surface exports its own harness's
        // document, at a commit that is on it. An untouched document has no
        // commits, and its export says so with an empty one.
        let source_doc = fields.get("document").and_then(|d| d.as_str());
        if let Some(document) = source_doc
            && document != own_doc
        {
            return Err(format!(
                "`{harness}` may export its own document `{own_doc}`, not `{document}`"
            ));
        }
        let source_commit = fields
            .get("commit")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        if fields.contains_key("commit") {
            match self.dag.get_commit(&source_commit) {
                Ok(Some(c)) if c.doc == own_doc => {}
                Ok(Some(c)) => {
                    return Err(format!(
                        "commit `{source_commit}` is on `{}`, not on `{own_doc}`",
                        c.doc
                    ));
                }
                _ => {
                    if !(source_commit.is_empty() && head.is_none()) {
                        return Err(format!("`{source_commit}` is not a commit on `{own_doc}`"));
                    }
                }
            }
        }
        // `<title>-<commit7>.png`: the name says which board state it shows.
        let suffix: String = source_commit.chars().take(7).collect();
        let name = docs::file_name(offered_name, &suffix, &extension);
        if bytes.is_empty() {
            return Err("the export is empty".into());
        }
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(format!(
                "the export is {} bytes; the limit is {} MiB",
                bytes.len(),
                MAX_ARTIFACT_BYTES >> 20
            ));
        }

        // Identical bytes are one document; each export is its own commit.
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let doc_id = format!("blob:{hash}");
        let size = bytes.len() as u64;
        self.docs.ensure(&doc_id, proto::DocKind::Blob);
        let ws = self.workspace.clone();
        self.access.add_document(&doc_id, &ws, &name, None);
        let diff = format!("exported {name}, {size} bytes of {kind}");
        let commit = self
            .dag
            .commit(
                &doc_id,
                harness,
                "surface:export",
                Json(serde_json::json!({
                    "kind": kind,
                    "name": name,
                    "document": own_doc,
                    "commit": source_commit,
                })),
                &bytes,
                &diff,
                proto::Author::User,
                None,
            )
            .map_err(|e| format!("{e:#}"))?;
        let record = store::DocumentRecord {
            title: name.clone(),
            kind: proto::DocKind::Blob,
            mime: mime.to_string(),
            bytes: size,
            hash,
            source: proto::DocumentSource::Export {
                harness: harness.to_string(),
                document: own_doc.clone(),
                commit: source_commit,
            },
            created_ms: dag::now_ms(),
            workspace: ws,
            harness: None,
            created_by: self.active.user.clone(),
            acl: None,
        };
        self.store
            .put_document(&doc_id, &record)
            .map_err(|e| format!("{e:#}"))?;
        let _ = self.audit.append(
            self.actor(),
            self.scope(&doc_id),
            "document.create",
            serde_json::json!({
                "source": "export",
                "harness": harness,
                "kind": kind,
                "mime": mime,
                "bytes": size,
                "name": name,
            }),
            "ok",
        );
        let spec = serde_json::json!({"kind": kind, "summary": summary, "fields": fields});
        let file = proto::ArtifactFile {
            name,
            mime: mime.to_string(),
            bytes: size,
        };
        let id =
            self.register_artifact(harness, &doc_id, Some(commit.id), &spec, &diff, Some(file))?;
        self.emit(proto::Event::DocChanged { doc: doc_id });
        self.task
            .artifacts
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| format!("`{id}` was registered and is not in the ledger"))
    }

    /// Every document the caller may see, from the records: a harness's, an
    /// export, later an upload.
    fn list_documents(&self) -> Vec<proto::DocumentInfo> {
        let identity = self.identity();
        let mut out = Vec::new();
        for (id, record) in self.store.documents().unwrap_or_default() {
            if self.access.check(&identity, &id, Level::View).is_err() {
                continue;
            }
            let head = self.dag.head(&id).ok().flatten();
            let hash = head
                .as_ref()
                .and_then(|c| self.dag.get_commit(c).ok().flatten())
                .map(|c| c.doc_hash)
                .unwrap_or_default();
            let is_file = record.harness.is_none();
            out.push(proto::DocumentInfo {
                id,
                title: record.title,
                kind: record.kind,
                mime: record.mime,
                // A file's size is its content's only while that content is
                // the head's: undone, the document has none. A harness's
                // document has no size to give.
                bytes: (is_file && hash == record.hash).then_some(record.bytes),
                hash,
                head,
                source: record.source,
                created_ms: Some(record.created_ms),
            });
        }
        out.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
        out
    }

    /// A file document's bytes at its head, with the name and media type a
    /// download needs. Reading a file is a document read, and audited as one.
    fn doc_blob(&self, doc: &str) -> std::result::Result<(String, String, Vec<u8>), String> {
        if let Err(denied) = self.access.check(&self.identity(), doc, Level::View) {
            return Err(denied.to_string());
        }
        let record = self
            .store
            .get_document(doc)
            .map_err(|e| format!("{e:#}"))?
            .filter(|r| r.harness.is_none())
            .ok_or_else(|| format!("`{doc}` is not a file"))?;
        let head = self
            .dag
            .head(doc)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| format!("`{doc}` has no content; its export was undone"))?;
        let commit = self
            .dag
            .get_commit(&head)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| format!("commit `{head}` is missing from the history"))?;
        let bytes = self
            .dag
            .get_blob(&commit.doc_hash)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| format!("the content of `{doc}` is missing from the blob store"))?;
        let _ = self.audit.append(
            self.actor(),
            self.scope(doc),
            "document.read",
            serde_json::json!({"name": record.title, "bytes": bytes.len()}),
            "ok",
        );
        Ok((record.title, record.mime, bytes))
    }

    // -- residency (spec §1.2) ----------------------------------------------

    /// Housekeeping: drop logic instances idle past their declared
    /// `idle_unload`. Called before every request and on the transport's idle
    /// timer, so an environment with thirty harnesses installed and one in use
    /// costs one harness of memory.
    pub fn tick(&mut self) {
        let dropped = self.registry.unload_idle(std::time::Instant::now());
        if dropped.is_empty() {
            return;
        }
        for id in &dropped {
            self.trace(format!(
                "unloaded `{id}` after idle_unload; its document stays"
            ));
        }
        // The Library shows which harnesses are resident; tell it.
        self.broadcast_environment();
    }

    /// A harness asked for more memory than it declared. Its instance is
    /// dropped now — the next call re-instantiates it fresh — and the user is
    /// told, because a silent restart hides a real defect in the harness.
    fn kill_over_budget(&mut self, harness: &str, asked_bytes: u64) {
        let budget_mb = self
            .registry
            .get(harness)
            .map(|h| h.manifest.resources.memory_mb.logic)
            .unwrap_or(0);
        if let Some(h) = self.registry.get_mut(harness) {
            h.runtime = None;
        }
        let text = format!(
            "`{harness}` exceeded its declared memory budget of {budget_mb} MB (asked for {} MB). \
             It was stopped and will restart on its next call.",
            asked_bytes.div_ceil(1024 * 1024)
        );
        self.notice(proto::NoticeLevel::Warn, text);
        let _ = self.audit.append(
            self.actor(),
            self.scope(""),
            "harness.over_budget",
            serde_json::json!({
                "harness": harness,
                "budget_mb": budget_mb,
                "asked_mb": asked_bytes.div_ceil(1024 * 1024),
            }),
            "killed",
        );
        self.broadcast_environment();
    }

    // -- request dispatch ---------------------------------------------------

    /// A request from the local user of a personal Core.
    pub fn handle(&mut self, req: proto::Request) -> proto::Response {
        let caller = Caller::local(&self.cfg.user);
        self.handle_as(&caller, req)
    }

    /// A request from `caller`: the per-user state becomes theirs, every
    /// check is made as them, and every event of the handling goes to them.
    pub fn handle_as(&mut self, caller: &Caller, req: proto::Request) -> proto::Response {
        if caller.is_system() {
            return self.handle_system(req);
        }
        self.activate(caller);
        self.handle_inner(req)
    }

    /// Refuse unless the caller is an administrator; the refusal is audited
    /// under `event`.
    fn require_admin(&mut self, event: &str) -> Option<proto::Response> {
        if self.active.is_admin() {
            return None;
        }
        let _ = self.audit.append(
            self.actor(),
            self.scope(""),
            event,
            serde_json::json!({}),
            "denied",
        );
        Some(proto::Response::Error {
            message: "Only an administrator can do that.".into(),
        })
    }

    /// Refuse unless the caller owns the workspace or is an administrator;
    /// a personal workspace takes no members at all.
    fn require_workspace_owner(&mut self, workspace: &str, event: &str) -> Option<proto::Response> {
        let Some(ws) = self.access.workspace(workspace) else {
            return Some(proto::Response::Error {
                message: format!("no workspace `{workspace}`"),
            });
        };
        if ws.personal_to.is_some() {
            return Some(proto::Response::Error {
                message: "A personal workspace is its owner's alone; share from a workspace an administrator made.".into(),
            });
        }
        let owner = ws.level_of(&self.identity()) == Some(Level::Owner);
        if owner || self.active.is_admin() {
            return None;
        }
        let _ = self.audit.append(
            self.actor(),
            Scope {
                workspace: workspace.to_string(),
                conversation: String::new(),
                document: String::new(),
            },
            event,
            serde_json::json!({}),
            "denied",
        );
        Some(proto::Response::Error {
            message: "Only the workspace's owner or an administrator can do that.".into(),
        })
    }

    fn persist_workspace(&mut self, id: &str) {
        if let Some(ws) = self.access.workspace(id).cloned()
            && let Err(e) = self.store.put_workspace(&ws)
        {
            self.trace(format!("workspace {id}: not written: {e:#}"));
        }
    }

    /// The workspaces as the caller sees them: theirs, with their level;
    /// every one for an administrator.
    fn workspace_infos(&self) -> Vec<proto::WorkspaceInfo> {
        let identity = self.identity();
        let admin = self.active.is_admin();
        let mut out: Vec<proto::WorkspaceInfo> = self
            .access
            .workspaces()
            .filter_map(|ws| {
                let mine = ws.level_of(&identity);
                if mine.is_none() && !admin {
                    return None;
                }
                Some(proto::WorkspaceInfo {
                    id: ws.id.clone(),
                    name: ws.name.clone(),
                    personal_to: ws.personal_to.clone(),
                    members: ws
                        .default_acl
                        .entries
                        .iter()
                        .map(|(p, l)| proto::Member {
                            principal: proto_principal(p),
                            level: proto_level(*l),
                        })
                        .collect(),
                    agent_writes: match ws.agent_writes {
                        acl::AgentWrites::Direct => proto::AgentWrites::Direct,
                        acl::AgentWrites::Proposal => proto::AgentWrites::Proposal,
                    },
                    created_ms: ws.created_ms,
                    mine: mine.map(proto_level),
                    current: ws.id == self.workspace,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.personal_to
                .is_some()
                .cmp(&a.personal_to.is_some())
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    /// Go to a workspace: a member goes; an administrator who is not one
    /// goes with a reason, audited as break-glass (deployment §6.1). The
    /// caller's conversations and ledger become the workspace's.
    fn select_workspace(&mut self, workspace: &str, reason: Option<String>) -> proto::Response {
        let identity = self.identity();
        let Some(ws) = self.access.workspace(workspace).cloned() else {
            return proto::Response::Error {
                message: "There is no such workspace.".into(),
            };
        };
        let member = ws.level_of(&identity).is_some();
        if member {
            // A member's way in needs no reason, and ends any break-glass.
            self.break_glass = None;
        } else {
            let reason = reason
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty());
            match (self.active.is_admin(), reason) {
                (true, Some(reason)) => {
                    let _ = self.audit.append(
                        self.actor(),
                        Scope {
                            workspace: workspace.to_string(),
                            conversation: String::new(),
                            document: String::new(),
                        },
                        "workspace.break_glass",
                        serde_json::json!({"reason": reason.as_str()}),
                        "ok",
                    );
                    self.break_glass = Some(BreakGlass {
                        workspace: ws.id.clone(),
                        reason,
                    });
                }
                (true, None) => {
                    return proto::Response::Error {
                        message: "You are not a member of that workspace. As an administrator you can open it with a reason, which is kept.".into(),
                    };
                }
                (false, _) => {
                    let _ = self.audit.append(
                        self.actor(),
                        Scope {
                            workspace: workspace.to_string(),
                            conversation: String::new(),
                            document: String::new(),
                        },
                        "workspace.select",
                        serde_json::json!({}),
                        "denied",
                    );
                    return proto::Response::Error {
                        message: "You are not a member of that workspace.".into(),
                    };
                }
            }
        }
        if self.workspace != ws.id {
            self.record_conversation();
            self.workspace = ws.id.clone();
            let user = self.active.user.clone();
            self.conversations =
                conversations::Store::load(&self.store, &user, &self.workspace, dag::now_ms());
            self.transcript = self
                .conversations
                .current()
                .map(|c| c.messages.clone())
                .unwrap_or_default();
            self.task = self
                .store
                .ledger(&user, &self.workspace)
                .unwrap_or_default()
                .unwrap_or_default();
            if let Err(e) = self.store.set_current_workspace(&user, &self.workspace) {
                self.trace(format!("current workspace: not written: {e:#}"));
            }
            let _ = self.audit.append(
                self.actor(),
                Scope {
                    workspace: workspace.to_string(),
                    conversation: String::new(),
                    document: String::new(),
                },
                "workspace.select",
                serde_json::json!({}),
                "ok",
            );
            self.emit(proto::Event::ConversationChanged {
                current: self.conversations.current.clone(),
            });
            self.emit_task();
        }
        self.emit_environment();
        proto::Response::Environment(self.environment())
    }

    fn user_infos(&self) -> Vec<proto::UserInfo> {
        let now = dag::now_ms();
        self.directory
            .users()
            .unwrap_or_default()
            .iter()
            .map(|u| {
                identity::info(
                    u,
                    self.directory
                        .account_locked_until(&u.id, now)
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    /// The actor of a sign-in event: the account named, from the address
    /// given, before there is a session to speak of.
    fn auth_actor(user: &str, ip: &str) -> Actor {
        Actor {
            user: user.to_string(),
            session: String::new(),
            ip: ip.to_string(),
            role: String::new(),
            break_glass: None,
        }
    }

    /// The server's own requests: signing users in and out. They touch no
    /// per-user state, so the system caller never gets any.
    fn handle_system(&mut self, req: proto::Request) -> proto::Response {
        use proto::Request as R;
        let now = dag::now_ms();
        let no_scope = Scope {
            workspace: String::new(),
            conversation: String::new(),
            document: String::new(),
        };
        match req {
            R::Bootstrap { email, name } => {
                match self.directory.is_empty() {
                    Ok(true) => {}
                    Ok(false) => {
                        return proto::Response::Error {
                            message: "this server has accounts already; an administrator makes the next one".into(),
                        };
                    }
                    Err(e) => {
                        return proto::Response::Error {
                            message: format!("{e:#}"),
                        };
                    }
                }
                match self
                    .directory
                    .create_user(&email, &name, vec![proto::UserRole::Admin], now)
                {
                    Ok((user, token)) => {
                        let _ = self.audit.append(
                            Self::auth_actor("system", ""),
                            no_scope,
                            "user.bootstrap",
                            serde_json::json!({"user": user.id, "email": user.email}),
                            "ok",
                        );
                        proto::Response::Invite(proto::Invite {
                            user: user.id,
                            email: user.email,
                            token,
                            expires_ms: now + identity::INVITE_TTL_MS,
                        })
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }
            R::Login {
                email,
                password,
                ip,
                user_agent,
            } => match self
                .directory
                .login(&email, &password, &ip, &user_agent, now)
            {
                Ok(Ok(signed)) => {
                    let _ = self.audit.append(
                        Self::auth_actor(&signed.user.id, &ip),
                        no_scope,
                        "auth.login",
                        serde_json::json!({"email": signed.user.email, "provider": signed.user.provider}),
                        "ok",
                    );
                    proto::Response::SignedIn {
                        session: signed.session,
                        expires_ms: signed.expires_ms,
                        user: identity::info(&signed.user, None),
                    }
                }
                Ok(Err(failure)) => {
                    let _ = self.audit.append(
                        Self::auth_actor(&identity::normalise_email(&email), &ip),
                        no_scope,
                        "auth.failed",
                        serde_json::json!({"reason": failure.label()}),
                        "denied",
                    );
                    proto::Response::Error {
                        message: identity::LoginFailure::MESSAGE.into(),
                    }
                }
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },
            R::Logout { session } => {
                let who = self
                    .directory
                    .session(&session, now)
                    .ok()
                    .flatten()
                    .map(|(u, _)| u.id)
                    .unwrap_or_default();
                match self.directory.logout(&session) {
                    Ok(true) => {
                        let _ = self.audit.append(
                            Self::auth_actor(&who, ""),
                            no_scope,
                            "auth.logout",
                            serde_json::json!({}),
                            "ok",
                        );
                        proto::Response::Ok
                    }
                    Ok(false) => proto::Response::Ok,
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }
            R::SetPassword {
                token,
                password,
                ip,
                user_agent,
            } => match self
                .directory
                .set_password(&token, &password, &ip, &user_agent, now)
            {
                Ok(signed) => {
                    let _ = self.audit.append(
                        Self::auth_actor(&signed.user.id, &ip),
                        no_scope,
                        "auth.password_set",
                        serde_json::json!({"email": signed.user.email}),
                        "ok",
                    );
                    proto::Response::SignedIn {
                        session: signed.session,
                        expires_ms: signed.expires_ms,
                        user: identity::info(&signed.user, None),
                    }
                }
                Err(message) => proto::Response::Error { message },
            },
            R::InviteStatus { token } => match self.directory.invite_user(&token, now) {
                Ok(Some(user)) => proto::Response::InviteStatus {
                    valid: true,
                    email: Some(user.email),
                    name: Some(user.name),
                },
                Ok(None) => proto::Response::InviteStatus {
                    valid: false,
                    email: None,
                    name: None,
                },
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },
            _ => proto::Response::Error {
                message: "the system caller only signs users in and out".into(),
            },
        }
    }

    fn handle_inner(&mut self, req: proto::Request) -> proto::Response {
        use proto::Request as R;
        self.tick();
        match req {
            R::GetEnvironment => proto::Response::Environment(self.environment()),

            R::SetNetworkMode { mode } => {
                let applied = self.gateway.lock().unwrap().set_mode(mode);
                if applied != mode {
                    self.notice(
                        proto::NoticeLevel::Warn,
                        format!(
                            "the administrator's ceiling is `{}`; the environment stays there",
                            applied.label()
                        ),
                    );
                }
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::SetFocus { harness } => {
                self.focus = harness;
                self.emit_environment();
                proto::Response::Ok
            }

            R::SetPinned { harness, pinned } => {
                self.pinned.retain(|p| *p != harness);
                if pinned {
                    self.pinned.push(harness);
                }
                self.emit_environment();
                proto::Response::Ok
            }

            R::SetHarnessEnabled { harness, enabled } => {
                if let Some(h) = self.registry.get_mut(&harness) {
                    h.enabled = enabled;
                }
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::InstallHarness { path } => match self.install(std::path::Path::new(&path)) {
                Ok(r) => {
                    self.broadcast_environment();
                    r
                }
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::ApproveInstall { harness, token } => {
                let Some(dir) = self.pending_installs.remove(&token) else {
                    return proto::Response::Error {
                        message: format!("no install of `{harness}` is waiting on approval"),
                    };
                };
                let _ = self.audit.append(
                    self.actor(),
                    self.scope(""),
                    "harness.capabilities_approved",
                    serde_json::json!({"harness": harness}),
                    "ok",
                );
                match self.install_inner(&dir, true) {
                    Ok(r) => {
                        self.broadcast_environment();
                        r
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }

            R::UninstallHarness { harness } => {
                let removed = self.registry.remove(&harness);
                // The environment's own copy goes with it; a package used
                // where it lies (`--harnesses`) is left alone.
                if let (Some(removed), Some(root)) = (removed, self.installed_root())
                    && removed.dir.starts_with(&root)
                {
                    drop(removed);
                    let _ = std::fs::remove_dir_all(root.join(&harness));
                }
                if self.focus.as_deref() == Some(harness.as_str()) {
                    self.focus = None;
                }
                self.pinned.retain(|p| *p != harness);
                self.write_lock();
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::SendMessage { text } => {
                agent::turn(self, &text);
                proto::Response::Transcript {
                    messages: self.transcript.clone(),
                }
            }

            R::CancelTurn => {
                self.run = None;
                proto::Response::Ok
            }

            R::GetTranscript => proto::Response::Transcript {
                messages: self.transcript.clone(),
            },

            R::ListConversations => self.conversations_response(),

            R::NewConversation => {
                self.record_conversation();
                self.conversations.start(dag::now_ms());
                self.transcript.clear();
                self.emit(proto::Event::ConversationChanged {
                    current: self.conversations.current.clone(),
                });
                self.conversations_response()
            }

            R::SelectConversation { id } => {
                self.record_conversation();
                match self.conversations.select(&id) {
                    Some(c) => {
                        self.transcript = c.messages.clone();
                        self.emit(proto::Event::ConversationChanged { current: id });
                        self.conversations_response()
                    }
                    None => proto::Response::Error {
                        message: format!("no conversation `{id}`"),
                    },
                }
            }

            R::DeleteConversation { id } => {
                self.record_conversation();
                if !self.conversations.delete(&id, dag::now_ms()) {
                    return proto::Response::Error {
                        message: format!("no conversation `{id}`"),
                    };
                }
                self.transcript = self
                    .conversations
                    .current()
                    .map(|c| c.messages.clone())
                    .unwrap_or_default();
                self.emit(proto::Event::ConversationChanged {
                    current: self.conversations.current.clone(),
                });
                self.conversations_response()
            }

            R::RenameConversation { id, title } => {
                if !self.conversations.rename(&id, &title) {
                    return proto::Response::Error {
                        message: format!("no conversation `{id}`"),
                    };
                }
                self.conversations_response()
            }

            R::Approve { id, granted } => {
                let Some(p) = self.pending.remove(&id) else {
                    return proto::Response::Error {
                        message: format!("no approval `{id}` is outstanding"),
                    };
                };
                if !granted {
                    self.notice(proto::NoticeLevel::Info, "declined");
                    return proto::Response::Ok;
                }
                if id.starts_with("egress:")
                    && let Some(url) = p.params.get("url").and_then(|u| u.as_str())
                    && let Some(domain) = gateway::host_of(url)
                {
                    self.gateway.lock().unwrap().approve_domain(&domain);
                }
                // Run it as the user: they just authorised this exact call.
                let outcome = self.call_tool(&p.tool, &p.params.clone(), proto::Author::User);
                self.emit(proto::Event::ToolCallFinished {
                    id: id.clone(),
                    tool: p.tool.clone(),
                    outcome: outcome.clone(),
                });
                if p.resume_agent {
                    agent::resume(self, &p.tool, &p.params, outcome.clone());
                }
                proto::Response::ToolResult(outcome)
            }

            R::CallTool { tool, params } => {
                let outcome = self.call_tool(&tool, &params.0, proto::Author::User);
                proto::Response::ToolResult(outcome)
            }

            R::GetSurfaceModule { harness, view } => match self.registry.get(&harness) {
                Some(h) => match h.surface_module(&view) {
                    Ok(bytes) => proto::Response::SurfaceModule {
                        bytes,
                        shape_schema: proto::SHAPE_SCHEMA,
                        memory_mb: h.manifest.resources.memory_mb.surface,
                    },
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
                None => proto::Response::Error {
                    message: format!("no harness `{harness}`"),
                },
            },

            R::GetSurfaceFile {
                harness,
                view,
                path,
            } => match self.registry.get(&harness) {
                Some(h) => match h.surface_file(&view, &path) {
                    Ok((bytes, mime)) => proto::Response::SurfaceFile { bytes, mime },
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
                None => proto::Response::Error {
                    message: format!("no harness `{harness}`"),
                },
            },

            R::GetWidgetView { harness, view } => {
                let doc = self
                    .doc_for(&harness)
                    .and_then(|d| self.docs.json(&d).ok())
                    .unwrap_or(J::Null);
                let services = self.services();
                let Some(h) = self.registry.get_mut(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                if let Err(e) = Registry::ensure_runtime(h, services) {
                    return proto::Response::Error {
                        message: format!("`{harness}` could not start: {e:#}"),
                    };
                }
                match h.runtime.as_mut().map(|rt| rt.view(&view, &doc)) {
                    Some(Ok(tree)) => match widgets::parse(&tree) {
                        Ok(root) => proto::Response::WidgetView { root },
                        Err(e) => proto::Response::Error {
                            message: format!(
                                "`{harness}` returned a widget tree Core cannot read: {e:#}"
                            ),
                        },
                    },
                    Some(Err(e)) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                    None => proto::Response::Error {
                        message: format!("`{harness}` has no running logic"),
                    },
                }
            }

            R::WidgetEvent {
                harness,
                view,
                event,
            } => {
                let payload = widgets::event_to_json(&event).to_string().into_bytes();
                self.handle_inner(R::HarnessEvent {
                    harness,
                    view,
                    payload,
                })
            }

            R::HarnessEvent {
                harness,
                view,
                payload,
            } => {
                if payload.len() > 64 * 1024 {
                    return proto::Response::Error {
                        message: "a surface message may not exceed 64 KB".into(),
                    };
                }
                let Some(doc_id) = self.doc_for(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                let doc = self.docs.json(&doc_id).unwrap_or(J::Null);
                let services = self.services();
                let (result, over_budget) = {
                    let Some(h) = self.registry.get_mut(&harness) else {
                        return proto::Response::Error {
                            message: format!("no harness `{harness}`"),
                        };
                    };
                    if let Err(e) = Registry::ensure_runtime(h, services) {
                        return proto::Response::Error {
                            message: format!("`{harness}` could not start: {e:#}"),
                        };
                    }
                    let rt = h.runtime.as_mut().expect("ensured above");
                    let result = rt.event(&view, &payload, &doc);
                    (result, rt.over_budget())
                };
                if let Some(asked) = over_budget {
                    self.kill_over_budget(&harness, asked);
                }
                match result {
                    Ok((reply, doc_out)) => {
                        if let Some(next) = doc_out
                            && let Ok(changes) = self.docs.apply_json(&doc_id, &next)
                            && !changes.is_empty()
                        {
                            let snapshot = self.docs.snapshot(&doc_id).unwrap_or_default();
                            let _ = self.dag.commit(
                                &doc_id,
                                &harness,
                                &format!("surface:{view}"),
                                Json(J::Null),
                                &snapshot,
                                &changes.summary(),
                                proto::Author::User,
                                None,
                            );
                            self.doc_changed(&doc_id);
                        }
                        if !reply.is_empty() {
                            self.emit(proto::Event::HarnessMessage {
                                harness,
                                view,
                                payload: reply,
                            });
                        }
                        proto::Response::Ok
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }

            R::OpenDoc { harness } => {
                let Some(kind) = self.registry.get(&harness).map(|h| h.doc_kind()) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                let Some(doc) = self.doc_for(&harness) else {
                    return proto::Response::Error {
                        message: format!("`{harness}` has no document"),
                    };
                };
                if let Err(denied) = self.access.check(&self.identity(), &doc, Level::View) {
                    return proto::Response::Error {
                        message: denied.to_string(),
                    };
                }
                let snapshot = self.docs.snapshot(&doc).unwrap_or_default();
                proto::Response::DocOpened {
                    doc,
                    snapshot,
                    kind,
                }
            }

            R::GetDocJson { harness } => {
                let Some(doc) = self.doc_for(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                if let Err(denied) = self.access.check(&self.identity(), &doc, Level::View) {
                    return proto::Response::Error {
                        message: denied.to_string(),
                    };
                }
                let json = self.docs.json(&doc).unwrap_or(J::Null);
                proto::Response::DocJson {
                    harness,
                    doc,
                    json: Json(json),
                }
            }

            R::ProduceArtifact {
                harness,
                view,
                kind,
                name,
                mime,
                bytes,
                fields,
                summary,
            } => match self.produce_artifact(
                &harness, &view, &kind, &name, &mime, bytes, fields.0, &summary,
            ) {
                Ok(artifact) => proto::Response::Artifact(artifact),
                Err(message) => proto::Response::Error { message },
            },

            R::ListDocuments => proto::Response::Documents {
                documents: self.list_documents(),
            },

            R::GetDocBlob { doc } => match self.doc_blob(&doc) {
                Ok((name, mime, bytes)) => proto::Response::DocBlob { name, mime, bytes },
                Err(message) => proto::Response::Error { message },
            },

            R::WriteDoc {
                harness,
                view,
                doc,
                commit,
            } => {
                let Some(h) = self.registry.get(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                if h.manifest.view(&view).is_none() {
                    return proto::Response::Error {
                        message: format!("`{harness}` has no view `{view}`"),
                    };
                }
                let Some(doc_id) = self.doc_for(&harness) else {
                    return proto::Response::Error {
                        message: format!("`{harness}` has no document"),
                    };
                };
                if let Err(denied) = self.access.check(&self.identity(), &doc_id, Level::Edit) {
                    return proto::Response::Error {
                        message: denied.to_string(),
                    };
                }
                // The same path a logic component's `doc_out` takes: reconcile
                // against the Automerge document, commit only a real change.
                let changes = match self.docs.apply_json(&doc_id, &doc.0) {
                    Ok(changes) => changes,
                    Err(e) => {
                        return proto::Response::Error {
                            message: format!("the document could not be applied: {e:#}"),
                        };
                    }
                };
                if !changes.is_empty() {
                    if commit {
                        let snapshot = self.docs.snapshot(&doc_id).unwrap_or_default();
                        let _ = self.dag.commit(
                            &doc_id,
                            &harness,
                            &format!("surface:{view}"),
                            Json(J::Null),
                            &snapshot,
                            &changes.summary(),
                            proto::Author::User,
                            None,
                        );
                    }
                    self.doc_changed(&doc_id);
                }
                proto::Response::Ok
            }

            R::DocSync { doc, peer, message } => {
                // A `view` member receives sync messages but their outgoing
                // changes are rejected here, server-side.
                if let Err(denied) = self.access.check(&self.identity(), &doc, Level::Edit) {
                    return proto::Response::Error {
                        message: denied.to_string(),
                    };
                }
                // Each replica has its own sync state: two frames on one
                // board must not answer for each other.
                let mut replica = self.take_replica(&doc, &peer);
                let before = self.docs.json(&doc).unwrap_or(J::Null);
                let res = self.docs.receive_sync(&doc, &mut replica.state, &message);
                self.put_replica(&doc, &peer, replica);
                match res {
                    Ok(changed) => {
                        if changed {
                            // A replica's edit is the user's edit: a commit in the
                            // history like any other write, undoable and durable.
                            let after = self.docs.json(&doc).unwrap_or(J::Null);
                            let changes = docs::diff_changes(&before, &after);
                            let harness = self
                                .store
                                .get_document(&doc)
                                .ok()
                                .flatten()
                                .and_then(|r| r.harness)
                                .unwrap_or_else(|| "core".to_string());
                            let snapshot = self.docs.snapshot(&doc).unwrap_or_default();
                            let _ = self.dag.commit(
                                &doc,
                                &harness,
                                "surface:sync",
                                Json(J::Null),
                                &snapshot,
                                &changes.summary(),
                                proto::Author::User,
                                None,
                            );
                            // Every reader and every replica, this one included.
                            self.doc_changed(&doc);
                        } else {
                            // Nothing new for anyone else; this replica may be owed an answer.
                            self.sync_replica(&doc, &peer);
                        }
                        proto::Response::Ok
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }

            R::DocSyncEnd { doc, peer } => {
                // Ending a sync state only ever costs that replica a fresh
                // start, so it asks for no more than having one did.
                if let Some(replicas) = self.sync_states.get_mut(&doc) {
                    replicas.remove(&peer);
                    if replicas.is_empty() {
                        self.sync_states.remove(&doc);
                    }
                }
                proto::Response::Ok
            }

            R::GetHistory { limit } => match self.dag.history(limit) {
                Ok(commits) => proto::Response::History { commits },
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::Undo => {
                let doc = match self.dag.last_touched_doc() {
                    Ok(Some(d)) => d,
                    _ => {
                        return proto::Response::Error {
                            message: "nothing to undo".into(),
                        };
                    }
                };
                match self.dag.undo(&doc) {
                    Ok(r) => self.apply_revert(r),
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }

            R::Redo => {
                let doc = match self.dag.last_touched_doc() {
                    Ok(Some(d)) => d,
                    _ => {
                        return proto::Response::Error {
                            message: "nothing to redo".into(),
                        };
                    }
                };
                match self.dag.redo(&doc) {
                    Ok(r) => self.apply_revert(r),
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }

            R::DropRun { run } => match self.dag.drop_run(&run) {
                Ok(reverts) => {
                    for r in reverts {
                        self.apply_revert(r);
                    }
                    self.proposals.retain(|p| p.run != run);
                    proto::Response::Ok
                }
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::ListCatalog => {
                // The installed directory is scanned too, so a package already
                // here is marked rather than offered again.
                let mut dirs = self.cfg.catalog_dirs.clone();
                if let Some(installed) = &self.cfg.harness_dir
                    && !dirs.contains(installed)
                {
                    dirs.push(installed.clone());
                }
                proto::Response::Catalog {
                    entries: catalog::scan(&dirs, &self.registry, &self.cfg.policy),
                }
            }

            R::ListModels => {
                let models = self
                    .router
                    .read()
                    .unwrap()
                    .info()
                    .map(|m| vec![m])
                    .unwrap_or_default();
                proto::Response::Models { models }
            }

            R::SelectModel { id } => {
                // `id` is `<base url>|<model>` so one call configures both.
                let (base, model) = match id.split_once('|') {
                    Some((b, m)) => (b.to_string(), m.to_string()),
                    None => ("http://localhost:1234/v1".to_string(), id.clone()),
                };
                let worker = model::OpenAiWorker::new(&base, &model);
                self.router.write().unwrap().chat = Some(Arc::new(worker));
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::ListModelCatalog => self.model_catalog(),

            R::DownloadModel { id } => match self.download_model(&id) {
                Ok(()) => self.model_catalog(),
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::LoadModel { id } => match self.load_model(&id) {
                Ok(()) => proto::Response::Ok,
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::UnloadModel => {
                if let Some(engine) = self.engine.take() {
                    engine.stop();
                    self.trace(format!(
                        "engine: stopped llama-server for {}",
                        engine.model_id
                    ));
                    let _ = self.audit.append(
                        self.actor(),
                        self.scope("models"),
                        "model.unload",
                        serde_json::json!({"model": engine.model_id}),
                        "ok",
                    );
                }
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::ImportModel { path } => match self.models.import(std::path::Path::new(&path)) {
                Ok(m) => {
                    self.trace(format!("models: imported {} as `{}`", path, m.id));
                    self.model_catalog()
                }
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },

            R::EngineLog { lines } => proto::Response::EngineLog {
                lines: self
                    .engine
                    .as_ref()
                    .map(|e| e.log_tail(lines.clamp(1, 2000)))
                    .unwrap_or_default(),
            },

            R::PreviewContext { budget } => {
                let mut profile = self.cfg.profile.clone();
                if budget > 0 {
                    profile.context_budget_tokens = budget;
                    profile.focused_context_tokens =
                        budget.min(profile.focused_context_tokens.max(budget / 2));
                }
                let saved = std::mem::replace(&mut self.cfg.profile, profile);
                let blocks = self.context_blocks();
                self.cfg.profile = saved;

                let active = self.active_set();
                let p = prompt::build(
                    &self.cfg.profile,
                    &active,
                    &blocks,
                    Some(&self.task),
                    &self.transcript,
                );
                proto::Response::Context {
                    blocks,
                    prompt_preview: p.render(),
                }
            }

            R::GetActiveSet => proto::Response::Active(self.active_set()),

            R::GetTask => proto::Response::Task(self.task.clone()),

            R::GetLock => proto::Response::Lock {
                json: Json(self.docs.json(lock::LOCK_DOC).unwrap_or(J::Null)),
            },

            // --- identity (deployment §4; Pilot 1, Phase A) ---
            R::ListUsers => match self.require_admin("user.list") {
                Some(refused) => refused,
                None => proto::Response::Users(self.user_infos()),
            },
            R::CreateUser { email, name, roles } => match self.require_admin("user.create") {
                Some(refused) => refused,
                None => {
                    let now = dag::now_ms();
                    match self
                        .directory
                        .create_user(&email, &name, roles.clone(), now)
                    {
                        Ok((user, token)) => {
                            let _ = self.audit.append(
                                self.actor(),
                                self.scope(""),
                                "user.create",
                                serde_json::json!({"user": user.id, "email": user.email, "roles": roles}),
                                "ok",
                            );
                            proto::Response::Invite(proto::Invite {
                                user: user.id,
                                email: user.email,
                                token,
                                expires_ms: now + identity::INVITE_TTL_MS,
                            })
                        }
                        Err(e) => proto::Response::Error {
                            message: format!("{e:#}"),
                        },
                    }
                }
            },
            R::SetUserRoles { user, roles } => match self.require_admin("user.roles") {
                Some(refused) => refused,
                None => match self.directory.set_roles(&user, roles.clone()) {
                    Ok(_) => {
                        let _ = self.audit.append(
                            self.actor(),
                            self.scope(""),
                            "user.roles",
                            serde_json::json!({"user": user, "roles": roles}),
                            "ok",
                        );
                        proto::Response::Users(self.user_infos())
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
            },
            R::DisableUser { user, disabled } => match self.require_admin("user.disable") {
                Some(refused) => refused,
                None => match self.directory.set_disabled(&user, disabled) {
                    Ok(_) => {
                        let _ = self.audit.append(
                            self.actor(),
                            self.scope(""),
                            "user.disable",
                            serde_json::json!({"user": user, "disabled": disabled}),
                            "ok",
                        );
                        proto::Response::Users(self.user_infos())
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
            },
            R::ResetPassword { user } => match self.require_admin("user.reset_password") {
                Some(refused) => refused,
                None => {
                    let now = dag::now_ms();
                    match self.directory.reset_password(&user, now) {
                        Ok(token) => {
                            let email = self
                                .directory
                                .user(&user)
                                .ok()
                                .flatten()
                                .map(|u| u.email)
                                .unwrap_or_default();
                            let _ = self.audit.append(
                                self.actor(),
                                self.scope(""),
                                "user.reset_password",
                                serde_json::json!({"user": user}),
                                "ok",
                            );
                            proto::Response::Invite(proto::Invite {
                                user,
                                email,
                                token,
                                expires_ms: now + identity::INVITE_TTL_MS,
                            })
                        }
                        Err(e) => proto::Response::Error {
                            message: format!("{e:#}"),
                        },
                    }
                }
            },
            R::UnlockUser { user } => match self.require_admin("user.unlock") {
                Some(refused) => refused,
                None => match self.directory.unlock(&user) {
                    Ok(()) => {
                        let _ = self.audit.append(
                            self.actor(),
                            self.scope(""),
                            "user.unlock",
                            serde_json::json!({"user": user}),
                            "ok",
                        );
                        proto::Response::Users(self.user_infos())
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
            },
            R::RevokeSessions { user } => match self.require_admin("session.revoke") {
                Some(refused) => refused,
                None => match self.directory.revoke_sessions(&user) {
                    Ok(ended) => {
                        let _ = self.audit.append(
                            self.actor(),
                            self.scope(""),
                            "session.revoke",
                            serde_json::json!({"user": user, "ended": ended}),
                            "ok",
                        );
                        proto::Response::Users(self.user_infos())
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                },
            },
            // --- workspaces (deployment §5–6; Pilot 1, Phase A) ---
            R::ListWorkspaces => proto::Response::Workspaces(self.workspace_infos()),
            R::CreateWorkspace { name } => match self.require_admin("workspace.create") {
                Some(refused) => refused,
                None => {
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        return proto::Response::Error {
                            message: "Give the workspace a name.".into(),
                        };
                    }
                    let id = format!("ws_{}", uuid::Uuid::new_v4().simple());
                    let ws = Workspace::shared_by(&id, &name, &self.active.user, dag::now_ms());
                    if let Err(e) = self.store.put_workspace(&ws) {
                        return proto::Response::Error {
                            message: format!("{e:#}"),
                        };
                    }
                    self.access.add_workspace(ws);
                    let _ = self.audit.append(
                        self.actor(),
                        Scope {
                            workspace: id.clone(),
                            conversation: String::new(),
                            document: String::new(),
                        },
                        "workspace.create",
                        serde_json::json!({"workspace": id, "name": name}),
                        "ok",
                    );
                    proto::Response::Workspaces(self.workspace_infos())
                }
            },
            R::SetMember {
                workspace,
                principal,
                level,
            } => match self.require_workspace_owner(&workspace, "workspace.member") {
                Some(refused) => refused,
                None => {
                    let principal = acl_principal(&principal);
                    let level = acl_level(level);
                    match self.access.workspace_mut(&workspace) {
                        Some(ws) => ws.set_member(principal.clone(), level),
                        None => {
                            return proto::Response::Error {
                                message: format!("no workspace `{workspace}`"),
                            };
                        }
                    }
                    self.persist_workspace(&workspace);
                    let _ = self.audit.append(
                        self.actor(),
                        Scope {
                            workspace: workspace.clone(),
                            conversation: String::new(),
                            document: String::new(),
                        },
                        "workspace.member",
                        serde_json::json!({"principal": principal, "level": level.label()}),
                        "ok",
                    );
                    proto::Response::Workspaces(self.workspace_infos())
                }
            },
            R::RemoveMember {
                workspace,
                principal,
            } => match self.require_workspace_owner(&workspace, "workspace.member") {
                Some(refused) => refused,
                None => {
                    let principal = acl_principal(&principal);
                    let removed = self
                        .access
                        .workspace_mut(&workspace)
                        .map(|ws| ws.remove_member(&principal))
                        .unwrap_or(false);
                    if !removed {
                        return proto::Response::Error {
                            message: "That member is not in the workspace.".into(),
                        };
                    }
                    self.persist_workspace(&workspace);
                    let _ = self.audit.append(
                        self.actor(),
                        Scope {
                            workspace: workspace.clone(),
                            conversation: String::new(),
                            document: String::new(),
                        },
                        "workspace.member",
                        serde_json::json!({"principal": principal, "level": null}),
                        "ok",
                    );
                    proto::Response::Workspaces(self.workspace_infos())
                }
            },
            R::SetAgentWrites { workspace, mode } => {
                match self.require_workspace_owner(&workspace, "workspace.agent_writes") {
                    Some(refused) => refused,
                    None => {
                        let mode = match mode {
                            proto::AgentWrites::Direct => acl::AgentWrites::Direct,
                            proto::AgentWrites::Proposal => acl::AgentWrites::Proposal,
                        };
                        match self.access.workspace_mut(&workspace) {
                            Some(ws) => ws.agent_writes = mode,
                            None => {
                                return proto::Response::Error {
                                    message: format!("no workspace `{workspace}`"),
                                };
                            }
                        }
                        self.persist_workspace(&workspace);
                        let _ = self.audit.append(
                            self.actor(),
                            Scope {
                                workspace: workspace.clone(),
                                conversation: String::new(),
                                document: String::new(),
                            },
                            "workspace.agent_writes",
                            serde_json::json!({"mode": format!("{mode:?}").to_lowercase()}),
                            "ok",
                        );
                        proto::Response::Workspaces(self.workspace_infos())
                    }
                }
            }
            R::SelectWorkspace { workspace, reason } => self.select_workspace(&workspace, reason),
            R::SetDocumentAccess { doc, members } => {
                // The document's owner, or an administrator.
                let owner = self
                    .access
                    .check(&self.identity(), &doc, Level::Owner)
                    .is_ok();
                if !owner && !self.active.is_admin() {
                    let _ = self.audit.append(
                        self.actor(),
                        self.scope(&doc),
                        "document.access",
                        serde_json::json!({}),
                        "denied",
                    );
                    return proto::Response::Error {
                        message: "Only the document's owner or an administrator can change who may open it.".into(),
                    };
                }
                let acl = members.as_ref().map(|members| Acl {
                    entries: members
                        .iter()
                        .map(|m| (acl_principal(&m.principal), acl_level(m.level)))
                        .collect(),
                });
                if let Err(message) = self.access.set_document_acl(&doc, acl.clone()) {
                    return proto::Response::Error { message };
                }
                if let Ok(Some(mut record)) = self.store.get_document(&doc) {
                    record.acl = acl;
                    if let Err(e) = self.store.put_document(&doc, &record) {
                        self.trace(format!("document {doc}: access not written: {e:#}"));
                    }
                }
                let _ = self.audit.append(
                    self.actor(),
                    self.scope(&doc),
                    "document.access",
                    serde_json::json!({"members": members}),
                    "ok",
                );
                proto::Response::Ok
            }

            R::Bootstrap { .. }
            | R::Login { .. }
            | R::Logout { .. }
            | R::SetPassword { .. }
            | R::InviteStatus { .. } => proto::Response::Error {
                message: "not a client request".into(),
            },

            R::FindCapability { need } => proto::Response::Capabilities {
                hits: exposure::rank_capabilities(&self.registry, &need),
            },

            R::RunEvals { harness } => {
                // Evals run in a transcript of their own; the user's conversation
                // is neither shown them nor overwritten by them.
                let saved = std::mem::take(&mut self.transcript);
                self.evals_running = true;
                let result = evals::run(self, &harness);
                self.evals_running = false;
                self.transcript = saved;
                match result {
                    Ok(report) => proto::Response::Evals(report),
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Services handed to harness logic
// ---------------------------------------------------------------------------

struct Services {
    router: Arc<RwLock<Router>>,
    gateway: Arc<Mutex<Gateway>>,
}

impl CoreServices for Services {
    fn model_complete(&self, prompt: &str) -> std::result::Result<String, String> {
        // A harness call is a background request and, unless its model_hints
        // demand reasoning, it belongs on the utility worker (§16.1).
        let mut req = model::ChatRequest::new(prompt.to_string());
        req.class = model::RequestClass::Background;
        self.router
            .read()
            .unwrap()
            .chat(model::WorkerRole::Utility, &req)
            .map(|r| r.text)
            .map_err(|e| format!("{e:#}"))
    }

    fn model_structured(&self, schema: &str, prompt: &str) -> std::result::Result<String, String> {
        let mut req = model::ChatRequest::new(format!(
            "{prompt}\n\nReply with JSON matching this schema and nothing else:\n{schema}"
        ));
        req.class = model::RequestClass::Background;
        req.temperature = 0.0;
        let reply = self
            .router
            .read()
            .unwrap()
            .chat(model::WorkerRole::Utility, &req)
            .map_err(|e| format!("{e:#}"))?;
        // Grammar-constrained backends return valid JSON; for the rest, take the
        // first JSON object in the reply rather than handing the harness prose.
        let text = reply.text;
        match (text.find('{'), text.rfind('}')) {
            (Some(a), Some(b)) if b > a => Ok(text[a..=b].to_string()),
            _ => Err("the model did not return JSON".into()),
        }
    }

    fn model_embed(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, String> {
        self.router
            .read()
            .unwrap()
            .embed(texts)
            .map_err(|e| format!("{e:#}"))
    }

    fn docs_search(&self, _query: &str) -> std::result::Result<String, String> {
        // Retrieval is Core-owned and ACL-filtered before ranking. The index
        // itself is not built in this build — see docs/STATUS.md.
        Err("retrieval is not available in this build".into())
    }

    fn net_fetch(
        &self,
        _harness: &str,
        url: &str,
        mode: &str,
    ) -> std::result::Result<String, String> {
        let mut gw = self.gateway.lock().unwrap();
        match gw.check(url) {
            Egress::Allowed => gw
                .fetch(url, mode)
                .map(|f| f.content)
                .map_err(|e| format!("{e:#}")),
            Egress::NeedsApproval(d) => Err(format!(
                "`{d}` needs the user's approval before this environment will fetch from it"
            )),
            Egress::Denied(why) => Err(why),
        }
    }
}

fn acl_level(level: proto::AccessLevel) -> Level {
    match level {
        proto::AccessLevel::View => Level::View,
        proto::AccessLevel::Comment => Level::Comment,
        proto::AccessLevel::Edit => Level::Edit,
        proto::AccessLevel::Owner => Level::Owner,
    }
}

fn proto_level(level: Level) -> proto::AccessLevel {
    match level {
        Level::View => proto::AccessLevel::View,
        Level::Comment => proto::AccessLevel::Comment,
        Level::Edit => proto::AccessLevel::Edit,
        Level::Owner => proto::AccessLevel::Owner,
    }
}

fn acl_principal(principal: &proto::Principal) -> acl::Principal {
    match principal {
        proto::Principal::User(u) => acl::Principal::User(u.clone()),
        proto::Principal::Group(g) => acl::Principal::Group(g.clone()),
        proto::Principal::Workspace => acl::Principal::Workspace,
    }
}

fn proto_principal(principal: &acl::Principal) -> proto::Principal {
    match principal {
        acl::Principal::User(u) => proto::Principal::User(u.clone()),
        acl::Principal::Group(g) => proto::Principal::Group(g.clone()),
        acl::Principal::Workspace => proto::Principal::Workspace,
    }
}

/// Copy a directory tree. Packages are small; nothing here is clever.
fn copy_dir(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_core_reports_its_environment() {
        let core = Core::ephemeral("anna").unwrap();
        let env = core.environment();
        assert_eq!(env.user, "anna");
        assert_eq!(env.topology, proto::Topology::Personal);
        assert!(env.harnesses.is_empty());
    }

    #[test]
    fn an_unknown_tool_is_refused_with_a_pointer_to_find_capability() {
        let mut core = Core::ephemeral("anna").unwrap();
        match core.call_tool("nope.nothing", &serde_json::json!({}), proto::Author::Agent) {
            proto::ToolOutcome::Error { message } => {
                assert!(message.contains("find_capability"), "{message}");
            }
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn find_capability_is_always_callable_even_with_nothing_installed() {
        let mut core = Core::ephemeral("anna").unwrap();
        let out = core.call_tool(
            "find_capability",
            &serde_json::json!({"need": "draw a shape"}),
            proto::Author::Agent,
        );
        match out {
            proto::ToolOutcome::Ok { diff_summary, .. } => {
                assert!(diff_summary.contains("nothing installed"), "{diff_summary}");
            }
            other => panic!("expected ok, got {other:?}"),
        }
    }

    #[test]
    fn airgapped_denies_a_fetch_before_any_socket_is_opened() {
        let mut cfg = Config::personal("anna");
        cfg.gateway.mode = proto::NetworkMode::Airgapped;
        cfg.gateway.ceiling = proto::NetworkMode::Airgapped;
        let mut core = Core::new(cfg).unwrap();

        let out = core.call_tool(
            "web.fetch",
            &serde_json::json!({"url": "https://example.com"}),
            proto::Author::User,
        );
        match out {
            proto::ToolOutcome::Denied { reason } => assert!(reason.contains("airgapped")),
            other => panic!("expected denial, got {other:?}"),
        }
        // And the tool is not even in the model's view.
        assert!(
            !core
                .active_set()
                .tools
                .iter()
                .any(|t| t.name.starts_with("web."))
        );
    }

    #[test]
    fn the_user_cannot_relax_the_network_mode_past_the_ceiling() {
        let mut cfg = Config::personal("anna");
        cfg.gateway.ceiling = proto::NetworkMode::Airgapped;
        cfg.gateway.mode = proto::NetworkMode::Airgapped;
        let mut core = Core::new(cfg).unwrap();
        core.handle(proto::Request::SetNetworkMode {
            mode: proto::NetworkMode::Online,
        });
        assert_eq!(core.environment().network, proto::NetworkMode::Airgapped);
    }
}
