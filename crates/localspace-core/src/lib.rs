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
pub mod footprint;
pub mod gateway;
pub mod grammar;
pub mod lock;
pub mod manifest;
pub mod model;
pub mod models;
pub mod planner;
pub mod profile;
pub mod prompt;
pub mod registry;
pub mod runtime;
pub mod task;
pub mod tools;
pub mod transport;
pub mod widgets;

use acl::{AccessControl, Identity, Level, Workspace};
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

pub const CORE_TOOLS: &[&str] = &["find_capability", "web.search", "web.fetch", "task.plan", "task.note"];

/// How many tool calls one agent turn may make before Core stops it.
pub const MAX_AGENT_STEPS: usize = 24;

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

/// An agent run that wrote to a shared document and is awaiting apply/discard.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub run: String,
    pub docs: Vec<String>,
    pub summary: String,
    pub by: String,
}

pub struct Core {
    cfg: Config,
    registry: Registry,
    docs: DocStore,
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
    sync_states: HashMap<String, automerge::sync::State>,

    events: Option<engine::EventSink>,
    /// Every conversation; `transcript` is the current one's messages.
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
        let dag = match &cfg.data_dir {
            Some(dir) => Dag::open(&dir.join("db").join("dag.redb"))?,
            None => Dag::in_memory()?,
        };
        let audit = match &cfg.data_dir {
            Some(dir) => AuditLog::open(&dir.join("audit").join("audit.jsonl"))?,
            None => AuditLog::in_memory(),
        };

        let gateway = Arc::new(Mutex::new(Gateway::new(cfg.gateway.clone())));
        let router = Arc::new(RwLock::new(Router::default()));

        let mut access = AccessControl::new();
        access.add_workspace(Workspace::personal(&cfg.user));
        let workspace = format!("ws_{}", cfg.user);

        let models_store = cfg
            .data_dir
            .as_ref()
            .map(|d| d.join("models"))
            .unwrap_or_else(|| std::env::temp_dir().join("localspace").join("models"));
        let models = models::Catalog::load(cfg.models_dir.as_deref(), &models_store);

        let conversations = conversations::Store::load(cfg.data_dir.as_deref(), dag::now_ms());
        let transcript = conversations.current().map(|c| c.messages.clone()).unwrap_or_default();

        let mut core = Core {
            conversations,
            evals_running: false,
            models,
            downloads: Arc::new(Mutex::new(HashMap::new())),
            engine: None,
            registry: Registry::new(),
            docs: DocStore::new(),
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
            task: proto::Task::default(),
            workspace,
            sync_states: HashMap::new(),
            events: None,
            next_approval: 1,
            cfg,
        };

