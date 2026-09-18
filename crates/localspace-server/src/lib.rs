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
pub mod net;
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

pub use net::Cidr;

/// How TLS reaches this server (deployment §3.3 `tls`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tls {
    /// None: right on loopback, refused anywhere else without `--insecure`.
    None,
    /// A reverse proxy terminates TLS and forwards plain HTTP; its addresses
    /// are in `trusted_proxies`, and their `X-Forwarded-For` is believed.
    BehindProxy,
    /// A certificate and key of the server's own. Not built in this release:
    /// `preflight` refuses it rather than ignore it.
    Native { cert: PathBuf, key: PathBuf },
}

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
    /// How long a signed-in session lives (deployment §3.3 `session_ttl`).
    pub session_ttl_ms: u64,
    /// The address users open, for the links the server prints; the bind
    /// address when unset.
    pub public_url: Option<String>,
    /// How TLS reaches this server (deployment §3.3 `tls`).
    pub tls: Tls,
    /// The proxies whose `X-Forwarded-For` is believed (deployment §3.3
    /// `trusted_proxies`); the connection's address otherwise.
    pub trusted_proxies: Vec<Cidr>,
    /// Serve plaintext off loopback anyway (`--insecure`). Logged loudly.
    pub insecure: bool,
    /// The largest upload or artifact (deployment §3.3 `max_upload_mb`).
    pub max_upload_mb: u64,
    /// `[organisation] name`: the sign-in page, the tab title and the
    /// invitations show it. Absent until the settings name one.
    pub organisation: Option<String>,
    /// The network policy Core starts with (deployment §3.3 `[network]`).
    pub gateway: localspace_core::gateway::GatewayConfig,
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
            session_ttl_ms: 12 * 60 * 60 * 1000,
            public_url: None,
            tls: Tls::None,
            trusted_proxies: Vec::new(),
            insecure: false,
            max_upload_mb: 200,
            organisation: None,
            gateway: localspace_core::gateway::GatewayConfig::default(),
        }
    }
}

/// The checks before a socket opens (Pilot 1, the answers of 2026-09-13).
/// `Err` is the one message the operator reads; `Ok` carries what to log
/// loudly. Nothing here is skipped by any caller: `start` runs it.
pub fn preflight(cfg: &ServerConfig) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    if let Tls::Native { .. } = cfg.tls {
        return Err(
            "TLS termination is not built in yet; put localSpace behind a reverse proxy \
                    and set [server] tls = \"behind-proxy\" and trusted_proxies."
                .into(),
        );
    }
    if cfg.tls == Tls::BehindProxy {
        if cfg.trusted_proxies.is_empty() {
            return Err(
                "[server] tls = \"behind-proxy\" needs trusted_proxies: the proxies whose \
                        X-Forwarded-For is believed. Without them anyone could claim any address."
                    .into(),
            );
        }
        if cfg.public_url.is_none() {
            return Err(
                "[server] public_url is not set: the address users open, which a server \
                        behind a proxy cannot know by itself."
                    .into(),
            );
        }
    }
    if !net::binds_loopback(&cfg.bind) {
        if cfg.public_url.is_none() {
            return Err(format!(
                "[server] public_url is not set: the address users open, needed for the links the \
                 server prints when it listens on {}.",
                cfg.bind
            ));
        }
        if cfg.tls != Tls::BehindProxy {
            if !cfg.insecure {
                return Err(format!(
                    "localspace serve refuses to serve plaintext off loopback: bind is {}. \
                     Terminate TLS in a reverse proxy and set [server] tls = \"behind-proxy\" and \
                     trusted_proxies, or pass --insecure to serve plaintext anyway (logged).",
                    cfg.bind
                ));
            }
            warnings.push(format!(
                "INSECURE: serving plaintext on {} because --insecure was passed. Every password \
                 and document crosses the network unencrypted. Not for a company's documents.",
                cfg.bind
            ));
        }
    }
    Ok(warnings)
}

fn upload_limit(cfg: &ServerConfig) -> usize {
    usize::try_from(cfg.max_upload_mb)
        .unwrap_or(usize::MAX)
        .saturating_mul(1024 * 1024)
}

