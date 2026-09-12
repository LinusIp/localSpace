#![deny(unsafe_code)]
//! `localspace serve` as a library: Core, the HTTP and WebSocket API, and the
//! web client bundle (architecture v2 §2–5).
//!
//! The desktop shell embeds this same router on loopback, so the browser in
//! organisation mode and the webview on a workstation reach Core through one
//! code path. Everything a client sends is a `proto::Request`; everything it
//! receives is a `proto::Response` or a `proto::Event`; JSON on the wire.

pub mod api;
pub mod auth;
pub mod openapi;
pub mod session;
pub mod surfaces;
pub mod ws;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use localspace_core::{Caller, Config, Core};
use localspace_proto as proto;
use session::Session;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub bind: String,
    pub harnesses: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub registry: Vec<PathBuf>,
    pub web_root: Option<PathBuf>,
    pub allow_below_floor: bool,
    /// Personal mode: one user, Core configured as on a workstation, the
    /// token generated at start. Otherwise organisation mode.
    pub personal: bool,
    /// The user in personal mode.
    pub user: String,
    /// A fixed token, or `None` to generate one.
    pub token: Option<String>,
    /// Mark the session cookie `Secure` (behind TLS).
    pub secure_cookies: bool,
    /// An organisation's own model catalog directory, on top of the built-in one.
    pub models: Option<PathBuf>,
    /// `llama-server`, when it is not under `<data>/engines` or on PATH.
    pub llama_server: Option<PathBuf>,
    /// Where harness surfaces live, one origin each: a host pattern with
    /// `{slug}` for the harness. `h-{slug}.localhost` resolves to loopback in
    /// every browser without DNS; a deployment on its own domain sets a
    /// wildcard it owns, such as `h-{slug}.apps.example.com`.
    pub surface_hosts: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            bind: "127.0.0.1:8443".into(),
            harnesses: None,
            data: None,
            registry: Vec::new(),
            web_root: Some(PathBuf::from("web/dist")),
            allow_below_floor: false,
            personal: false,
            user: whoami(),
            token: None,
            secure_cookies: false,
            models: Some(PathBuf::from("models")).filter(|p| p.exists()),
            llama_server: None,
            surface_hosts: surfaces::DEFAULT_HOSTS.into(),
        }
    }
}

pub fn whoami() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "local".into())
}

pub struct Server {
    /// The one Core of this server, made on first use.
    core: Mutex<Option<Arc<Session>>>,
    /// Held while the Core is being created, so two first requests do not
    /// race to open one database.
    creating: tokio::sync::Mutex<()>,
    pub cfg: ServerConfig,
    pub started: std::time::Instant,
    pub connections: Mutex<u64>,
    /// The one token this server accepts. Personal mode hands it to the
    /// shell; organisation mode reads it from the command line until OIDC.
    pub token: String,
    /// Open surfaces: the grant behind each harness-origin cookie.
    pub grants: Mutex<HashMap<String, surfaces::Grant>>,
}

impl Server {
    pub fn new(mut cfg: ServerConfig) -> Arc<Server> {
        let token = cfg
            .token
            .take()
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        Arc::new(Server {
            core: Mutex::new(None),
            creating: tokio::sync::Mutex::new(()),
            cfg,
            started: std::time::Instant::now(),
            connections: Mutex::new(0),
            token,
            grants: Mutex::new(HashMap::new()),
        })
    }

    /// Whom a valid token stands for. Personal mode: the one user, the
    /// admin of their own machine. Organisation mode: the operator, an
    /// admin, until Phase A's identity gives every user a session of their
    /// own — then the session names the user, the roles and the address.
    pub fn caller_for(&self, _token: &str) -> Caller {
        if self.cfg.personal {
            Caller::local(&self.cfg.user)
        } else {
            self.caller_named("operator")
        }
    }

