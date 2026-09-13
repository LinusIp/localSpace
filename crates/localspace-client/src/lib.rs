#![deny(unsafe_code)]
//! The localSpace Client. Same code natively and in the browser; no IO except
//! through `Backend`.
//!
//! The Client holds only view state — open panels, scroll positions, selection.
//! Documents, history, conversation and environment all live in Core, so signing
//! in from a different browser shows the same environment.

pub mod perf;
pub mod surface;
pub mod theme;
pub mod ui;
pub mod widgets;

use localspace_proto as proto;
use std::collections::HashMap;
use std::sync::Arc;

pub use localspace_proto::Request;

/// What the Client needs from a transport. Mirrors `localspace_core::transport`,
/// restated here so the Client crate does not depend on Core.
/// Called from the transport whenever a response or event is waiting.
pub type Wake = Box<dyn Fn() + Send + Sync>;

pub trait Backend: Send + Sync {
    fn request(&self, req: proto::Request) -> u64;
    fn poll(&self) -> Vec<Incoming>;
    /// Install a callback fired when something arrives, so an idle Client can
    /// sleep until there is something to draw. Transports that cannot wake
    /// anyone keep the default, and the Client polls instead.
    fn set_wake(&self, _wake: Wake) {}
    fn connected(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub enum Incoming {
    Response { id: u64, response: proto::Response },
    Event(proto::Event),
}

/// What the icon rail switches between.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum RailTab {
    #[default]
    Canvas,
    Agent,
    Tools,
    Models,
    Data,
    History,
    Library,
    Marketplace,
    Settings,
    Help,
}

#[derive(Default)]
struct OpenSurface {
    runner: Option<surface::SurfaceRunner>,
    error: Option<String>,
    /// Messages waiting to be handed to the surface on its next frame.
    inbox: Vec<Vec<u8>>,
    /// Whether it ever painted, so a death reads differently from a failure to
    /// start.
    ever_ran: bool,
    /// Restarts after it broke its memory budget: the first is automatic.
    restarts: surface::Restarts,
}

pub struct App {
    backend: Arc<dyn Backend>,
    env: Option<proto::EnvironmentState>,
    transcript: Vec<proto::ChatMessage>,
    trace: Vec<String>,
    notices: Vec<(proto::NoticeLevel, String)>,
    approvals: Vec<(String, proto::ApprovalKind, String)>,
    history: Vec<proto::Commit>,
    active: Option<proto::ActiveSet>,
    catalog: Vec<proto::CatalogEntry>,
    /// The current run's ledger (spec §18.1), as Core last sent it.
    task: Option<proto::Task>,
    perf: perf::Meter,
    widget_views: HashMap<(String, String), proto::Widget>,
    surfaces: HashMap<(String, String), OpenSurface>,
    /// Surfaces compiling on their own thread: a cold JIT of a 4 MB module
    /// takes seconds, and the window keeps drawing meanwhile.
    #[cfg(not(target_arch = "wasm32"))]
    surface_loads: Vec<SurfaceLoad>,
    /// To ask for a repaint from a loader thread when its surface is ready.
    ctx: egui::Context,
    docs: HashMap<String, String>,

