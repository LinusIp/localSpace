//! `/api/v1`: JSON over HTTP for request and response (v2 §5).
//!
//! The contract is `proto::Request` and `proto::Response`, unchanged: `POST
//! /api/v1/request` carries any request and answers with its response, and the
//! `GET` routes are the read-only requests a client asks for most, addressable
//! by URL. Every call is checked by Core; the client is never trusted.

use crate::session::CallError;
use crate::Server;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use localspace_proto as proto;
use std::sync::Arc;

/// The user behind a request, from the token it presented. The middleware has
/// already checked the token is this server's.
fn user(server: &Server, headers: &HeaderMap, query_token: Option<&str>) -> String {
    let token = crate::auth::presented(headers, query_token).unwrap_or_default();
    server.user_for(&token)
}

fn failed(err: CallError) -> Response {
    let status = match err {
        CallError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        CallError::Closed => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status, Json(serde_json::json!({"error": err.to_string()}))).into_response()
}

async fn call(
    server: &Server,
    headers: &HeaderMap,
    query_token: Option<&str>,
    request: proto::Request,
) -> Response {
    let session = match server.session(&user(server, headers, query_token)).await {
        Ok(session) => session,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Core could not start: {e:#}")})),
            )
                .into_response()
        }
    };
    match session.call(request).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => failed(e),
    }
}

/// `GET /api/v1/me`: who the token is, and which mode this server runs in.
pub async fn me(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
) -> Response {
    Json(serde_json::json!({
        "user": user(&server, &headers, q.token.as_deref()),
        "topology": if server.cfg.personal { "personal" } else { "organisation" },
        "version": env!("CARGO_PKG_VERSION"),
        "harness_api": proto::HARNESS_API,
    }))
    .into_response()
}

/// `POST /api/v1/request`: any request, its response.
pub async fn request(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
    Json(request): Json<proto::Request>,
) -> Response {
    call(&server, &headers, q.token.as_deref(), request).await
}

macro_rules! get_route {
    ($name:ident, $request:expr) => {
        pub async fn $name(
            State(server): State<Arc<Server>>,
            Query(q): Query<crate::auth::TokenQuery>,
            headers: HeaderMap,
        ) -> Response {
            call(&server, &headers, q.token.as_deref(), $request).await
        }
    };
}

get_route!(environment, proto::Request::GetEnvironment);
get_route!(catalog, proto::Request::ListCatalog);
get_route!(task, proto::Request::GetTask);
get_route!(active_set, proto::Request::GetActiveSet);
get_route!(lock, proto::Request::GetLock);

#[derive(serde::Deserialize)]
pub struct HistoryQuery {
    pub token: Option<String>,
    pub limit: Option<usize>,
}

/// `GET /api/v1/history?limit=N`: the most recent commits.
pub async fn history(
    State(server): State<Arc<Server>>,
    Query(q): Query<HistoryQuery>,
    headers: HeaderMap,
) -> Response {
    call(
        &server,
        &headers,
        q.token.as_deref(),
        proto::Request::GetHistory {
            limit: q.limit.unwrap_or(100).min(1000),
        },
    )
    .await
}

/// `GET /api/v1/docs/{harness}`: the harness document as JSON.
pub async fn doc(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    Path(harness): Path<String>,
    headers: HeaderMap,
) -> Response {
    call(
        &server,
        &headers,
        q.token.as_deref(),
        proto::Request::GetDocJson { harness },
    )
    .await
}
