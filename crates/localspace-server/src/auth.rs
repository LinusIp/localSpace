//! Who is asking (deployment §4.1; Pilot 1, Phase A). In personal mode the
//! one token, generated at start and handed to the shell, stands for the one
//! user. In organisation mode a session does: made by `POST /api/v1/auth/login`
//! with an email and a password, or by a one-time link's `set-password`,
//! carried in an httpOnly cookie or as a bearer token for the API, read on
//! every request from the database — so a session an admin ended is over
//! at the next request, tab or no tab.

use crate::Server;
use axum::Json;
use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use localspace_core::Caller;
use localspace_proto as proto;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

pub const COOKIE: &str = "ls_session";
/// The most a sign-in body may be.
const BODY_LIMIT: usize = 16 * 1024;

/// The token or session id the request presents, wherever it put it.
pub fn presented(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        return Some(token.trim().to_string());
    }
    for cookie in headers.get_all(header::COOKIE) {
        let Ok(text) = cookie.to_str() else { continue };
        for pair in text.split(';') {
            let pair = pair.trim();
            if let Some(value) = pair.strip_prefix(COOKIE).and_then(|r| r.strip_prefix('=')) {
                return Some(value.to_string());
            }
        }
    }
    query_token.map(|t| t.to_string())
}

/// Personal mode's token, compared as hashes, so the comparison takes the
/// same time whatever matches.
pub fn matches(server: &Server, token: &str) -> bool {
    blake3::hash(token.as_bytes()) == blake3::hash(server.token.as_bytes())
}

#[derive(Deserialize, Default)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// The address a request carries: the connection's, or behind a trusted
/// proxy (deployment §3.3 `trusted_proxies`) the client `X-Forwarded-For`
/// names. The tests' requests have no connection and get none.
pub fn client_ip(
    server: &Server,
    extensions: &axum::http::Extensions,
    headers: &HeaderMap,
) -> String {
    let peer = extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let forwarded = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    crate::net::client_ip(peer, forwarded, &server.cfg.trusted_proxies)
        .map(|ip| ip.to_string())
        .unwrap_or_default()
}

fn user_agent(headers: &HeaderMap) -> String {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Who presented this: the one user for personal mode's token, the
/// session's user in organisation mode, or nobody.
pub async fn authenticate(
    server: &Server,
    headers: &HeaderMap,
    query_token: Option<&str>,
    ip: &str,
) -> Option<Caller> {
    let presented = presented(headers, query_token)?;
    if server.cfg.personal {
        return matches(server, &presented).then(|| Caller::local(&server.cfg.user));
    }
    let session = server.core().await.ok()?;
    let (user, record) = session
        .directory
        .session(&presented, localspace_core::dag::now_ms())
        .ok()
        .flatten()?;
    Some(Caller {
        user: user.id,
        // The audit names the session by a prefix of its hash, never the id.
        session: localspace_core::identity::key_of(&presented)[..12].to_string(),
        ip: if ip.is_empty() {
            record.ip
        } else {
            ip.to_string()
        },
        roles: user.roles,
        groups: Vec::new(),
    })
}

/// Middleware: everything under `/api/v1` except signing in needs a caller,
/// which the handlers then find in the request's extensions.
pub async fn require(
    State(server): State<Arc<Server>>,
    Query(query): Query<TokenQuery>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let ip = client_ip(&server, request.extensions(), request.headers());
    match authenticate(&server, request.headers(), query.token.as_deref(), &ip).await {
        Some(caller) => {
            request.extensions_mut().insert(caller);
            next.run(request).await
        }
        None => unauthorised("sign in first"),
    }
}

fn unauthorised(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": message})),
    )
        .into_response()
}

fn session_cookie(server: &Server, value: &str, max_age_secs: u64) -> String {
    format!(
        "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{}",
        if server.cfg.secure_cookies {
            "; Secure"
        } else {
            ""
        }
    )
}

#[derive(Deserialize)]
pub struct Login {
    pub token: String,
}

/// `POST /api/v1/login` with personal mode's token sets the session cookie.
/// An organisation server signs in with an email and a password instead.
pub async fn login(State(server): State<Arc<Server>>, Json(login): Json<Login>) -> Response {
    if !server.cfg.personal {
        return unauthorised("This server signs in with an email and a password.");
    }
    if !matches(&server, &login.token) {
        return unauthorised("that token is not this server's");
    }
    (
        [(
            header::SET_COOKIE,
            session_cookie(&server, &login.token, 365 * 24 * 60 * 60),
        )],
        Json(serde_json::json!({"user": server.cfg.user})),
    )
        .into_response()
}