        if let Some(dir) = core.cfg.harness_dir.clone() {
            core.load_harnesses(&dir);
        }
        // What the user installed from a catalog, kept under the data
        // directory (v2 §1: only chat ships in the box; the rest is installed).
        if let Some(root) = core.installed_root() {
            if root.is_dir() {
                core.load_harnesses(&root);
            }
        }
        Ok(core)
    }

    pub fn set_event_sink(&mut self, sink: Box<dyn Fn(proto::Event) + Send + Sync>) {
        self.events = Some(Arc::from(sink));
    }

    /// The sink for Core's own threads — downloads, the engine supervisor —
    /// which report the same way requests do.
    fn sink(&self) -> engine::EventSink {
        self.events.clone().unwrap_or_else(|| Arc::new(|_| {}))
    }

    fn emit(&self, ev: proto::Event) {
        if let Some(sink) = &self.events {
            sink(ev);
        }
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

    pub fn provider_cache_hit_rate(&self) -> f32 {
        self.providers.hit_rate()
    }

    fn identity(&self) -> Identity {
        Identity::user(&self.cfg.user)
    }

    fn actor(&self) -> Actor {
        Actor {
            user: self.cfg.user.clone(),
            role: "member".into(),
            ..Default::default()
        }
    }

    fn scope(&self, document: &str) -> Scope {
        Scope {
            workspace: self.workspace.clone(),
            conversation: "c_1".into(),
            document: document.to_string(),
        }
    }

    // -- harnesses ----------------------------------------------------------

    pub fn load_harnesses(&mut self, dir: &std::path::Path) {
        let services = self.services();
        let failures = self.registry.load_dir(dir, &self.cfg.policy.clone(), services);
        for (name, err) in failures {
            self.notice(
                proto::NoticeLevel::Error,
                format!("harness `{name}` failed to install: {err:#}"),
            );
        }
        let ids: Vec<(String, proto::DocKind, String)> = self
            .registry
            .iter()
            .map(|h| (h.doc_id.clone(), h.doc_kind(), h.manifest.harness.title.clone()))
            .collect();
        for (doc_id, kind, title) in ids {
            self.docs.ensure(&doc_id, kind);
            let ws = self.workspace.clone();
            self.access.add_document(&doc_id, &ws, &title, None);
            self.restore_from_dag(&doc_id);
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

        // Dependencies (spec §17.2) are resolved against what is installed and
        // what the catalog offers: one version per package per environment, and
        // interface dependencies bound to any provider. What is missing is
        // installed first, in dependency order; a conflict names both dependents.
        if !staged.manifest.dependencies.is_empty() {
            let mut candidates = catalog::candidates(&self.catalog_dirs_all(), &self.registry);
            if !candidates.iter().any(|c| c.id == staged.manifest.harness.id) {
                candidates.push(deps::Candidate {
                    id: staged.manifest.harness.id.clone(),
                    version: staged.manifest.harness.version.clone(),
                    kind: staged.manifest.package.kind,
                    provides: staged.manifest.provides.interfaces.clone(),
                    deps: staged.manifest.dependencies(),
                    path: dir.to_path_buf(),
                    installed: false,
                });
            }
            let resolution = deps::resolve(&staged.manifest.harness.id, &candidates)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            for (id, version) in &resolution.install {
                if *id == staged.manifest.harness.id {
                    continue;
                }
                let Some(dep) = candidates.iter().find(|c| c.id == *id && c.version == *version)
                else {
                    continue;
                };
                let path = dep.path.clone();
                self.trace(format!("installing dependency `{id}` {version} first"));
                match self.install_inner(&path, false)? {
                    proto::Response::Ok => {}
                    proto::Response::InstallPrompt { harness, .. } => anyhow::bail!(
                        "dependency `{harness}` widens capabilities; approve it before installing `{}`",
                        staged.manifest.harness.id
                    ),
                    other => anyhow::bail!("installing dependency `{id}` failed: {other:?}"),
                }
            }
            for (interface, provider) in &resolution.bindings {
                self.trace(format!("`{interface}` is provided by `{provider}`"));
            }
        }

        // An update that widens capabilities does not auto-install: it re-prompts
        // with a diff, and only proceeds once the user has answered.
        if !capabilities_approved {
            if let Some(existing) = self.registry.get(staged.id()) {
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
        let doc_id = staged.doc_id.clone();
        let kind = staged.doc_kind();
        let title = staged.manifest.harness.title.clone();
        let id = staged.id().to_string();
        let staged_is_harness = staged.manifest.package.kind.is_harness();

        self.docs.ensure(&doc_id, kind);
        let ws = self.workspace.clone();
        self.access.add_document(&doc_id, &ws, &title, None);
        // A package installed again finds its document where its history
        // left it; the DAG is the durable store, not the package.
        self.restore_from_dag(&doc_id);
        self.registry.insert(staged);

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

    // -- conversations (v2 §8) ------------------------------------------------

    /// Write the transcript into the current conversation and save.
    pub(crate) fn record_conversation(&mut self) {
        if self.evals_running {
            return;
        }
        self.conversations.record(&self.transcript, dag::now_ms());
        self.save_conversations();
    }

    fn save_conversations(&self) {
        if let Err(e) = self.conversations.save() {
            self.trace(format!("conversations: not saved: {e:#}"));
        }
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
        self.models.download(id, self.downloads.clone(), self.sink())?;
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
        let binary = engine::find_binary(self.cfg.llama_server.as_deref(), self.cfg.data_dir.as_deref())
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
            None => vec!["-c".into(), context_len.to_string(), "-ngl".into(), "999".into()],
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
            user: self.cfg.user.clone(),
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
            machine: self.cfg.machine.describe(),
            profile: self.cfg.profile.name.clone(),
            engine,
        }
    }

    fn broadcast_environment(&self) {
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
            if let Some(h) = self.registry.get_mut(&id) {
                if h.enabled && h.manifest.contributes.context_provider {
                    if let Err(e) = Registry::ensure_runtime(h, services.clone()) {
                        self.trace(format!("`{id}` could not start for its context provider: {e:#}"));
                    }
                }
            }
        }

        context::assemble(
            &mut self.registry,
            &mut self.docs,
            &mut self.providers,
            &self.cfg.profile,
            focus.as_deref(),
            &also,
        )
    }

    // -- the single mutation path -------------------------------------------

    /// Every tool call — from the agent, from the Client, from an eval — comes
    /// through here. Permission check, confirm gate, schema validation, run,
    /// commit, short result.
    pub fn call_tool(&mut self, tool: &str, params: &J, author: proto::Author) -> proto::ToolOutcome {
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

        let (decl, doc_id, enabled) = {
            let h = self.registry.get(&owner).expect("owner exists");
            (
                h.tools.get(tool).cloned(),
                h.doc_id.clone(),
                h.enabled,
            )
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
                message: out.error.unwrap_or_else(|| "the harness reported a failure".into()),
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
                                self.push_doc_patch(doc_id);
                            }
                            Err(e) => {
                                return proto::ToolOutcome::Error {
                                    message: format!("the change could not be committed: {e:#}"),
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return proto::ToolOutcome::Error {
                        message: format!("the harness returned a document Core could not apply: {e:#}"),
                    }
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
        let mut diff_summary = diff_summary;
        if let Some(spec) = out.result.get("artifact").cloned() {
            match self.register_artifact(owner, doc_id, commit_id.clone(), &spec, &diff_summary) {
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

    fn push_doc_patch(&mut self, doc_id: &str) {
        let state = self
            .sync_states
            .entry(doc_id.to_string())
            .or_insert_with(automerge::sync::State::new);
        if let Some(message) = self.docs.sync_message(doc_id, state) {
            self.emit(proto::Event::DocPatch {
                doc: doc_id.to_string(),
                message,
            });
        }
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
                let mode = params.get("mode").and_then(|m| m.as_str()).unwrap_or("text");
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
        self.push_doc_patch(&revert.doc);
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
        if let Some(installed) = &self.cfg.harness_dir {
            if !dirs.contains(installed) {
                dirs.push(installed.clone());
            }
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
    fn resolve_handoff(&mut self, owner: &str, params: &J) -> std::result::Result<Vec<(String, String)>, String> {
        let Some(id) = params.get("artifact").and_then(|a| a.as_str()) else {
            return Ok(Vec::new());
        };
        let Some(art) = self.task.artifacts.iter().find(|a| a.id == id).cloned() else {
            let known: Vec<&str> = self.task.artifacts.iter().map(|a| a.id.as_str()).collect();
            return Err(if known.is_empty() {
                format!("`{id}` is not an artifact in this task; nothing has been produced yet")
            } else {
                format!("`{id}` is not an artifact in this task; the ledger has {}", known.join(", "))
            });
        };

        let accepts = self
            .registry
            .get(owner)
            .map(|h| h.manifest.contributes.accepts.clone())
            .unwrap_or_default();
        if !accepts.iter().any(|k| *k == art.kind) {
            let takers = task::who_accepts(&self.registry, &art.kind);
            return Err(if takers.is_empty() {
                format!("`{owner}` does not accept {}, and nothing installed does", art.kind)
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
                    _ => return Err(format!("the version {} is pinned to is missing from the DAG", art.id)),
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
        if !produces.iter().any(|k| *k == kind) {
            return Err(format!(
                "`{owner}` does not declare that it produces {kind}; add it to [contributes] produces"
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
            self.trace(format!("unloaded `{id}` after idle_unload; its document stays"));
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

    pub fn handle(&mut self, req: proto::Request) -> proto::Response {
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
                self.broadcast_environment();
                proto::Response::Ok
            }

            R::SetPinned { harness, pinned } => {
                self.pinned.retain(|p| *p != harness);
                if pinned {
                    self.pinned.push(harness);
                }
                self.broadcast_environment();
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
                if let (Some(removed), Some(root)) = (removed, self.installed_root()) {
                    if removed.dir.starts_with(&root) {
                        drop(removed);
                        let _ = std::fs::remove_dir_all(root.join(&harness));
                    }
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
                self.save_conversations();
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
                        self.save_conversations();
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
                self.save_conversations();
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
                self.save_conversations();
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
                if id.starts_with("egress:") {
                    if let Some(url) = p.params.get("url").and_then(|u| u.as_str()) {
                        if let Some(domain) = gateway::host_of(url) {
                            self.gateway.lock().unwrap().approve_domain(&domain);
                        }
                    }
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

            R::GetSurfaceFile { harness, view, path } => match self.registry.get(&harness) {
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
                    .registry
                    .get(&harness)
                    .map(|h| h.doc_id.clone())
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
                            message: format!("`{harness}` returned a widget tree Core cannot read: {e:#}"),
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
                self.handle(R::HarnessEvent {
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
                let doc_id = match self.registry.get(&harness) {
                    Some(h) => h.doc_id.clone(),
                    None => {
                        return proto::Response::Error {
                            message: format!("no harness `{harness}`"),
                        }
                    }
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
                        if let Some(next) = doc_out {
                            if let Ok(changes) = self.docs.apply_json(&doc_id, &next) {
                                if !changes.is_empty() {
                                    let snapshot =
                                        self.docs.snapshot(&doc_id).unwrap_or_default();
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
                                    self.push_doc_patch(&doc_id);
                                }
                            }
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
                let Some(h) = self.registry.get(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                let doc = h.doc_id.clone();
                let kind = h.doc_kind();
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
                let Some(h) = self.registry.get(&harness) else {
                    return proto::Response::Error {
                        message: format!("no harness `{harness}`"),
                    };
                };
                let doc = h.doc_id.clone();
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
                let doc_id = h.doc_id.clone();
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
                        }
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
                    self.push_doc_patch(&doc_id);
                }
                proto::Response::Ok
            }

            R::DocSync { doc, message } => {
                // A `view` member receives sync messages but their outgoing
                // changes are rejected here, server-side.
                if let Err(denied) = self.access.check(&self.identity(), &doc, Level::Edit) {
                    return proto::Response::Error {
                        message: denied.to_string(),
                    };
                }
                let mut state = self
                    .sync_states
                    .remove(&doc)
                    .unwrap_or_else(automerge::sync::State::new);
                let res = self.docs.receive_sync(&doc, &mut state, &message);
                self.sync_states.insert(doc.clone(), state);
                match res {
                    Ok(()) => {
                        self.push_doc_patch(&doc);
                        proto::Response::Ok
                    }
                    Err(e) => proto::Response::Error {
                        message: format!("{e:#}"),
                    },
                }
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
                        }
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
                        }
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
                if let Some(installed) = &self.cfg.harness_dir {
                    if !dirs.contains(installed) {
                        dirs.push(installed.clone());
                    }
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
                    self.trace(format!("engine: stopped llama-server for {}", engine.model_id));
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
                    profile.focused_context_tokens = budget.min(profile.focused_context_tokens.max(budget / 2));
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
            Egress::Allowed => gw.fetch(url, mode).map(|f| f.content).map_err(|e| format!("{e:#}")),
            Egress::NeedsApproval(d) => Err(format!(
                "`{d}` needs the user's approval before this environment will fetch from it"
            )),
            Egress::Denied(why) => Err(why),
        }
    }
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
        assert!(!core
            .active_set()
            .tools
            .iter()
            .any(|t| t.name.starts_with("web.")));
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
