//! `/api/v1`: JSON over HTTP for request and response (v2 §5).
//!
//! The contract is `proto::Request` and `proto::Response`, unchanged: `POST
//! /api/v1/request` carries any request and answers with its response, and the
//! `GET` routes are the read-only requests a client asks for most, addressable
//! by URL. Every call is checked by Core; the client is never trusted.

use crate::Server;
use crate::session::CallError;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
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
    let token = crate::auth::presented(headers, query_token).unwrap_or_default();
    let caller = server.caller_for(&token);
    let session = match server.core().await {
        Ok(session) => session,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Core could not start: {e:#}")})),
            )
                .into_response();
        }
    };
    match session.call_as(&caller, request).await {
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

get_route!(documents, proto::Request::ListDocuments);

#[derive(serde::Deserialize)]
pub struct ArtifactQuery {
    pub token: Option<String>,
    pub harness: String,
    pub view: String,
    pub kind: String,
    pub name: String,
    /// The fields the kind requires, as a JSON object.
    pub fields: Option<String>,
    pub summary: Option<String>,
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": message})),
    )
        .into_response()
}

/// `POST /api/v1/artifacts?harness=&view=&kind=&name=&fields=&summary=`:
/// the body is the file and its `Content-Type` the kind's media type; the
/// rest of `ProduceArtifact` travels in the query, so a surface's export
/// is one upload with no copy of the bytes into JSON. Answers as
/// `POST /api/v1/request` would.
pub async fn produce_artifact(
    State(server): State<Arc<Server>>,
    Query(q): Query<ArtifactQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or_default().trim().to_string())
        .unwrap_or_default();
    if mime.is_empty() {
        return bad_request("the body needs a Content-Type: the kind's media type");
    }
    let fields = match q.fields.as_deref().map(str::trim) {
        None | Some("") => serde_json::Value::Object(Default::default()),
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) if value.is_object() => value,
            _ => return bad_request("`fields` must be a JSON object"),
        },
    };
    call(
        &server,
        &headers,
        q.token.as_deref(),
        proto::Request::ProduceArtifact {
            harness: q.harness,
            view: q.view,
            kind: q.kind,
            name: q.name,
            mime,
            bytes: body.to_vec(),
            fields: proto::Json(fields),
            summary: q.summary.unwrap_or_default(),
        },
    )
    .await
}

/// `GET /api/v1/documents/{id}/content`: a file document's bytes at its
/// head — always an attachment, never sniffed, so nothing a user exported
/// or uploaded ever runs on the app's origin.
pub async fn document_content(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let token = crate::auth::presented(&headers, q.token.as_deref()).unwrap_or_default();
    let caller = server.caller_for(&token);
    let session = match server.core().await {
        Ok(session) => session,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Core could not start: {e:#}")})),
            )
                .into_response();
        }
    };
    match session
        .call_as(&caller, proto::Request::GetDocBlob { doc: id })
        .await
    {
        Ok(proto::Response::DocBlob { name, mime, bytes }) => {
            let mut response = (StatusCode::OK, bytes).into_response();
            let h = response.headers_mut();
            h.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&mime)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            );
            h.insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&content_disposition(&name))
                    .unwrap_or(HeaderValue::from_static("attachment")),
            );
            h.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            h.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
        // Not a file the caller may read, or one with no content: the same
        // answer, so the route says nothing about what exists.
        Ok(proto::Response::Error { message }) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": message})),
        )
            .into_response(),
        Ok(other) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("unexpected answer: {other:?}")})),
        )
            .into_response(),
        Err(e) => failed(e),
    }
}

/// `attachment; filename="…"` with an ASCII-safe name, and the exact name
/// as `filename*` when it has characters outside ASCII (RFC 6266).
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if (c.is_ascii_graphic() && c != '"' && c != '\\') || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ascii = if ascii.chars().any(|c| c.is_ascii_alphanumeric()) {
        ascii
    } else {
        "download".to_string()
    };
    if ascii == name {
        format!("attachment; filename=\"{ascii}\"")
    } else {
        format!(
            "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
            percent_encode(name)
        )
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_is_always_an_attachment_with_a_safe_name() {
        assert_eq!(
            content_disposition("board-a1b2c3d.png"),
            "attachment; filename=\"board-a1b2c3d.png\""
        );
        assert_eq!(
            content_disposition("say \"hi\".png"),
            "attachment; filename=\"say _hi_.png\"; filename*=UTF-8''say%20%22hi%22.png"
        );
        assert_eq!(
            content_disposition("доска.svg"),
            "attachment; filename=\"_____.svg\"; filename*=UTF-8''%D0%B4%D0%BE%D1%81%D0%BA%D0%B0.svg"
        );
        assert_eq!(
            content_disposition("\n"),
            "attachment; filename=\"download\"; filename*=UTF-8''%0A"
        );
    }
}
