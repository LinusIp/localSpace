//! The HTTP and WebSocket API a browser or the desktop shell speaks
//! (architecture v2 §5): JSON in, JSON out, a token to get in.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use localspace_proto as proto;
use localspace_server::{router, Server, ServerConfig};
use std::path::PathBuf;
use tower::ServiceExt;

fn harness_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harnesses");
    dir.join("whiteboard/logic.wasm").exists().then_some(dir)
}

fn config(token: &str) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".into(),
        harnesses: harness_dir(),
        personal: true,
        user: "tester".into(),
        token: Some(token.into()),
        web_root: None,
        ..ServerConfig::default()
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn the_api_is_closed_without_the_token_and_open_with_it() {
    let app = router(Server::new(config("secret-1")));

    let anonymous = app
        .clone()
        .oneshot(Request::get("/api/v1/environment").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let wrong = app
        .clone()
        .oneshot(
            Request::post("/api/v1/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"token":"guess"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let login = app
        .clone()
        .oneshot(
            Request::post("/api/v1/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"token":"secret-1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .expect("login sets the session cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("ls_session=secret-1;"), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");

    // The cookie opens the door; so does a bearer token.
    let with_cookie = app
        .clone()
        .oneshot(
            Request::get("/api/v1/environment")
                .header(header::COOKIE, "ls_session=secret-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(with_cookie.status(), StatusCode::OK);
    let env = body_json(with_cookie).await;
    let state = &env["environment"];
    assert_eq!(state["user"], "tester", "personal mode: the configured user: {env}");
    assert_eq!(state["topology"], "personal");
    if harness_dir().is_some() {
        assert!(
            state["harnesses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["id"] == "io.localspace.whiteboard"),
            "the installed harness is listed: {env}"
        );
    }

    let bearer = app
        .clone()
        .oneshot(
            Request::get("/api/v1/me")
                .header(header::AUTHORIZATION, "Bearer secret-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bearer.status(), StatusCode::OK);
    let me = body_json(bearer).await;
    assert_eq!(me["user"], "tester");
    assert_eq!(me["topology"], "personal");
}

#[tokio::test]
async fn any_request_goes_through_one_door_and_is_answered_in_json() {
    let app = router(Server::new(config("secret-2")));
    let response = app
        .oneshot(
            Request::post("/api/v1/request")
                .header(header::AUTHORIZATION, "Bearer secret-2")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"get_history":{"limit":5}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(
        body.get("history").is_some(),
        "a Response, serde-named, as the TypeScript types expect: {body}"
    );
}

#[tokio::test]
async fn in_personal_mode_the_readiness_probe_and_the_user_share_one_core() {
    // The bug this guards: with a data directory, the probe's own Core held
    // the database, the user's Core could not open it, the failure panicked
    // inside a lock, and every later request failed on the poisoned lock.
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config("secret-5");
    cfg.data = Some(data.path().to_path_buf());
    let app = router(Server::new(cfg));

    let ready = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    for _ in 0..2 {
        let env = app
            .clone()
            .oneshot(
                Request::get("/api/v1/environment")
                    .header(header::AUTHORIZATION, "Bearer secret-5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(env.status(), StatusCode::OK, "the user's Core after the probe's");
    }
}

#[tokio::test]
async fn the_openapi_document_is_generated_from_proto() {
    let app = router(Server::new(config("secret-3")));
    let response = app
        .oneshot(Request::get("/api/v1/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let doc = body_json(response).await;
    assert_eq!(doc["openapi"], "3.1.0");
    let schemas = doc["components"]["schemas"].as_object().unwrap();
    for name in ["Request", "Response", "Event", "Envelope", "EnvironmentState", "HarnessSummary"] {
        assert!(schemas.contains_key(name), "schema {name} missing; have {:?}", schemas.keys().collect::<Vec<_>>());
    }
    assert!(doc["paths"]["/api/v1/request"]["post"].is_object());
    // The schema carries serde's names, the same the wire and the TypeScript use.
    let request = serde_json::to_string(&schemas["Request"]).unwrap();
    assert!(request.contains("get_environment"), "{request}");
}

#[tokio::test]
async fn the_json_socket_streams_events_and_answers_requests_under_their_id() {
    let running = localspace_server::start(config("secret-4")).await.expect("start");
    let url = format!("ws://{}/ws/json?token=secret-4", running.addr);
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.expect("connect");

    // The first frame is the welcome notice, an Event.
    let first = socket.next().await.unwrap().unwrap();
    let hello: proto::Envelope = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert!(matches!(hello.body, proto::Body::Event(proto::Event::Notice { .. })));

    let request = proto::Envelope {
        id: 7,
        body: proto::Body::Request(proto::Request::GetEnvironment),
    };
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&request).unwrap().into(),
        ))
        .await
        .unwrap();

    // The answer comes back under id 7, whatever events arrive around it.
    let answer = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let frame = socket.next().await.unwrap().unwrap();
            let env: proto::Envelope = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            if env.id == 7 {
                return env;
            }
        }
    })
    .await
    .expect("an answer within the timeout");
    match answer.body {
        proto::Body::Response(proto::Response::Environment(state)) => {
            assert_eq!(state.user, "tester");
        }
        other => panic!("expected the environment, got {other:?}"),
    }

    // Without a token the socket is refused before the upgrade.
    let refused = tokio_tungstenite::connect_async(format!("ws://{}/ws/json", running.addr)).await;
    assert!(refused.is_err(), "an anonymous socket must not upgrade");
}