    input: String,
    rail: RailTab,
    details_open: bool,
    zoom: i32,
    dirty_since_commit: bool,
    /// The in-flight surface edit, if any. Cleared when Core answers.
    pending_doc_write: Option<u64>,
    model_endpoint: String,
    model_name: String,
    busy: bool,
    pending_env_refresh: bool,
    last_widget_request: Option<(String, String)>,
    last_surface_request: Option<(String, String)>,
}

/// A surface compiling on its own thread, and where its runner will arrive.
#[cfg(not(target_arch = "wasm32"))]
type SurfaceLoad = (
    (String, String),
    std::sync::mpsc::Receiver<Result<surface::SurfaceRunner, String>>,
);

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, backend: Arc<dyn Backend>) -> App {
        theme::apply(&cc.egui_ctx);

        // Idle means idle. The Client repaints when the user does something or
        // when Core has something to say — never on a timer, never in a loop.
        let ctx = cc.egui_ctx.clone();
        backend.set_wake(Box::new(move || ctx.request_repaint()));
        perf::log("Core ready, window created, App constructed");
        let app = App {
            backend,
            env: None,
            transcript: Vec::new(),
            trace: Vec::new(),
            notices: Vec::new(),
            approvals: Vec::new(),
            history: Vec::new(),
            active: None,
            catalog: Vec::new(),
            task: None,
            perf: perf::Meter::new(),
            widget_views: HashMap::new(),
            surfaces: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            surface_loads: Vec::new(),
            ctx: cc.egui_ctx.clone(),
            docs: HashMap::new(),
            input: String::new(),
            rail: RailTab::Canvas,
            details_open: true,
            zoom: 100,
            dirty_since_commit: false,
            pending_doc_write: None,
            model_endpoint: "http://localhost:1234/v1".into(),
            model_name: String::new(),
            busy: false,
            pending_env_refresh: false,
            last_widget_request: None,
            last_surface_request: None,
        };
        app.backend.request(proto::Request::GetEnvironment);
        app.backend.request(proto::Request::GetActiveSet);
        app.backend
            .request(proto::Request::GetHistory { limit: 50 });
        app.backend.request(proto::Request::ListCatalog);
        app
    }

    fn send(&self, req: proto::Request) {
        self.backend.request(req);
    }

    /// Push a command straight into the focused surface's inbox. Used by the
    /// zoom and fit controls, which act on the surface's own viewport.
    fn surface_command(&mut self, value: serde_json::Value) {
        let payload = value.to_string().into_bytes();
        for surface in self.surfaces.values_mut() {
            surface.inbox.push(payload.clone());
            if let Some(r) = surface.runner.as_mut() {
                r.mark_dirty();
            }
        }
    }

    fn set_zoom(&mut self, percent: i32) {
        self.zoom = percent.clamp(25, 400);
        let zoom = self.zoom;
        self.surface_command(serde_json::json!({"kind": "zoom", "value": zoom as f32 / 100.0}));
    }

    /// Run one of the focused harness's front-door tools directly.
    ///
    /// Same path as the agent's: `CallTool` goes through Core's permission check,
    /// confirmation gate and DAG commit like any other call.
    fn run_front_door(&mut self, tool: &str) {
        let params = match tool.rsplit('.').next() {
            // A tool whose only required parameter is text gets a placeholder the
            // user can then edit on the canvas.
            Some("add_sticky") => serde_json::json!({"text": "New note"}),
            _ => serde_json::json!({}),
        };
        self.send(proto::Request::CallTool {
            tool: tool.to_string(),
            params: proto::Json(params),
        });
    }

    fn drain(&mut self, ctx: &egui::Context) {
        for msg in self.backend.poll() {
            match msg {
                Incoming::Response { id, response } => self.apply_response(id, response),
                Incoming::Event(ev) => self.apply_event(ev),
            }
            ctx.request_repaint();
        }
        if self.pending_env_refresh {
            self.pending_env_refresh = false;
            self.send(proto::Request::GetActiveSet);
        }
    }

    fn apply_response(&mut self, id: u64, res: proto::Response) {
        // A surface edit is "unsaved" only until Core has answered for it.
        if self.pending_doc_write == Some(id) {
            self.pending_doc_write = None;
            self.dirty_since_commit = false;
        }
        use proto::Response as R;
        match res {
            R::Environment(env) => {
                self.env = Some(env);
                self.pending_env_refresh = true;
            }
            R::Transcript { messages } => {
                self.transcript = messages;
                self.busy = false;
            }
            R::History { commits } => self.history = commits,
            R::Catalog { entries } => self.catalog = entries,
            R::Task(task) => self.task = Some(task),
            R::Active(set) => self.active = Some(set),
            R::WidgetView { root } => {
                // The Client asked for exactly one view at a time.
                if let Some(key) = self.last_widget_request.clone() {
                    self.widget_views.insert(key, root);
                }
            }
            R::SurfaceModule {
                bytes,
                shape_schema,
                memory_mb,
            } => {
                if let Some(key) = self.last_surface_request.clone() {
                    if shape_schema != proto::SHAPE_SCHEMA {
                        let entry = self.surfaces.entry(key).or_default();
                        entry.error = Some(format!(
                            "surface was built against shape schema {shape_schema}; this Client speaks {}",
                            proto::SHAPE_SCHEMA
                        ));
                    } else {
                        self.surfaces.entry(key.clone()).or_default();
                        self.load_surface(key, bytes, memory_mb);
                    }
                }
            }
            R::DocOpened { doc, .. } => {
                self.docs.entry(doc).or_default();
            }
            R::DocJson { harness, json, .. } => {
                // Hand the projection to every surface this harness owns. Only a
                // changed document reaches the guest; an unchanged one is dropped
                // by the runner, which is what keeps an idle canvas at zero cost.
                let text = json.to_string();
                for ((h, _), surface) in self.surfaces.iter_mut() {
                    if *h == harness
                        && let Some(r) = surface.runner.as_mut()
                    {
                        r.set_doc(text.clone());
                    }
                }
            }
            R::ToolResult(outcome) => {
                self.busy = false;
                if let proto::ToolOutcome::Error { message } = outcome {
                    self.notices.push((proto::NoticeLevel::Error, message));
                }
            }
            R::Error { message } => {
                self.busy = false;
                self.notices.push((proto::NoticeLevel::Error, message));
            }
            R::Evals(report) => {
                self.notices.push((
                    proto::NoticeLevel::Info,
                    format!(
                        "{}: {}/{} eval cases passed on {}",
                        report.harness, report.passed, report.total, report.model
                    ),
                ));
            }
            R::InstallPrompt {
                harness,
                token,
                diff,
                native_reason,
            } => {
                // Widening capabilities does not auto-install: it asks, showing
                // exactly what would be granted, and any native reason verbatim.
                let mut prompt = format!(
                    "Installing `{harness}` would grant it:
  {}",
                    diff.join(
                        "
  "
                    )
                );
                if let Some(reason) = native_reason {
                    prompt.push_str(&format!(
                        "

It runs as a native process. Its stated reason: {reason}"
                    ));
                }
                self.approvals.push((
                    format!("install:{harness}:{token}"),
                    proto::ApprovalKind::Capability,
                    prompt,
                ));
            }
            _ => {}
        }
    }

    fn apply_event(&mut self, ev: proto::Event) {
        use proto::Event as E;
        match ev {
            E::EnvironmentChanged(env) => {
                self.env = Some(env);
                self.pending_env_refresh = true;
            }
            // Something shared moved: ask for this user's view again.
            E::EnvironmentOutdated => {
                self.send(proto::Request::GetEnvironment);
            }
            E::AssistantDelta { .. } | E::AssistantDone => {
                self.send(proto::Request::GetTranscript);
            }
            E::ToolCallStarted { tool, .. } => self.trace.push(format!("-> {tool}")),
            E::ToolCallFinished { tool, outcome, .. } => {
                let line = match &outcome {
                    proto::ToolOutcome::Ok { diff_summary, .. } => {
                        format!("<- {tool}: {diff_summary}")
                    }
                    proto::ToolOutcome::Denied { reason } => format!("<- {tool} denied: {reason}"),
                    proto::ToolOutcome::Error { message } => format!("<- {tool} error: {message}"),
                    proto::ToolOutcome::AwaitingConfirm { .. } => {
                        format!("<- {tool}: waiting for you")
                    }
                    proto::ToolOutcome::Queued { job } => format!("<- {tool} queued as {job}"),
                };
                self.trace.push(line);
                self.send(proto::Request::GetHistory { limit: 50 });
                // A tool call moves focus, which changes the active set.
                self.send(proto::Request::GetActiveSet);
            }
            E::TaskChanged(task) => self.task = Some(task),
            // The egui client holds no replica: a sync message is for a frame's.
            E::DocPatch { .. } => {}
            // Nor a board of its own: who is on one is the web shell's to show.
            E::Presence { .. } => {}
            E::DocChanged { doc } => {
                // Core is authoritative. Ask it for the projection every open
                // surface reads, rather than trying to keep a second copy in step.
                self.docs.entry(doc).or_default();
                let mut asked: Vec<String> = Vec::new();
                for (harness, _) in self.surfaces.keys() {
                    if !asked.contains(harness) {
                        asked.push(harness.clone());
                    }
                }
                for harness in asked {
                    self.send(proto::Request::GetDocJson { harness });
                }
            }
            E::HarnessMessage {
                harness,
                view,
                payload,
            } => {
                if let Some(s) = self.surfaces.get_mut(&(harness, view)) {
                    s.inbox.push(payload);
                }
            }
            E::WidgetViewChanged {
                harness,
                view,
                root,
            } => {
                self.widget_views.insert((harness, view), root);
            }
            E::ApprovalRequest { id, kind, prompt } => self.approvals.push((id, kind, prompt)),
            E::SurfaceSlow { harness, view, ms } => {
                self.trace.push(format!("{harness}/{view} took {ms:.1} ms"))
            }
            E::Notice { level, text } => self.notices.push((level, text)),
            // The web client shows these; the egui client, kept until parity, does not.
            E::ModelProgress { .. } | E::EngineChanged(_) | E::ConversationChanged { .. } => {}
            E::TraceLine { text } => {
                self.trace.push(text);
                if self.trace.len() > 500 {
                    self.trace.drain(..100);
                }
            }
        }
    }
}

