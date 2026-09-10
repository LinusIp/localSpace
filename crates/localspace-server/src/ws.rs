//! The event stream: `/ws/json` carries `proto::Envelope` as JSON text frames
//! (v2 §5). Events flow out as they happen; a request sent on the socket is
//! answered on the socket under its own id. Binary frames are reserved for
//! Automerge sync messages and blobs.
//!
//! `/ws` is the older postcard socket the egui Client speaks, kept until that
//! Client retires.

use crate::Server;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use localspace_core::transport::{Backend, Incoming};
use localspace_proto as proto;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

/// A browser cannot set headers on a WebSocket, so the cookie or `?token=`
/// carries the session here.
pub async fn json_upgrade(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(token) = crate::auth::presented(&headers, q.token.as_deref()) else {
        return (StatusCode::UNAUTHORIZED, "sign in first").into_response();
    };
    if !crate::auth::matches(&server, &token) {
        return (StatusCode::UNAUTHORIZED, "that token is not this server's").into_response();
    }
    let user = server.user_for(&token);
    *server.connections.lock().unwrap() += 1;
    ws.on_upgrade(move |socket| serve_json(server, user, socket))
        .into_response()
}

async fn serve_json(server: Arc<Server>, user: String, mut socket: WebSocket) {
    let session = match server.session(&user).await {
        Ok(session) => session,
        Err(e) => {
            let refusal = proto::Envelope {
                id: 0,
                body: proto::Body::Event(proto::Event::Notice {
                    level: proto::NoticeLevel::Error,
                    text: format!("Core could not start: {e:#}"),
                }),
            };
            let _ = send_json(&mut socket, &refusal).await;
            return;
        }
    };
    let mut events = session.subscribe();

    let hello = proto::Envelope {
        id: 0,
        body: proto::Body::Event(proto::Event::Notice {
            level: proto::NoticeLevel::Info,
            text: format!("connected to localSpace {}", env!("CARGO_PKG_VERSION")),
        }),
    };
    if send_json(&mut socket, &hello).await.is_err() {
        return;
    }

    // Answers to requests made on this socket arrive here, from their tasks.
    let (answers_tx, mut answers_rx) = tokio::sync::mpsc::channel::<proto::Envelope>(64);

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<proto::Envelope>(&text) {
                            Ok(proto::Envelope { id, body: proto::Body::Request(request) }) => {
                                let session = session.clone();
                                let answers = answers_tx.clone();
                                tokio::spawn(async move {
                                    let body = match session.call(request).await {
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
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::info!("socket closed: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            answer = answers_rx.recv() => {
                if let Some(env) = answer {
                    if send_json(&mut socket, &env).await.is_err() {
                        break;
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let env = proto::Envelope { id: 0, body: proto::Body::Event(event) };
                        if send_json(&mut socket, &env).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        let env = proto::Envelope {
                            id: 0,
                            body: proto::Body::Event(proto::Event::Notice {
                                level: proto::NoticeLevel::Warn,
                                text: format!("{n} events were missed; refresh the environment"),
                            }),
                        };
                        if send_json(&mut socket, &env).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, env: &proto::Envelope) -> Result<(), ()> {
    let text = serde_json::to_string(env).map_err(|_| ())?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

// ---------------------------------------------------------------------------
// The postcard socket the egui Client speaks
// ---------------------------------------------------------------------------

pub async fn legacy_upgrade(State(server): State<Arc<Server>>, ws: WebSocketUpgrade) -> Response {
    *server.connections.lock().unwrap() += 1;
    ws.on_upgrade(move |socket| serve_legacy(server, socket))
        .into_response()
}

async fn serve_legacy(server: Arc<Server>, mut socket: WebSocket) {
    // One Core per connection, as before: this path predates sessions.
    let key = format!("s_{}", uuid::Uuid::new_v4());
    let backend = match server.legacy_backend(&key).await {
        Ok(backend) => backend,
        Err(e) => {
            tracing::warn!("a postcard client could not get a Core: {e:#}");
            return;
        }
    };

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
                    Some(Ok(Message::Binary(bytes))) => match proto::decode(&bytes) {
                        Ok(env) => {
                            if let proto::Body::Request(req) = env.body {
                                backend.request(req);
                            }
                        }
                        Err(e) => tracing::warn!("undecodable frame: {e}"),
                    },
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
                        break;
                    }
                }
            }
        }
    }

    server.drop_legacy_backend(&key);
}
