//! Who is asking. A session cookie for browsers, a bearer token for the desktop
//! shell and scripts (v2 §5). In personal mode the token is generated at start
//! and handed to the shell; in organisation mode it stands in for the IdP leg
//! until OIDC is built (build order step 7), and every holder of it is the
//! same operator.

use crate::Server;
use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;

pub const COOKIE: &str = "ls_session";

/// The token the request presents, wherever it put it.
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

/// Compared as hashes, so the comparison takes the same time whatever matches.
pub fn matches(server: &Server, token: &str) -> bool {
    blake3::hash(token.as_bytes()) == blake3::hash(server.token.as_bytes())
}

#[derive(Deserialize, Default)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// Middleware: everything under `/api/v1` except login needs a valid token.
pub async fn require(
    State(server): State<Arc<Server>>,
    Query(query): Query<TokenQuery>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match presented(request.headers(), query.token.as_deref()) {
        Some(token) if matches(&server, &token) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "sign in first"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct Login {
    pub token: String,
}

/// `POST /api/v1/login` with the token sets the session cookie.
pub async fn login(State(server): State<Arc<Server>>, Json(login): Json<Login>) -> Response {
    if !matches(&server, &login.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "that token is not this server's"})),
        )
            .into_response();
    }
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict{}",
        login.token,
        if server.cfg.secure_cookies {
            "; Secure"
        } else {
            ""
        }
    );
    (
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"user": server.user_for(&login.token)})),
    )
        .into_response()
}

/// `POST /api/v1/logout` clears it.
pub async fn logout() -> Response {
    let cookie = format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
    (
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}
