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
pub mod dag;
pub mod docs;
pub mod evals;
pub mod exposure;
pub mod footprint;
pub mod gateway;
pub mod grammar;
pub mod manifest;
pub mod model;
pub mod planner;
pub mod profile;
pub mod prompt;
pub mod registry;
pub mod runtime;
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

pub const CORE_TOOLS: &[&str] = &["find_capability", "web.search", "web.fetch"];

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
    workspace: String,
    sync_states: HashMap<String, automerge::sync::State>,

    events: Option<Box<dyn Fn(proto::Event) + Send>>,
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

        let mut core = Core {
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
            transcript: Vec::new(),
            pending: HashMap::new(),
            pending_installs: HashMap::new(),
            proposals: Vec::new(),
            run: None,
            workspace,
            sync_states: HashMap::new(),
            events: None,
            next_approval: 1,
            cfg,
        };

        if let Some(dir) = core.cfg.harness_dir.clone() {
            core.load_harnesses(&dir);
        }
        Ok(core)
    }

    pub fn set_event_sink(&mut self, sink: Box<dyn Fn(proto::Event) + Send>) {
        self.events = Some(sink);
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
            self.focus = self.registry.iter().next().map(|h| h.id().to_string());
        }
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

    fn install_inner(
        &mut self,
        dir: &std::path::Path,
        capabilities_approved: bool,
    ) -> Result<proto::Response> {
        let policy = self.cfg.policy.clone();
        let mut staged = Registry::stage(dir, &policy)?;

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

        self.docs.ensure(&doc_id, kind);
        let ws = self.workspace.clone();
        self.access.add_document(&doc_id, &ws, &title, None);
        self.registry.insert(staged);

        let _ = self.audit.append(
            self.actor(),
            self.scope(&doc_id),
            "harness.install",
            serde_json::json!({"harness": id}),
            "ok",
        );
        if self.focus.is_none() {
            self.focus = Some(id);
        }
        Ok(proto::Response::Ok)
    }

    // -- environment --------------------------------------------------------

    pub fn environment(&self) -> proto::EnvironmentState {
        let gw = self.gateway.lock().unwrap();
        let model = self.router.read().unwrap().info();
        let engine = match &model {
            Some(m) => proto::EngineState {
                running: true,
                detail: m.backend.clone(),
            },
            None => proto::EngineState {
                running: false,
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
            let out = rt.call(tool, params, &doc);
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
                self.registry.remove(&harness);
                if self.focus.as_deref() == Some(harness.as_str()) {
                    self.focus = None;
                }
                self.pinned.retain(|p| *p != harness);
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
                let p = prompt::build(&self.cfg.profile, &active, &blocks, &self.transcript);
                proto::Response::Context {
                    blocks,
                    prompt_preview: p.render(),
                }
            }

            R::GetActiveSet => proto::Response::Active(self.active_set()),

            R::FindCapability { need } => proto::Response::Capabilities {
                hits: exposure::rank_capabilities(&self.registry, &need),
            },

            R::RunEvals { harness } => match evals::run(self, &harness) {
                Ok(report) => proto::Response::Evals(report),
                Err(e) => proto::Response::Error {
                    message: format!("{e:#}"),
                },
            },
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
