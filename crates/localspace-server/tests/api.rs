//! The HTTP and WebSocket API a browser or the desktop shell speaks
//! (architecture v2 §5): JSON in, JSON out, a token to get in.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use localspace_proto as proto;
use localspace_server::{Server, ServerConfig, router};
use std::path::PathBuf;
use tower::ServiceExt;

fn harness_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harnesses");
    dir.join("whiteboard/logic.wasm").exists().then_some(dir)
}

/// The repository's `registry/`: the catalog the whiteboard's dependency,
/// the types package, resolves from (plugin spec §18.3).
fn registry_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry");
    dir.join("types/types.toml").exists().then_some(dir)
}

fn config(token: &str) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".into(),
        harnesses: harness_dir(),
        registry: registry_dir().into_iter().collect(),
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
        .oneshot(
            Request::get("/api/v1/environment")
                .body(Body::empty())
                .unwrap(),
        )
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

    // The token in the address is not a way in: only the WebSocket upgrade
    // reads it there.
    let in_the_address = app
        .clone()
        .oneshot(
            Request::get("/api/v1/environment?token=secret-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(in_the_address.status(), StatusCode::UNAUTHORIZED);

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
    assert_eq!(
        state["user"], "tester",
        "personal mode: the configured user: {env}"
    );
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
        assert_eq!(
            env.status(),
            StatusCode::OK,
            "the user's Core after the probe's"
        );
    }
}