// The Client asks for one view or module at a time; these remember which.
impl App {
    fn request_widget_view(&mut self, harness: &str, view: &str) {
        self.last_widget_request = Some((harness.to_string(), view.to_string()));
        self.send(proto::Request::GetWidgetView {
            harness: harness.to_string(),
            view: view.to_string(),
        });
    }

    fn request_surface(&mut self, harness: &str, view: &str) {
        self.last_surface_request = Some((harness.to_string(), view.to_string()));
        self.send(proto::Request::GetSurfaceModule {
            harness: harness.to_string(),
            view: view.to_string(),
        });
    }
}

// Surfaces are compiled off the UI thread. The panel says "Loading surface…"
// until the runner arrives; the window never stops drawing for a JIT.
impl App {
    fn load_surface(&mut self, key: (String, String), bytes: Vec<u8>, memory_mb: u32) {
        let name = format!("{}/{}", key.0, key.1);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let compile = std::time::Instant::now();
                let result = surface::SurfaceRunner::load(&name, &bytes, memory_mb)
                    .map_err(|e| format!("{e:#}"));
                if result.is_ok() {
                    perf::log(format!(
                        "surface {name} ready in {:.0} ms ({} KB of wasm), off the UI thread",
                        compile.elapsed().as_secs_f32() * 1000.0,
                        bytes.len() / 1024
                    ));
                }
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            self.surface_loads.push((key, rx));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let result = surface::SurfaceRunner::load(&name, &bytes, memory_mb)
                .map_err(|e| format!("{e:#}"));
            self.finish_surface_load(key, result);
        }
    }

    fn finish_surface_load(
        &mut self,
        key: (String, String),
        result: Result<surface::SurfaceRunner, String>,
    ) {
        let harness = key.0.clone();
        let loaded = {
            let entry = self.surfaces.entry(key).or_default();
            match result {
                Ok(r) => {
                    entry.runner = Some(r);
                    entry.error = None;
                    true
                }
                Err(e) => {
                    entry.error = Some(e);
                    false
                }
            }
        };
        // A surface starts empty until Core hands it the document it is meant
        // to be showing.
        if loaded {
            self.send(proto::Request::GetDocJson { harness });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_surface_loads(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let mut done = Vec::new();
        self.surface_loads
            .retain_mut(|(key, rx)| match rx.try_recv() {
                Ok(result) => {
                    done.push((key.clone(), result));
                    false
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => {
                    done.push((key.clone(), Err("the surface loader thread died".into())));
                    false
                }
            });
        for (key, result) in done {
            self.finish_surface_load(key, result);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(not(target_arch = "wasm32"))]
        self.poll_surface_loads();

        // What the previous frame cost, and whether any surface ran its guest.
        let (guest_total, guest_last) = self
            .surfaces
            .values()
            .filter_map(|s| s.runner.as_ref())
            .fold((0u64, 0f32), |(t, l), r| {
                (t + r.guest_frames, l.max(r.stats.last_ms))
            });
        self.perf
            .frame(frame.info().cpu_usage, guest_total, guest_last);
        if perf::enabled() {
            // egui records the file and line of every request_repaint call
            // that led to this frame, so an idle loop names its own author.
            self.perf
                .note_causes(ctx.repaint_causes().iter().map(|c| c.to_string()));
            // Which GPU is presenting. On a laptop with two, the wrong one —
            // or one whose driver is broken — caps the frame rate regardless
            // of what a frame costs.
            if !self.perf.gpu_reported {
                self.perf.gpu_reported = true;
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(rs) = frame.wgpu_render_state() {
                    let info = rs.adapter.get_info();
                    perf::log(format!(
                        "gpu: {} · {:?} · {:?} · driver {} {}",
                        info.name, info.device_type, info.backend, info.driver, info.driver_info
                    ));
                }
            }
            if perf::spin() {
                ctx.request_repaint();
                // A visible counter, so a screen capture can count the frames
                // that actually reached the screen, not just the ones drawn.
                ctx.debug_painter().text(
                    egui::pos2(24.0, 60.0),
                    egui::Align2::LEFT_TOP,
                    format!("frame {}", self.perf.total_frames),
                    egui::FontId::monospace(30.0),
                    egui::Color32::RED,
                );
            }
        }

        self.drain(&ctx);

        // Chrome first, so the canvas gets whatever is left.
        self.top_bar(ui);
        self.status_bar(ui);
        self.rail(ui);
        self.details(ui);
        self.central(ui);

        // Floating, so neither is ever hidden behind a panel.
        self.notices_strip(&ctx);
        self.approvals_window(&ctx);
    }
}
