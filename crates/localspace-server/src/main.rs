//! `localspace serve` — Core, the HTTP/WS API, and the wasm Client bundle.
//!
//! Same Core, same `proto`, same harnesses as the desktop binary; only the
//! transport differs. Everything a browser Client sends is a `proto::Request` and
//! everything it receives is a `proto::Response` or `proto::Event`, encoded with
//! postcard over a binary WebSocket.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use localspace_core::transport::{Backend, InProcess, Incoming};
use localspace_core::{profile, Config, Core};
use localspace_proto as proto;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One environment per user. v1 is single-tenant and vertically scaled: one Core
/// process, one environment per session, no cross-org data path in the binary.
struct Server {
    sessions: Mutex<HashMap<String, Arc<InProcess>>>,
    cfg: ServerConfig,
    started: std::time::Instant,
    connections: Mutex<u64>,
}

#[derive(Clone)]
struct ServerConfig {
    bind: String,
    harnesses: Option<PathBuf>,
    data: Option<PathBuf>,
    web_root: Option<PathBuf>,
    allow_below_floor: bool,
}

impl Server {
    /// Sessions are keyed by a token the browser supplies. Identity itself comes
    /// from the organisation's IdP; that leg is not built in this configuration —
    /// see docs/STATUS.md — so a token is taken at face value here.
    fn session(&self, key: &str) -> Arc<InProcess> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(existing) = sessions.get(key) {
            return existing.clone();
        }
        let mut cfg = Config::organisation(key);
        cfg.harness_dir = self.cfg.harnesses.clone();
        cfg.data_dir = self
            .cfg
            .data
            .as_ref()
            .map(|d| d.join("sessions").join(sanitize(key)));
        let core = Core::new(cfg).expect("creating a Core for this session");
        let backend = Arc::new(InProcess::spawn(core));
        sessions.insert(key.to_string(), backend.clone());
        backend
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localspace=info,tower_http=info".into()),
        )
        .init();

    let cfg = parse_args();
    let machine = profile::Machine::detect();
    let tier = machine.tier();

    tracing::info!("machine: {}", machine.describe());
    if !tier.may_serve() {
        if tier.team_mode() {
            tracing::warn!(
                "team mode: this is a workstation profile, not a server. Expect one interactive \
                 stream and graceful queuing beyond that."
            );
        } else if !cfg.allow_below_floor {
            eprintln!(
                "localspace serve refuses to start below the supported floor.\n\
                 Detected: {}\n\
                 Run `localspace doctor` for the details, or pass --allow-below-floor \
                 (logged, and shown permanently in the console).",
                machine.describe()
            );
            std::process::exit(2);
        } else {
            tracing::warn!(
                "running below the supported hardware floor because --allow-below-floor was passed"
            );
        }
    }

    let server = Arc::new(Server {
        sessions: Mutex::new(HashMap::new()),
        started: std::time::Instant::now(),
        connections: Mutex::new(0),
        cfg: cfg.clone(),
    });

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_upgrade))
        .route("/api/v1/openapi.json", get(openapi))
        .with_state(server.clone());

    // The wasm Client bundle, when one has been built.
    match &cfg.web_root {
        Some(dir) if dir.exists() => {
            app = app.fallback_service(tower_http::services::ServeDir::new(dir));
            tracing::info!("serving the web Client from {}", dir.display());
        }
        _ => {
            app = app.fallback(get(placeholder));
        }
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("localspace serve listening on {}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_args() -> ServerConfig {
    let mut cfg = ServerConfig {
        bind: "127.0.0.1:8443".into(),
        harnesses: None,
        data: None,
        web_root: Some(PathBuf::from("dist")),
        allow_below_floor: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bind" => cfg.bind = it.next().unwrap_or(cfg.bind),
            "--harnesses" => cfg.harnesses = it.next().map(PathBuf::from),
            "--data" => cfg.data = it.next().map(PathBuf::from),
            "--web" => cfg.web_root = it.next().map(PathBuf::from),
            "--allow-below-floor" => cfg.allow_below_floor = true,
            other => eprintln!("ignoring unknown argument `{other}`"),
        }
    }
    cfg
}