#[tokio::test]
async fn the_openapi_document_is_generated_from_proto() {
    let app = router(Server::new(config("secret-3")));
    let response = app
        .oneshot(
            Request::get("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let doc = body_json(response).await;
    assert_eq!(doc["openapi"], "3.1.0");
    let schemas = doc["components"]["schemas"].as_object().unwrap();
    for name in [
        "Request",
        "Response",
        "Event",
        "Envelope",
        "EnvironmentState",
        "HarnessSummary",
    ] {
        assert!(
            schemas.contains_key(name),
            "schema {name} missing; have {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }
    assert!(doc["paths"]["/api/v1/request"]["post"].is_object());
    // The schema carries serde's names, the same the wire and the TypeScript use.
    let request = serde_json::to_string(&schemas["Request"]).unwrap();
    assert!(request.contains("get_environment"), "{request}");
}

#[tokio::test]
async fn the_json_socket_streams_events_and_answers_requests_under_their_id() {
    let running = localspace_server::start(config("secret-4"))
        .await
        .expect("start");
    let url = format!("ws://{}/ws/json?token=secret-4", running.addr);
    let (mut socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect");

    // The first frame is the welcome notice, an Event.
    let first = socket.next().await.unwrap().unwrap();
    let hello: proto::Envelope = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert!(matches!(
        hello.body,
        proto::Body::Event(proto::Event::Notice { .. })
    ));

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

#[tokio::test]
async fn a_web_surface_lives_on_its_own_origin_behind_a_grant() {
    // v2 §6.3: the shell asks for a view, gets a URL on the harness's origin,
    // and that origin serves the view's files and nothing else, under a CSP.
    let Some(_) = harness_dir() else { return };
    let app = router(Server::new(config("secret-4")));

    let opened = app
        .clone()
        .oneshot(
            Request::post("/api/v1/surfaces")
                .header(header::COOKIE, "ls_session=secret-4")
                .header(header::HOST, "127.0.0.1:8443")
                .header(header::ORIGIN, "http://127.0.0.1:8443")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"harness":"io.localspace.whiteboard","view":"web"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(opened.status(), StatusCode::OK);
    let grant = body_json(opened).await;
    let url = grant["url"].as_str().unwrap().to_string();
    assert!(
        url.starts_with("http://h-io-localspace-whiteboard.localhost:8443/s/"),
        "{url}"
    );
    assert!(url.ends_with('/'), "the page is the grant directory: {url}");
    let host = "h-io-localspace-whiteboard.localhost:8443";
    let token = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let under = |p: &str| format!("/s/{token}/{p}");

    // A view that is not a web surface, or does not exist, gets no origin.
    for view in ["settings", "nothing"] {
        let refused = app
            .clone()
            .oneshot(
                Request::post("/api/v1/surfaces")
                    .header(header::COOKIE, "ls_session=secret-4")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        r#"{{"harness":"io.localspace.whiteboard","view":"{view}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "view `{view}`");
    }

    // The grant directory is the generated page, which maps the SDK and loads
    // the entry module by relative URL, so both stay under the grant.
    let index = app
        .clone()
        .oneshot(
            Request::get(under(""))
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(index.status(), StatusCode::OK);
    let csp = index
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .expect("a policy on every surface response")
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.starts_with("default-src 'none'"), "{csp}");
    assert!(
        csp.contains("frame-ancestors http://127.0.0.1:8443"),
        "{csp}"
    );
    assert!(
        !index.headers().contains_key(header::SET_COOKIE),
        "no cookie: a third-party frame would not send it back"
    );
    let html = String::from_utf8(
        index
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        html.contains(r#""@localspace/harness-sdk":"./_localspace/sdk.js""#),
        "{html}"
    );
    assert!(html.contains(r#"src="./index.js""#), "{html}");
    let nonce = html
        .split(r#"nonce=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the import map carries a nonce");
    assert!(
        csp.contains(&format!("'nonce-{nonce}'")),
        "the page's nonce is in its policy: {csp}"
    );

    let fetch = |path: String, on_host: &str| {
        app.clone().oneshot(
            Request::get(path)
                .header(header::HOST, on_host)
                .body(Body::empty())
                .unwrap(),
        )
    };

    // The entry module and the SDK, under the grant.
    let module = fetch(under("index.js"), host).await.unwrap();
    assert_eq!(module.status(), StatusCode::OK);
    assert_eq!(
        module.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert!(
        module
            .headers()
            .contains_key(header::CONTENT_SECURITY_POLICY)
    );
    let sdk = fetch(under("_localspace/sdk.js"), host).await.unwrap();
    assert_eq!(sdk.status(), StatusCode::OK);
    let sdk_text =
        String::from_utf8(sdk.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(
        sdk_text.contains("export function connect"),
        "the embedded SDK"
    );

    // Nothing above the view's directory, nothing without the grant, and
    // nothing on another harness's origin with this grant.
    for escape in [
        "../harness.toml",
        "../../logic.wasm",
        "%2e%2e/harness.toml",
        "assets",
    ] {
        let refused = fetch(under(escape), host).await.unwrap();
        assert_ne!(
            refused.status(),
            StatusCode::OK,
            "`{escape}` must not be served"
        );
    }
    assert_eq!(
        fetch("/index.js".into(), host).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fetch("/s/0123456789abcdef/index.js".into(), host)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED,
        "a token nobody minted"
    );
    assert_eq!(
        fetch(under("index.js"), "h-io-other.localhost:8443")
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // The harness origin is not the API: the session cookie means nothing
    // there, and the API is not reachable through it.
    let api_through_surface = app
        .clone()
        .oneshot(
            Request::get("/api/v1/environment")
                .header(header::HOST, host)
                .header(header::COOKIE, "ls_session=secret-4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api_through_surface.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_harness_origin_serves_the_shell_s_libraries_and_nothing_beside_them() {
    // v2.1 §6.3: the shell provides React, the canvas, the UI library and
    // Automerge to frames through the import map, from `_localspace/` in
    // the web bundle; a name that is not a plain file name is not a file.
    let Some(_) = harness_dir() else { return };
    let web = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(web.path().join("_localspace")).unwrap();
    std::fs::write(
        web.path().join("_localspace/canvas.js"),
        b"export const canvas = 1;",
    )
    .unwrap();
    std::fs::write(web.path().join("index.html"), b"<!doctype html>").unwrap();
    std::fs::write(web.path().join("secret.txt"), b"not for frames").unwrap();
    let mut cfg = config("secret-5");
    cfg.web_root = Some(web.path().to_path_buf());
    let app = router(Server::new(cfg));

    let opened = app
        .clone()
        .oneshot(
            Request::post("/api/v1/surfaces")
                .header(header::COOKIE, "ls_session=secret-5")
                .header(header::HOST, "127.0.0.1:8443")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"harness":"io.localspace.whiteboard","view":"web"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(opened.status(), StatusCode::OK);
    let url = body_json(opened).await["url"].as_str().unwrap().to_string();
    let token = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let host = "h-io-localspace-whiteboard.localhost:8443";
    let fetch = |path: String| {
        app.clone().oneshot(
            Request::get(path)
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
    };

    let lib = fetch(format!("/s/{token}/_localspace/canvas.js"))
        .await
        .unwrap();
    assert_eq!(lib.status(), StatusCode::OK);
    assert_eq!(
        lib.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert!(lib.headers().contains_key(header::CONTENT_SECURITY_POLICY));
    let text =
        String::from_utf8(lib.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert_eq!(text, "export const canvas = 1;");

    // Automerge's WebAssembly, fetched by its module from beside it, comes
    // with the type a browser compiles as it streams in, under the same policy.
    std::fs::write(
        web.path().join("_localspace/automerge_wasm_bg.wasm"),
        b"\0asm\x01\0\0\0",
    )
    .unwrap();
    let wasm = fetch(format!("/s/{token}/_localspace/automerge_wasm_bg.wasm"))
        .await
        .unwrap();
    assert_eq!(wasm.status(), StatusCode::OK);
    assert_eq!(
        wasm.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/wasm"
    );
    assert!(wasm.headers().contains_key(header::CONTENT_SECURITY_POLICY));

    for missing in [
        "_localspace/nothing.js",
        "_localspace/../secret.txt",
        "_localspace/..%2Fsecret.txt",
        "_localspace/.hidden",
        "_localspace/",
    ] {
        let refused = fetch(format!("/s/{token}/{missing}")).await.unwrap();
        assert_eq!(
            refused.status(),
            StatusCode::NOT_FOUND,
            "`{missing}` must not be served"
        );
    }
    // The page's import map names every one of them by that path.
    let index = fetch(format!("/s/{token}/")).await.unwrap();
    let html = String::from_utf8(
        index
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    for name in [
        "sdk.js",
        "canvas.js",
        "ui.js",
        "react.js",
        "react-jsx-runtime.js",
        "react-dom-client.js",
        "automerge.js",
    ] {
        assert!(
            html.contains(&format!("./_localspace/{name}")),
            "{name} in the import map: {html}"
        );
    }
}

async fn send(app: &axum::Router, request: Request<Body>) -> axum::response::Response {
    app.clone().oneshot(request).await.unwrap()
}

fn percent(s: &str) -> String {
    let mut out = String::new();
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

#[tokio::test]
async fn an_export_goes_in_as_bytes_and_comes_out_as_a_download() {
    // 6.0: a surface's PNG or SVG of its board is one upload with the file
    // as the body, a document of its own afterwards, and a download that is
    // always an attachment and never sniffed.
    let Some(_) = harness_dir() else { return };
    let app = router(Server::new(config("secret-6")));
    let auth = || (header::AUTHORIZATION, "Bearer secret-6");
    let get = |path: String| {
        Request::get(path)
            .header(auth().0, auth().1)
            .body(Body::empty())
            .unwrap()
    };
    let post_bytes = |path: String, mime: &str, bytes: Vec<u8>| {
        Request::post(path)
            .header(auth().0, auth().1)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(bytes))
            .unwrap()
    };

    // A commit on the board, to export from.
    let added = send(
        &app,
        Request::post("/api/v1/request")
            .header(auth().0, auth().1)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"call_tool":{"tool":"canvas.add_sticky","params":{"text":"export me","fill":"yellow"}}}"#,
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(added.status(), StatusCode::OK);
    let board = body_json(send(&app, get("/api/v1/docs/io.localspace.whiteboard".into())).await)
        .await["doc_json"]["doc"]
        .as_str()
        .unwrap()
        .to_string();
    let history = body_json(send(&app, get("/api/v1/history?limit=5".into())).await).await;
    let newest = &history["history"]["commits"][0];
    assert_eq!(newest["doc"], board, "{history}");
    let head = newest["id"].as_str().unwrap().to_string();

    let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &[0u8; 32]].concat();
    let fields = format!(r#"{{"document":"{board}","commit":"{head}"}}"#);
    let query = format!(
        "harness=io.localspace.whiteboard&view=web&kind=image.v1&name=board&summary=PNG%20of%20the%20board&fields={}",
        percent(&fields)
    );
    let produced = send(
        &app,
        post_bytes(
            format!("/api/v1/artifacts?{query}"),
            "image/png",
            png.clone(),
        ),
    )
    .await;
    assert_eq!(produced.status(), StatusCode::OK);
    let body = body_json(produced).await;
    let artifact = &body["artifact"];
    assert_eq!(artifact["kind"], "image.v1", "{body}");
    let name = format!("board-{}.png", &head[..7]);
    assert_eq!(artifact["file"]["name"], name);
    assert_eq!(artifact["file"]["mime"], "image/png");
    assert_eq!(artifact["fields"]["commit"], head);
    let doc = artifact["doc"].as_str().unwrap().to_string();
    assert!(doc.starts_with("blob:"), "{doc}");

    // Listed, with where it came from.
    let listed = body_json(send(&app, get("/api/v1/documents".into())).await).await;
    let entry = listed["documents"]["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == doc)
        .unwrap_or_else(|| panic!("the export is not listed: {listed}"));
    assert_eq!(entry["source"]["export"]["commit"], head);
    assert_eq!(entry["bytes"], png.len());

    // Downloaded: an attachment with its own media type, never sniffed.
    let content = send(&app, get(format!("/api/v1/documents/{doc}/content"))).await;
    assert_eq!(content.status(), StatusCode::OK);
    let headers = content.headers().clone();
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert_eq!(
        headers.get(header::CONTENT_DISPOSITION).unwrap(),
        &format!("attachment; filename=\"{name}\"")
    );
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    let bytes = content.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), png.as_slice());

    // An SVG, which a browser would run inline, is an attachment too.
    let svg =
        br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.to_vec();
    let svg_query = query.replace("kind=image.v1", "kind=svg.v1");
    let produced = send(
        &app,
        post_bytes(
            format!("/api/v1/artifacts?{svg_query}"),
            "image/svg+xml",
            svg.clone(),
        ),
    )
    .await;
    let body = body_json(produced).await;
    let svg_doc = body["artifact"]["doc"]
        .as_str()
        .unwrap_or_else(|| panic!("{body}"))
        .to_string();
    let content = send(&app, get(format!("/api/v1/documents/{svg_doc}/content"))).await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        content.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/svg+xml"
    );
    assert!(
        content
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );

    // The wrong media type for the kind is refused in the answer, and
    // nothing is kept.
    let wrong = send(
        &app,
        post_bytes(
            format!("/api/v1/artifacts?{query}"),
            "text/plain",
            png.clone(),
        ),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::OK);
    let body = body_json(wrong).await;
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("image/png"),
        "{body}"
    );

    // No Content-Type: the route cannot say what the file is.
    let untyped = send(
        &app,
        Request::post(format!("/api/v1/artifacts?{query}"))
            .header(auth().0, auth().1)
            .body(Body::from(png.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(untyped.status(), StatusCode::BAD_REQUEST);

    // Over the limit: refused by size before it is read.
    let too_big = vec![0u8; localspace_core::MAX_ARTIFACT_BYTES + 1];
    let refused = send(
        &app,
        post_bytes(format!("/api/v1/artifacts?{query}"), "image/png", too_big),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // The board's own document is not a file: no download.
    let none = send(&app, get(format!("/api/v1/documents/{board}/content"))).await;
    assert_eq!(none.status(), StatusCode::NOT_FOUND);

    // Without a session, none of it.
    for request in [
        Request::post(format!("/api/v1/artifacts?{query}"))
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(png.clone()))
            .unwrap(),
        Request::get("/api/v1/documents")
            .body(Body::empty())
            .unwrap(),
        Request::get(format!("/api/v1/documents/{doc}/content"))
            .body(Body::empty())
            .unwrap(),
    ] {
        let path = request.uri().path().to_string();
        assert_eq!(
            send(&app, request).await.status(),
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
}

// ---------------------------------------------------------------------------
// Organisation mode: accounts, sessions and the first administrator
// (deployment §4; Pilot 1, Phase A)
// ---------------------------------------------------------------------------

fn organisation(data: &std::path::Path) -> ServerConfig {
    ServerConfig {
        bind: "127.0.0.1:0".into(),
        harnesses: harness_dir(),
        registry: registry_dir().into_iter().collect(),
        personal: false,
        data: Some(data.to_path_buf()),
        web_root: None,
        ..ServerConfig::default()
    }
}

/// The session cookie a sign-in set, as the next request sends it back.
fn cookie_of(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::SET_COOKIE)
        .expect("the sign-in set a cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

fn post_json(path: &str, body: String, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::from(body)).unwrap()
}

fn get_with(path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn an_organisation_signs_in_with_email_and_password_from_the_first_admin_s_link() {
    let Some(_) = harness_dir() else { return };
    let data = tempfile::tempdir().unwrap();
    let server = Server::new(organisation(data.path()));
    let app = router(server.clone());

    // Personal mode's token is not a way in here.
    let token_login = send(
        &app,
        post_json("/api/v1/login", r#"{"token":"anything"}"#.into(), None),
    )
    .await;
    assert_eq!(token_login.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        send(&app, get_with("/api/v1/environment", None))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // The first administrator's link is minted by the server itself while
    // there is nobody, and asks who they are, since the server knows none.
    let invite = server
        .mint_first_admin()
        .await
        .unwrap()
        .expect("no accounts yet");
    assert!(invite.first_admin);
    let link = server.invite_link(&invite.token, &"127.0.0.1:8443".parse().unwrap());
    assert_eq!(
        link,
        format!("http://127.0.0.1:8443/invite/{}", invite.token)
    );

    // The link is checked without being spent, then spent on a name, an
    // email and a password.
    let status = body_json(
        send(
            &app,
            get_with(&format!("/api/v1/auth/invite/{}", invite.token), None),
        )
        .await,
    )
    .await;
    assert_eq!(status["valid"], true);
    assert_eq!(status["first_admin"], true);
    assert!(status["email"].is_null());
    let nameless = send(
        &app,
        post_json(
            "/api/v1/auth/set-password",
            format!(
                r#"{{"token":"{}","password":"a root password of length"}}"#,
                invite.token
            ),
            None,
        ),
    )
    .await;
    assert_eq!(
        nameless.status(),
        StatusCode::BAD_REQUEST,
        "the first administrator gives a name and an email"
    );
    let short = send(
        &app,
        post_json(
            "/api/v1/auth/set-password",
            format!(
                r#"{{"token":"{}","password":"short","email":"root@example.com","name":"Root"}}"#,
                invite.token
            ),
            None,
        ),
    )
    .await;
    assert_eq!(
        short.status(),
        StatusCode::BAD_REQUEST,
        "twelve characters at least"
    );
    let set = send(
        &app,
        post_json(
            "/api/v1/auth/set-password",
            format!(
                r#"{{"token":"{}","password":"a root password of length","email":"Root@Example.com","name":"Root"}}"#,
                invite.token
            ),
            None,
        ),
    )
    .await;
    assert_eq!(set.status(), StatusCode::OK);
    assert!(
        server.mint_first_admin().await.unwrap().is_none(),
        "only while there are no accounts"
    );
    let root_cookie = cookie_of(&set);
    assert!(root_cookie.starts_with("ls_session="), "{root_cookie}");
    let spent = body_json(
        send(
            &app,
            get_with(&format!("/api/v1/auth/invite/{}", invite.token), None),
        )
        .await,
    )
    .await;
    assert_eq!(spent["valid"], false, "single use");

    // Signed in: the caller is the admin, by the cookie and as a bearer.
    let me = body_json(send(&app, get_with("/api/v1/me", Some(&root_cookie))).await).await;
    assert_eq!(me["email"], "root@example.com");
    assert_eq!(me["roles"], serde_json::json!(["admin"]));
    assert_eq!(me["topology"], "organisation");
    let bearer = root_cookie.trim_start_matches("ls_session=").to_string();
    let as_bearer = send(
        &app,
        Request::get("/api/v1/me")
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(as_bearer.status(), StatusCode::OK);

    // The admin makes a member; the member sets a password and signs in.
    let made = body_json(
        send(
            &app,
            post_json(
                "/api/v1/request",
                r#"{"create_user":{"email":"Anna@Example.com","name":"Anna","roles":["member"]}}"#
                    .into(),
                Some(&root_cookie),
            ),
        )
        .await,
    )
    .await;
    let anna_token = made["invite"]["token"]
        .as_str()
        .expect("an invite for Anna")
        .to_string();
    let set = send(
        &app,
        post_json(
            "/api/v1/auth/set-password",
            format!(r#"{{"token":"{anna_token}","password":"anna has a long password"}}"#),
            None,
        ),
    )
    .await;
    assert_eq!(set.status(), StatusCode::OK);
    let login = send(
        &app,
        post_json(
            "/api/v1/auth/login",
            r#"{"email":"anna@example.com","password":"anna has a long password"}"#.into(),
            None,
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let anna_cookie = cookie_of(&login);
    let me = body_json(send(&app, get_with("/api/v1/me", Some(&anna_cookie))).await).await;
    assert_eq!(me["email"], "anna@example.com");
    assert_eq!(me["roles"], serde_json::json!(["member"]));

    // A member is not an admin: the accounts are refused to her.
    let refused = body_json(
        send(
            &app,
            post_json(
                "/api/v1/request",
                r#""list_users""#.into(),
                Some(&anna_cookie),
            ),
        )
        .await,
    )
    .await;
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("administrator"),
        "{refused}"
    );

    // The wrong password and an unknown email say the same thing, as 401.
    let wrong = send(
        &app,
        post_json(
            "/api/v1/auth/login",
            r#"{"email":"anna@example.com","password":"not it, not it"}"#.into(),
            None,
        ),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let wrong = body_json(wrong).await;
    let unknown = body_json(
        send(
            &app,
            post_json(
                "/api/v1/auth/login",
                r#"{"email":"nobody@example.com","password":"not it, not it"}"#.into(),
                None,
            ),
        )
        .await,
    )
    .await;
    assert_eq!(wrong["error"], unknown["error"]);
    assert_eq!(wrong["error"], "That email or password isn't right.");

    // Five wrong passwords lock the account: even the right one is refused,
    // with the same sentence; the admin sees the lock and clears it.
    for _ in 0..4 {
        let _ = send(
            &app,
            post_json(
                "/api/v1/auth/login",
                r#"{"email":"anna@example.com","password":"not it, not it"}"#.into(),
                None,
            ),
        )
        .await;
    }
    let locked = send(
        &app,
        post_json(
            "/api/v1/auth/login",
            r#"{"email":"anna@example.com","password":"anna has a long password"}"#.into(),
            None,
        ),
    )
    .await;
    assert_eq!(locked.status(), StatusCode::UNAUTHORIZED);
    let listed = body_json(
        send(
            &app,
            post_json(
                "/api/v1/request",
                r#""list_users""#.into(),
                Some(&root_cookie),
            ),
        )
        .await,
    )
    .await;
    let anna = listed["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == "anna@example.com")
        .unwrap();
    assert!(anna["locked_until_ms"].is_number(), "{anna}");
    let anna_id = anna["id"].as_str().unwrap().to_string();
    let _ = send(
        &app,
        post_json(
            "/api/v1/request",
            format!(r#"{{"unlock_user":{{"user":"{anna_id}"}}}}"#),
            Some(&root_cookie),
        ),
    )
    .await;
    let unlocked = send(
        &app,
        post_json(
            "/api/v1/auth/login",
            r#"{"email":"anna@example.com","password":"anna has a long password"}"#.into(),
            None,
        ),
    )
    .await;
    assert_eq!(unlocked.status(), StatusCode::OK);

    // Disabling ends her sessions: the cookie that worked is refused at once.
    assert_eq!(
        send(&app, get_with("/api/v1/environment", Some(&anna_cookie)))
            .await
            .status(),
        StatusCode::OK
    );
    let _ = send(
        &app,
        post_json(
            "/api/v1/request",
            format!(r#"{{"disable_user":{{"user":"{anna_id}","disabled":true}}}}"#),
            Some(&root_cookie),
        ),
    )
    .await;
    assert_eq!(
        send(&app, get_with("/api/v1/environment", Some(&anna_cookie)))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // Logout ends the admin's session and clears the cookie.
    let out = send(
        &app,
        post_json("/api/v1/logout", "{}".into(), Some(&root_cookie)),
    )
    .await;
    assert!(
        out.headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        send(&app, get_with("/api/v1/me", Some(&root_cookie)))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // The readiness probe needs no user in organisation mode.
    assert_eq!(
        send(&app, get_with("/readyz", None)).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_first_administrator_s_link_is_written_at_start_and_deleted_when_used() {
    let Some(_) = harness_dir() else { return };
    let data = tempfile::tempdir().unwrap();
    let running = localspace_server::start(organisation(data.path()))
        .await
        .unwrap();
    let path = data.path().join("first-admin-link.txt");
    let text = std::fs::read_to_string(&path).expect("the link file is written at start");
    let token = text
        .lines()
        .find_map(|l| {
            l.trim()
                .rsplit_once("/invite/")
                .map(|(_, t)| t.trim().to_string())
        })
        .expect("a link in the file");
    assert!(
        text.contains(&format!("http://{}/invite/{token}", running.addr)),
        "{text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "readable by the service user alone");
    }

    // Used: the administrator exists, the file is gone.
    let session = running.server.core().await.unwrap();
    let signed = session
        .call_as(
            &localspace_core::Caller::system(),
            proto::Request::SetPassword {
                token,
                password: "a root password of length".into(),
                ip: "10.0.0.1".into(),
                user_agent: "test".into(),
                email: Some("root@example.com".into()),
                name: Some("Root".into()),
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(signed, proto::Response::SignedIn { .. }),
        "{signed:?}"
    );
    assert!(!path.exists(), "the file goes when the link is used");
    assert!(running.server.mint_first_admin().await.unwrap().is_none());
    running.task.abort();
}