    /// The caller a grant or a probe names by user.
    pub fn caller_named(&self, user: &str) -> Caller {
        if self.cfg.personal {
            return Caller::local(&self.cfg.user);
        }
        Caller {
            user: user.to_string(),
            session: String::new(),
            ip: String::new(),
            roles: vec![proto::UserRole::Admin],
            groups: Vec::new(),
        }
    }

    /// The user a valid token stands for.
    pub fn user_for(&self, token: &str) -> String {
        self.caller_for(token).user
    }

    fn core_config(&self) -> Config {
        let mut cfg = if self.cfg.personal {
            Config::personal(&self.cfg.user)
        } else {
            Config::organisation("operator")
        };
        cfg.harness_dir = self.cfg.harnesses.clone();
        cfg.catalog_dirs = self.cfg.registry.clone();
        cfg.models_dir = self.cfg.models.clone();
        cfg.llama_server = self.cfg.llama_server.clone();
        cfg.data_dir = self.cfg.data.clone();
        cfg
    }

    /// The one Core, created on first use. Creating it loads the
    /// environment — harnesses, documents, the database — so it runs off
    /// the async threads, once. A failure is the caller's error, not a
    /// poisoned lock for everyone after.
    pub async fn core(&self) -> anyhow::Result<Arc<Session>> {
        if let Some(existing) = self.core.lock().unwrap().clone() {
            return Ok(existing);
        }
        let _creating = self.creating.lock().await;
        if let Some(existing) = self.core.lock().unwrap().clone() {
            return Ok(existing);
        }
        let cfg = self.core_config();
        let core = tokio::task::spawn_blocking(move || Core::new(cfg)).await??;
        let session = Session::spawn(core);
        *self.core.lock().unwrap() = Some(session.clone());
        Ok(session)
    }

    /// Whom the readiness probe asks as: the operator, or the one user.
    pub fn health_caller(&self) -> Caller {
        self.caller_named("operator")
    }

    /// Users who have opened an event stream since the server started.
    pub fn user_count(&self) -> usize {
        self.core
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.user_count())
            .unwrap_or(0)
    }
}

/// The whole HTTP surface.
pub fn router(server: Arc<Server>) -> Router {
    let protected = Router::new()
        .route("/me", get(api::me))
        .route("/request", post(api::request))
        .route("/environment", get(api::environment))
        .route("/catalog", get(api::catalog))
        .route("/history", get(api::history))
        .route("/task", get(api::task))
        .route("/active-set", get(api::active_set))
        .route("/lock", get(api::lock))
        .route("/docs/{harness}", get(api::doc))
        .route("/documents", get(api::documents))
        .route("/documents/{id}/content", get(api::document_content))
        .route(
            "/artifacts",
            post(api::produce_artifact).layer(axum::extract::DefaultBodyLimit::max(
                localspace_core::MAX_ARTIFACT_BYTES,
            )),
        )
        .route("/surfaces", post(surfaces::open))
        .route("/logout", post(auth::logout))
        .route_layer(axum::middleware::from_fn_with_state(
            server.clone(),
            auth::require,
        ));

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws::legacy_upgrade))
        .route("/ws/json", get(ws::json_upgrade))
        .route("/api/v1/openapi.json", get(openapi_json))
        .route("/api/v1/login", post(auth::login))
        .nest("/api/v1", protected)
        .with_state(server.clone());

    match &server.cfg.web_root {
        Some(dir) if dir.join("index.html").exists() => {
            // The bundle, with its own routes falling back to index.html.
            let index = tower_http::services::ServeFile::new(dir.join("index.html"));
            app = app.fallback_service(
                tower_http::services::ServeDir::new(dir).not_found_service(index),
            );
            tracing::info!("serving the web client from {}", dir.display());
        }
        _ => {
            app = app.fallback(get(placeholder));
        }
    }
    // Harness origins are answered before anything else sees the request.
    app.layer(axum::middleware::from_fn_with_state(
        server,
        surfaces::route,
    ))
}