/// Core's configuration from the server's: the one mapping, shared with the
/// commands that build a Core beside the server (`bench`, `evals`, `call`).
pub fn core_config(cfg: &ServerConfig) -> Config {
    let mut core = if cfg.personal {
        Config::personal(&cfg.user)
    } else {
        Config::organisation("operator")
    };
    core.harness_dir = cfg.harnesses.clone();
    core.catalog_dirs = cfg.registry.clone();
    core.models_dir = cfg.models.clone();
    core.llama_server = cfg.llama_server.clone();
    core.data_dir = cfg.data.clone();
    core.session_ttl_ms = cfg.session_ttl_ms;
    core.gateway = cfg.gateway.clone();
    core
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
        // A site users open over https gets cookies the browser sends only
        // over https, whatever the caller said.
        if cfg
            .public_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
        {
            cfg.secure_cookies = true;
        }
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

    /// The caller a surface grant names by user: the files of a view are
    /// served as that user, with no role to speak of. Personal mode has one
    /// user whatever the grant says.
    pub fn caller_named(&self, user: &str) -> Caller {
        if self.cfg.personal {
            return Caller::local(&self.cfg.user);
        }
        Caller {
            user: user.to_string(),
            session: String::new(),
            ip: String::new(),
            roles: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Who presented a token or session; see `auth::authenticate`.
    pub async fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
        query_token: Option<&str>,
        ip: &str,
    ) -> Option<Caller> {
        auth::authenticate(self, headers, query_token, ip).await
    }

    fn core_config(&self) -> Config {
        core_config(&self.cfg)
    }

    /// The first administrator's one-time link, minted by the server itself
    /// while there are no accounts at all (the fourth answer of 2026-09-13).
    /// `None` when accounts exist.
    pub async fn mint_first_admin(&self) -> anyhow::Result<Option<proto::Invite>> {
        let session = self.core().await?;
        match session
            .call_as(&Caller::system(), proto::Request::Bootstrap)
            .await?
        {
            proto::Response::Invite(invite) => Ok(Some(invite)),
            proto::Response::Error { message } if message.contains("already") => Ok(None),
            proto::Response::Error { message } => anyhow::bail!("{message}"),
            other => anyhow::bail!("unexpected answer to bootstrap: {other:?}"),
        }
    }

    /// The link a one-time token becomes, on the address users open.
    pub fn invite_link(&self, token: &str, addr: &SocketAddr) -> String {
        let base = self
            .cfg
            .public_url
            .clone()
            .unwrap_or_else(|| format!("http://{addr}"));
        format!("{}/invite/{token}", base.trim_end_matches('/'))
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
    let signing_in = Router::new()
        .route("/login", post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/auth/mode", get(auth::mode))
        .route("/auth/login", post(auth::login_with_password))
        .route("/auth/invite/{token}", get(auth::invite_status))
        .route("/auth/set-password", post(auth::set_password));

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
            post(api::produce_artifact).layer(axum::extract::DefaultBodyLimit::max(upload_limit(
                &server.cfg,
            ))),
        )
        .route("/surfaces", post(surfaces::open))
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
        .nest("/api/v1", signing_in)
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

impl Running {
    /// Stop what would outlive this process: the model's sidecar. The desktop
    /// app calls this as it exits, because a process that exits runs no
    /// destructors and a `llama-server` left behind keeps the graphics
    /// memory. Asked as the machine's own user, so the audit log says who.
    pub async fn shutdown(&self) {
        let existing = self.server.core.lock().unwrap().clone();
        let Some(session) = existing else { return };
        let caller = Caller::local(&self.server.cfg.user);
        if let Err(e) = session.call_as(&caller, proto::Request::UnloadModel).await {
            tracing::warn!("the model was not unloaded on the way out: {e}");
        }
    }
}

/// Bind and serve in the background. The desktop shell uses this with
/// `bind = "127.0.0.1:0"` and opens its window on the address returned.
pub async fn start(cfg: ServerConfig) -> anyhow::Result<Running> {
    for warning in preflight(&cfg).map_err(|refusal| anyhow::anyhow!("{refusal}"))? {
        tracing::warn!("{warning}");
    }
    let server = Server::new(cfg);
    let app = router(server.clone());
    let listener = tokio::net::TcpListener::bind(&server.cfg.bind).await?;
    let addr = listener.local_addr()?;
    if let Some(data) = &server.cfg.data {
        // For a browser on the same machine: the token, readable by the user.
        std::fs::create_dir_all(data).ok();
        std::fs::write(data.join("token"), &server.token).ok();
    }
    if !server.cfg.personal {
        // The first administrator's link: minted here while there is nobody,
        // logged, and written where only the service user reads it; the
        // file goes once the link is used, or here when it is stale.
        match server.mint_first_admin().await? {
            Some(invite) => {
                let link = server.invite_link(&invite.token, &addr);
                let written = server
                    .cfg
                    .data
                    .as_deref()
                    .map(|dir| localspace_core::identity::write_first_admin_link(dir, &link));
                match written {
                    Some(Ok(path)) => tracing::info!(
                        "first administrator: open {link} within 24 hours (also in {})",
                        path.display()
                    ),
                    Some(Err(e)) => tracing::warn!(
                        "first administrator: open {link} within 24 hours; the file could not \
                         be written: {e:#}"
                    ),
                    None => tracing::info!("first administrator: open {link} within 24 hours"),
                }
            }
            None => {
                if let Some(dir) = server.cfg.data.as_deref() {
                    localspace_core::identity::remove_first_admin_link(dir);
                }
            }
        }
    }
    tracing::info!("localspace serve listening on http://{addr}");
    let task = tokio::spawn(async move {
        if let Err(e) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
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
    // Ready means: the Core is up, and its database answers. In personal
    // mode the one user's environment is asked for as well; an organisation
    // server has no user to ask as until someone signs in.
    let session = match server.core().await {
        Ok(session) => session,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("core could not start: {e:#}"),
            );
        }
    };
    if !server.cfg.personal {
        return match session.directory.users() {
            Ok(_) => (StatusCode::OK, "ready".to_string()),
            Err(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("database not answering: {e:#}"),
            ),
        };
    }
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        session.call_as(
            &Caller::local(&server.cfg.user),
            proto::Request::GetEnvironment,
        ),
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