async fn readyz(State(server): State<Arc<Server>>) -> impl IntoResponse {
    // Ready means: a Core can be created and its environment answers.
    let backend = server.session("healthcheck");
    let id = backend.request(proto::Request::GetEnvironment);
    for _ in 0..200 {
        for msg in backend.poll() {
            if let Incoming::Response { id: got, .. } = msg {
                if got == id {
                    return (StatusCode::OK, "ready");
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    (StatusCode::SERVICE_UNAVAILABLE, "core not answering")
}

async fn metrics(State(server): State<Arc<Server>>) -> impl IntoResponse {
    let sessions = server.sessions.lock().unwrap().len();
    let connections = *server.connections.lock().unwrap();
    let uptime = server.started.elapsed().as_secs();
    format!(
        "# HELP localspace_uptime_seconds Seconds since this process started.\n\
         # TYPE localspace_uptime_seconds counter\n\
         localspace_uptime_seconds {uptime}\n\
         # HELP localspace_sessions Environments currently held in memory.\n\
         # TYPE localspace_sessions gauge\n\
         localspace_sessions {sessions}\n\
         # HELP localspace_ws_connections WebSocket connections accepted.\n\
         # TYPE localspace_ws_connections counter\n\
         localspace_ws_connections {connections}\n"
    )
}

async fn openapi() -> impl IntoResponse {
    // `proto` is the API. This document points at it rather than restating it in
    // a second, drift-prone form.
    axum::Json(serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "localSpace",
            "version": env!("CARGO_PKG_VERSION"),
            "description":
                "The API is localspace-proto: Request, Response and Event, postcard-encoded \
                 over the binary WebSocket at /ws. There is no second REST surface, so the \
                 desktop and server modes cannot diverge."
        },
        "paths": {
            "/ws": {"get": {"summary": "Client stream (postcard over binary WebSocket)"}},
            "/healthz": {"get": {"summary": "process up"}},
            "/readyz": {"get": {"summary": "Core answering"}},
            "/metrics": {"get": {"summary": "Prometheus metrics"}}
        }
    }))
}

async fn placeholder() -> impl IntoResponse {
    Html(
        "<!doctype html><meta charset=utf-8><title>localSpace</title>\
         <style>body{font:14px system-ui;margin:3rem auto;max-width:44rem;line-height:1.6}\
         code{background:#eee;padding:.1rem .3rem;border-radius:3px}</style>\
         <h1>localSpace</h1>\
         <p>Core is running and the API is live at <code>/ws</code>.</p>\
         <p>The browser Client bundle has not been built into this deployment. Build it with \
         <code>trunk build --release</code> and start the server with <code>--web dist</code>, \
         or use the desktop Client: <code>localspace</code>.</p>\
         <p><a href=\"/metrics\">/metrics</a> · <a href=\"/readyz\">/readyz</a> · \
         <a href=\"/api/v1/openapi.json\">/api/v1/openapi.json</a></p>",
    )
}

async fn ws_upgrade(
    State(server): State<Arc<Server>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    *server.connections.lock().unwrap() += 1;
    ws.on_upgrade(move |socket| serve_socket(server, socket))
}

async fn serve_socket(server: Arc<Server>, mut socket: WebSocket) {
    // One session per connection in this configuration. With an IdP in front, the
    // key is the authenticated subject and the same environment follows the user
    // between browsers.
    let key = format!("s_{}", uuid::Uuid::new_v4());
    let backend = server.session(&key);

    let hello = proto::Envelope {
        id: 0,
        body: proto::Body::Event(proto::Event::Notice {
            level: proto::NoticeLevel::Info,
            text: format!("connected to localSpace {}", env!("CARGO_PKG_VERSION")),
        }),
    };
    if let Ok(bytes) = proto::encode(&hello) {
        let _ = socket.send(Message::Binary(bytes.into())).await;
    }

    let mut ticker = tokio::time::interval(Duration::from_millis(8));
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Binary(bytes))) => {
                        match proto::decode(&bytes) {
                            Ok(env) => {
                                if let proto::Body::Request(req) = env.body {
                                    backend.request(req);
                                }
                            }
                            Err(e) => tracing::warn!("undecodable frame: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::info!("socket closed: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                for msg in backend.poll() {
                    let env = match msg {
                        Incoming::Response { id, response } => proto::Envelope {
                            id,
                            body: proto::Body::Response(response),
                        },
                        Incoming::Event(event) => proto::Envelope {
                            id: 0,
                            body: proto::Body::Event(event),
                        },
                    };
                    let Ok(bytes) = proto::encode(&env) else { continue };
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    server.sessions.lock().unwrap().remove(&key);
}