/// `POST /api/v1/logout` ends the session and clears the cookie.
pub async fn logout(
    State(server): State<Arc<Server>>,
    Query(q): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !server.cfg.personal
        && let Some(id) = presented(&headers, q.token.as_deref())
        && let Ok(session) = server.core().await
    {
        let _ = session
            .call_as(&Caller::system(), proto::Request::Logout { session: id })
            .await;
    }
    (
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
        )],
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct EmailLogin {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SetPassword {
    pub token: String,
    pub password: String,
    /// The first administrator's link asks who they are.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

async fn body<T: serde::de::DeserializeOwned>(
    server: &Server,
    request: Request<Body>,
) -> Result<(T, String, String), Box<Response>> {
    let (parts, body) = request.into_parts();
    let ip = client_ip(server, &parts.extensions, &parts.headers);
    let agent = user_agent(&parts.headers);
    let bytes = axum::body::to_bytes(body, BODY_LIMIT)
        .await
        .map_err(|_| Box::new(unauthorised("the request could not be read")))?;
    let value = serde_json::from_slice::<T>(&bytes).map_err(|_| {
        Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "the request is not the JSON expected"})),
            )
                .into_response(),
        )
    })?;
    Ok((value, ip, agent))
}

/// The answer to a sign-in: the cookie, and who signed in.
fn signed_in(server: &Server, response: proto::Response) -> Response {
    match response {
        proto::Response::SignedIn {
            session,
            expires_ms,
            user,
        } => {
            let max_age = expires_ms
                .saturating_sub(localspace_core::dag::now_ms())
                .div_ceil(1000);
            (
                [(
                    header::SET_COOKIE,
                    session_cookie(server, &session, max_age),
                )],
                Json(serde_json::json!({"user": user, "expires_ms": expires_ms})),
            )
                .into_response()
        }
        proto::Response::Error { message } => unauthorised(&message),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("unexpected answer: {other:?}")})),
        )
            .into_response(),
    }
}

async fn as_system(
    server: &Server,
    request: proto::Request,
) -> Result<proto::Response, Box<Response>> {
    let session = server.core().await.map_err(|e| {
        Box::new(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Core could not start: {e:#}")})),
            )
                .into_response(),
        )
    })?;
    session
        .call_as(&Caller::system(), request)
        .await
        .map_err(|e| {
            Box::new(
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response(),
            )
        })
}

/// `POST /api/v1/auth/login {email, password}`: organisation mode's sign-in.
/// A wrong email, a wrong password and a locked account get one sentence.
pub async fn login_with_password(
    State(server): State<Arc<Server>>,
    request: Request<Body>,
) -> Response {
    if server.cfg.personal {
        return unauthorised("This server signs in with its token.");
    }
    let (login, ip, user_agent) = match body::<EmailLogin>(&server, request).await {
        Ok(read) => read,
        Err(response) => return *response,
    };
    // The hash is verified on Core's own thread, never on the async runtime.
    match as_system(
        &server,
        proto::Request::Login {
            email: login.email,
            password: login.password,
            ip,
            user_agent,
        },
    )
    .await
    {
        Ok(response) => signed_in(&server, response),
        Err(response) => *response,
    }
}

/// `GET /api/v1/auth/invite/{token}`: whether a one-time link is live, and
/// for whom, without spending it.
pub async fn invite_status(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Response {
    match as_system(&server, proto::Request::InviteStatus { token }).await {
        Ok(proto::Response::InviteStatus {
            valid,
            email,
            name,
            first_admin,
        }) => Json(serde_json::json!({
            "valid": valid,
            "email": email,
            "name": name,
            "first_admin": first_admin,
        }))
        .into_response(),
        Ok(other) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("unexpected answer: {other:?}")})),
        )
            .into_response(),
        Err(response) => *response,
    }
}

/// `POST /api/v1/auth/set-password {token, password}`: spend a one-time
/// link on a password and sign in.
pub async fn set_password(State(server): State<Arc<Server>>, request: Request<Body>) -> Response {
    let (set, ip, user_agent) = match body::<SetPassword>(&server, request).await {
        Ok(read) => read,
        Err(response) => return *response,
    };
    match as_system(
        &server,
        proto::Request::SetPassword {
            token: set.token,
            password: set.password,
            ip,
            user_agent,
            email: set.email,
            name: set.name,
        },
    )
    .await
    {
        Ok(proto::Response::Error { message }) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": message})),
        )
            .into_response(),
        Ok(response) => signed_in(&server, response),
        Err(response) => *response,
    }
}
