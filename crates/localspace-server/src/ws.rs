//! The event stream: `/ws/json` carries `proto::Envelope` as JSON text frames
//! (v2 §5). Events flow out as they happen — the user's own, and everyone's;
//! a request sent on the socket is answered on the socket under its own id.
//! Binary frames are reserved for Automerge sync messages and blobs.
//!
//! `/ws` is the older postcard socket the egui Client speaks, kept until that
//! Client retires. It speaks to the same one Core, as the same caller.

use crate::Server;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use localspace_core::Caller;
use localspace_proto as proto;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

/// How a socket's frames are encoded: JSON text for the web client, postcard
/// binary for the egui Client.
#[derive(Clone, Copy)]
enum Wire {
    Json,
    Postcard,
}

/// A browser cannot set headers on a WebSocket, so the cookie or `?token=`
/// carries the session here.
pub async fn json_upgrade(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    upgrade(server, q, headers, ws, Wire::Json).await
}

/// The postcard socket, under the same token as the JSON one: one Core
/// answers every socket, so none may reach it unsigned.
pub async fn legacy_upgrade(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    upgrade(server, q, headers, ws, Wire::Postcard).await
}

async fn upgrade(
    server: Arc<Server>,
    q: crate::auth::TokenQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    wire: Wire,
) -> Response {
    let Some(caller) = server.authenticate(&headers, q.token.as_deref(), "").await else {
        return (StatusCode::UNAUTHORIZED, "sign in first").into_response();
    };
    let presented = crate::auth::presented(&headers, q.token.as_deref()).unwrap_or_default();
    *server.connections.lock().unwrap() += 1;
    ws.on_upgrade(move |socket| serve(server, caller, presented, socket, wire))
        .into_response()
}

/// How often an open socket checks that its session is still live, so a
/// session an admin ended closes the tab's stream within the minute.
const REVALIDATE_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

async fn serve(
    server: Arc<Server>,
    caller: Caller,
    presented: String,
    mut socket: WebSocket,
    wire: Wire,
) {
    let mut revalidate = tokio::time::interval(REVALIDATE_EVERY);
    revalidate.tick().await;
    let bearer = format!("Bearer {presented}");
    let mut as_headers = HeaderMap::new();
    if let Ok(value) = bearer.parse() {
        as_headers.insert(axum::http::header::AUTHORIZATION, value);
    }
    let notice = |level, text: String| proto::Envelope {
        id: 0,
        body: proto::Body::Event(proto::Event::Notice { level, text }),
    };
    let session = match server.core().await {
        Ok(session) => session,
        Err(e) => {
            let refusal = notice(
                proto::NoticeLevel::Error,
                format!("Core could not start: {e:#}"),
            );
            let _ = send(&mut socket, wire, &refusal).await;
            return;
        }
    };
    let mut events = session.subscribe(&caller.user);

    let hello = notice(
        proto::NoticeLevel::Info,
        format!("connected to localSpace {}", env!("CARGO_PKG_VERSION")),
    );
    if send(&mut socket, wire, &hello).await.is_err() {
        return;
    }

    // Answers to requests made on this socket arrive here, from their tasks.
    let (answers_tx, mut answers_rx) = tokio::sync::mpsc::channel::<proto::Envelope>(64);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let envelope = match (wire, incoming) {
                    (Wire::Json, Some(Ok(Message::Text(text)))) => {
                        serde_json::from_str::<proto::Envelope>(&text).map_err(|e| e.to_string())
                    }
                    (Wire::Postcard, Some(Ok(Message::Binary(bytes)))) => {
                        proto::decode(&bytes).map_err(|e| e.to_string())
                    }
                    (_, Some(Ok(Message::Close(_)))) | (_, None) => break,
                    (_, Some(Err(e))) => {
                        tracing::info!("socket closed: {e}");
                        break;
                    }
                    _ => continue,
                };
                match envelope {
                    Ok(proto::Envelope { id, body: proto::Body::Request(request) }) => {
                        let session = session.clone();
                        let caller = caller.clone();
                        let answers = answers_tx.clone();
                        tokio::spawn(async move {
                            let body = match session.call_as(&caller, request).await {
                                Ok(response) => proto::Body::Response(response),
                                Err(e) => proto::Body::Event(proto::Event::Notice {
                                    level: proto::NoticeLevel::Error,
                                    text: e.to_string(),
                                }),
                            };
                            let _ = answers.send(proto::Envelope { id, body }).await;
                        });
                    }
                    Ok(_) => tracing::warn!("a client sent something other than a request"),
                    Err(e) => tracing::warn!("undecodable frame: {e}"),
                }
            }
            answer = answers_rx.recv() => {
                if let Some(env) = answer
                    && send(&mut socket, wire, &env).await.is_err()
                {
                    break;
                }
            }
            _ = revalidate.tick() => {
                if server.authenticate(&as_headers, None, "").await.is_none() {
                    let ended = notice(
                        proto::NoticeLevel::Warn,
                        "Your session has ended. Sign in again.".into(),
                    );
                    let _ = send(&mut socket, wire, &ended).await;
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let env = proto::Envelope { id: 0, body: proto::Body::Event(event) };
                        if send(&mut socket, wire, &env).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        let env = notice(
                            proto::NoticeLevel::Warn,
                            format!("{n} events were missed; refresh the environment"),
                        );
                        if send(&mut socket, wire, &env).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send(socket: &mut WebSocket, wire: Wire, env: &proto::Envelope) -> Result<(), ()> {
    let message = match wire {
        Wire::Json => Message::Text(serde_json::to_string(env).map_err(|_| ())?.into()),
        Wire::Postcard => Message::Binary(proto::encode(env).map_err(|_| ())?.into()),
    };
    socket.send(message).await.map_err(|_| ())
}