/// A running server: where it listens, and the token it accepts.
pub struct Running {
    pub addr: SocketAddr,
    pub token: String,
    pub server: Arc<Server>,
    pub task: tokio::task::JoinHandle<()>,
}

/// Bind and serve in the background. The desktop shell uses this with
/// `bind = "127.0.0.1:0"` and opens its window on the address returned.
pub async fn start(cfg: ServerConfig) -> anyhow::Result<Running> {
    let server = Server::new(cfg);
    let app = router(server.clone());
    let listener = tokio::net::TcpListener::bind(&server.cfg.bind).await?;
    let addr = listener.local_addr()?;
    if let Some(data) = &server.cfg.data {
        // For a browser on the same machine: the token, readable by the user.
        std::fs::create_dir_all(data).ok();
        std::fs::write(data.join("token"), &server.token).ok();
    }
    tracing::info!("localspace serve listening on http://{addr}");
    let task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("server stopped: {e}");
        }
    });
    Ok(Running {
        addr,
        token: server.token.clone(),
        server,
        task,
    })
}

async fn readyz(State(server): State<Arc<Server>>) -> impl IntoResponse {
    // Ready means: the Core is up and its environment answers.
    let session = match server.core().await {
        Ok(session) => session,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("core could not start: {e:#}"),
            );
        }
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        session.call_as(&server.health_caller(), proto::Request::GetEnvironment),
    )
    .await
    {
        Ok(Ok(_)) => (StatusCode::OK, "ready".to_string()),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            "core not answering".to_string(),
        ),
    }
}

async fn metrics(State(server): State<Arc<Server>>) -> impl IntoResponse {
    let sessions = server.user_count();
    let connections = *server.connections.lock().unwrap();
    let uptime = server.started.elapsed().as_secs();
    let footprint = localspace_core::footprint::Footprint::measure();
    let private_bytes = footprint.private_bytes.unwrap_or(0);
    format!(
        "# HELP localspace_uptime_seconds Seconds since this process started.\n\
         # TYPE localspace_uptime_seconds counter\n\
         localspace_uptime_seconds {uptime}\n\
         # HELP localspace_sessions Users who have opened an event stream since the start.\n\
         # TYPE localspace_sessions gauge\n\
         localspace_sessions {sessions}\n\
         # HELP localspace_ws_connections WebSocket connections accepted.\n\
         # TYPE localspace_ws_connections counter\n\
         localspace_ws_connections {connections}\n\
         # HELP localspace_process_private_bytes Private resident memory of this process.\n\
         # TYPE localspace_process_private_bytes gauge\n\
         localspace_process_private_bytes {private_bytes}\n\
         # HELP localspace_app_budget_bytes The spec's idle budget for Core.\n\
         # TYPE localspace_app_budget_bytes gauge\n\
         localspace_app_budget_bytes {}\n",
        localspace_core::footprint::APP_BUDGET_BYTES
    )
}

async fn openapi_json() -> impl IntoResponse {
    axum::Json(openapi::document().clone())
}

async fn placeholder() -> impl IntoResponse {
    Html(
        "<!doctype html><meta charset=utf-8><title>localSpace</title>\
         <style>body{font:14px system-ui;margin:3rem auto;max-width:44rem;line-height:1.6}\
         code{background:#eee;padding:.1rem .3rem;border-radius:3px}</style>\
         <h1>localSpace</h1>\
         <p>Core is running and the API is live under <code>/api/v1</code> and at <code>/ws/json</code>.</p>\
         <p>The web client has not been built into this deployment. Build it with \
         <code>npm run build</code> in <code>web/</code> and start the server with \
         <code>--web web/dist</code>.</p>\
         <p><a href=\"/metrics\">/metrics</a> · <a href=\"/readyz\">/readyz</a> · \
         <a href=\"/api/v1/openapi.json\">/api/v1/openapi.json</a></p>",
    )
}
