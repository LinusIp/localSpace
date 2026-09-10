//! Harness surfaces on their own origins (architecture v2 §6.3).
//!
//! A `web` view runs in an iframe whose origin is the harness's alone:
//! `http://h-<slug>.localhost:<port>` beside a shell at `127.0.0.1:<port>`.
//! Chromium, WebView2 and Firefox resolve `*.localhost` to loopback without
//! DNS, so this costs an organisation nothing on a workstation; a deployment
//! on a real domain sets `surface_hosts` to a wildcard it owns.
//!
//! The shell asks `POST /api/v1/surfaces` for a view and gets a URL carrying
//! a grant: a random token bound to the user, the harness and the view, as
//! the first path segment of everything the frame loads,
//! `/s/<token>/index.js`. No cookie: a browser withholds cookies from a
//! third-party frame, which is exactly what this frame is to the shell. The
//! origin serves the directory of the view's entry module and nothing else:
//! the SDK the shell embeds, the files Core reads through the registry, and
//! a generated page that maps `@localspace/harness-sdk` to the SDK. Every
//! response carries a Content-Security-Policy that lets the surface load
//! only itself, so the bridge to the shell is its one capability.

use crate::Server;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use localspace_proto as proto;
use serde::Deserialize;
use std::sync::Arc;

pub const DEFAULT_HOSTS: &str = "h-{slug}.localhost";
/// Grants beyond this many are dropped oldest first when a new one is
/// minted. A surface that outlives its grant answers 401 on its next file,
/// and the shell reopens it with a fresh URL.
const MAX_GRANTS: usize = 512;

/// The bridge SDK, embedded so the harness origin never depends on where the
/// web bundle is on disk.
pub const SDK_JS: &str = include_str!("../../../web/public/harness-sdk.js");

#[derive(Clone, Debug)]
pub struct Grant {
    pub user: String,
    pub harness: String,
    pub view: String,
    pub slug: String,
    /// The shell origin that opened the view, for `frame-ancestors`.
    pub shell_origin: String,
    pub minted: std::time::Instant,
}

/// `io.localspace.whiteboard` becomes `io-localspace-whiteboard`: one label
/// of a host name, lower case, nothing but letters, digits and hyphens.
pub fn slug(harness: &str) -> String {
    let mut out = String::with_capacity(harness.len());
    for c in harness.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// The host a slug lives on, from the `surface_hosts` pattern.
pub fn host_for(pattern: &str, slug: &str) -> String {
    pattern.replace("{slug}", slug)
}

/// The slug a request's `Host` names, when the host fits the pattern.
pub fn slug_of(pattern: &str, host: &str) -> Option<String> {
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let (prefix, suffix) = pattern.split_once("{slug}")?;
    let rest = host.strip_prefix(prefix)?;
    let slug = rest.strip_suffix(suffix)?;
    if slug.is_empty() || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(slug.to_string())
}

/// `/s/<token>/<rest>` split into the token and the rest, or nothing.
pub fn split_grant_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/s/")?;
    let (token, rest) = rest.split_once('/')?;
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((token, rest))
}

#[derive(Deserialize)]
pub struct Open {
    pub harness: String,
    pub view: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// `POST /api/v1/surfaces`: a URL the shell may put in an iframe for one
/// view of one harness, valid for this user.
pub async fn open(
    State(server): State<Arc<Server>>,
    Query(q): Query<crate::auth::TokenQuery>,
    headers: HeaderMap,
    Json(open): Json<Open>,
) -> Response {
    let token = crate::auth::presented(&headers, q.token.as_deref()).unwrap_or_default();
    let user = server.user_for(&token);
    let session = match server.session(&user).await {
        Ok(s) => s,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Core could not start: {e:#}"),
            )
        }
    };
    // Core says whether the view exists and is a web surface: asking for its
    // entry file is the same check the origin will make on every file.
    match session
        .call(proto::Request::GetSurfaceFile {
            harness: open.harness.clone(),
            view: open.view.clone(),
            path: "index.js".into(),
        })
        .await
    {
        Ok(proto::Response::SurfaceFile { .. }) => {}
        Ok(proto::Response::Error { message }) => return error(StatusCode::NOT_FOUND, message),
        Ok(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "Core answered something else"),
        Err(e) => return error(StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
    }

    let shell_host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1")
        .to_string();
    let origin_header = headers
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .map(|o| o.to_string());
    let scheme = origin_header
        .as_deref()
        .and_then(|o| o.split_once("://").map(|(s, _)| s.to_string()))
        .unwrap_or_else(|| {
            if server.cfg.secure_cookies {
                "https".into()
            } else {
                "http".into()
            }
        });
    let shell_origin = origin_header.unwrap_or_else(|| format!("{scheme}://{shell_host}"));
    let port = shell_host
        .rsplit_once(':')
        .map(|(_, p)| format!(":{p}"))
        .unwrap_or_default();

    let slug = slug(&open.harness);
    let grant_token = uuid::Uuid::new_v4().simple().to_string();
    {
        let mut grants = server.grants.lock().unwrap();
        if grants.len() >= MAX_GRANTS {
            let mut by_age: Vec<(String, std::time::Instant)> =
                grants.iter().map(|(k, g)| (k.clone(), g.minted)).collect();
            by_age.sort_by_key(|(_, t)| *t);
            let excess = grants.len() + 1 - MAX_GRANTS;
            for (k, _) in by_age.into_iter().take(excess) {
                grants.remove(&k);
            }
        }
        grants.insert(
            grant_token.clone(),
            Grant {
                user,
                harness: open.harness.clone(),
                view: open.view.clone(),
                slug: slug.clone(),
                shell_origin,
                minted: std::time::Instant::now(),
            },
        );
    }
    let host = host_for(&server.cfg.surface_hosts, &slug);
    Json(serde_json::json!({
        "url": format!("{scheme}://{host}{port}/s/{grant_token}/"),
        "origin": format!("{scheme}://{host}{port}"),
    }))
    .into_response()
}

/// Middleware in front of everything: a request whose `Host` is a harness
/// origin is answered here and never reaches the API or the shell bundle.
pub async fn route(
    State(server): State<Arc<Server>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    match slug_of(&server.cfg.surface_hosts, &host) {
        Some(slug) => serve(&server, &slug, request).await,
        None => next.run(request).await,
    }
}

/// Locked down to the surface's own origin. Inline styles are allowed
/// because the canvas libraries the whiteboard and CAD surfaces use set
/// them; the only inline script is the import map on the generated page,
/// admitted by a nonce minted for that one response. `frame-ancestors` is
/// the one shell that opened the view.
pub fn csp(shell_origin: &str, nonce: Option<&str>) -> String {
    let inline = nonce.map(|n| format!(" 'nonce-{n}'")).unwrap_or_default();
    // `data:` in connect-src is not a network: a bundler inlines small files
    // (a translation table) as data URLs that the surface then fetches.
    format!(
        "default-src 'none'; script-src 'self'{inline}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; font-src 'self' data:; media-src 'self' blob:; \
         connect-src 'self' data:; worker-src 'self' blob:; base-uri 'none'; form-action 'none'; \
         frame-ancestors {shell_origin}"
    )
}

fn hardened(response: Response, shell_origin: &str) -> Response {
    hardened_with(response, shell_origin, None)
}

fn hardened_with(mut response: Response, shell_origin: &str, nonce: Option<&str>) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp(shell_origin, nonce))
            .unwrap_or_else(|_| HeaderValue::from_static("default-src 'none'")),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

/// The page on the harness origin. Everything in it is relative, so it and
/// the entry module's own imports resolve under the grant's path.
/// What a surface may import by name (v2.1 §6.3): the SDK, React and its
/// JSX runtime and DOM client, the in-house canvas and UI libraries, and
/// Automerge, each one file the shell provides under `_localspace/`.
pub const IMPORT_MAP: &str = concat!(
    r#"{"imports":{"#,
    r#""@localspace/harness-sdk":"./_localspace/sdk.js","#,
    r#""@localspace/canvas":"./_localspace/canvas.js","#,
    r#""@localspace/ui":"./_localspace/ui.js","#,
    r#""react":"./_localspace/react.js","#,
    r#""react/jsx-runtime":"./_localspace/react-jsx-runtime.js","#,
    r#""react-dom/client":"./_localspace/react-dom-client.js","#,
    r#""@automerge/automerge":"./_localspace/automerge.js""#,
    r#"}}"#
);

fn index_html(entry: &str, title: &str, nonce: &str) -> String {
    let title = title.replace('&', "&amp;").replace('<', "&lt;");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<script type="importmap" nonce="{nonce}">{IMPORT_MAP}</script>
<link rel="stylesheet" href="./_localspace/ui.css">
<style>html,body{{margin:0;height:100%;overflow:hidden}}#root{{height:100%}}</style>
</head>
<body>
<div id="root"></div>
<script type="module" src="./{entry}"></script>
</body>
</html>
"#
    )
}

async fn serve(server: &Server, slug: &str, request: Request<Body>) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return hardened(StatusCode::METHOD_NOT_ALLOWED.into_response(), "'none'");
    }
    let path = request.uri().path().to_string();
    let (token, rest) = match split_grant_path(&path) {
        Some(parts) => parts,
        None => {
            return hardened(
                (
                    StatusCode::UNAUTHORIZED,
                    "this surface was not opened by the shell",
                )
                    .into_response(),
                "'none'",
            )
        }
    };
    let grant = server
        .grants
        .lock()
        .unwrap()
        .get(token)
        .cloned()
        .filter(|g| g.slug == slug);
    let Some(grant) = grant else {
        // No grant, or a grant for another harness's origin. The response
        // still carries a policy, so nothing here is ever a blank canvas.
        return hardened(
            (
                StatusCode::UNAUTHORIZED,
                "this surface was not opened by the shell",
            )
                .into_response(),
            "'none'",
        );
    };

    if rest.is_empty() {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let response = (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            index_html("index.js", &grant.view, &nonce),
        )
            .into_response();
        return hardened_with(response, &grant.shell_origin, Some(&nonce));
    }
    if rest == "_localspace/sdk.js" {
        let response = (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            SDK_JS,
        )
            .into_response();
        return hardened(response, &grant.shell_origin);
    }
    // The libraries the shell provides through the import map (v2.1 §6.3):
    // built into the web bundle's `_localspace/` directory, one file each,
    // named by nothing but letters, digits, dots, dashes and underscores.
    if let Some(name) = rest.strip_prefix("_localspace/") {
        let plain = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
            && !name.starts_with('.');
        let file = plain.then(|| server.cfg.web_root.as_ref().map(|root| root.join("_localspace").join(name))).flatten();
        return match file.filter(|f| f.is_file()) {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => hardened(
                    ([(header::CONTENT_TYPE, localspace_core::registry::mime_for(name))], bytes).into_response(),
                    &grant.shell_origin,
                ),
                Err(e) => hardened(
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("reading {name}: {e}")).into_response(),
                    &grant.shell_origin,
                ),
            },
            None => hardened(
                (StatusCode::NOT_FOUND, format!("the shell does not provide `{name}`")).into_response(),
                &grant.shell_origin,
            ),
        };
    }

    let session = match server.session(&grant.user).await {
        Ok(s) => s,
        Err(e) => {
            return hardened(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Core could not start: {e:#}"),
                )
                    .into_response(),
                &grant.shell_origin,
            )
        }
    };
    match session
        .call(proto::Request::GetSurfaceFile {
            harness: grant.harness.clone(),
            view: grant.view.clone(),
            path: rest.to_string(),
        })
        .await
    {
        Ok(proto::Response::SurfaceFile { bytes, mime }) => hardened(
            ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
            &grant.shell_origin,
        ),
        Ok(proto::Response::Error { message }) => hardened(
            (StatusCode::NOT_FOUND, message).into_response(),
            &grant.shell_origin,
        ),
        Ok(_) => hardened(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Core answered something else",
            )
                .into_response(),
            &grant.shell_origin,
        ),
        Err(e) => hardened(
            (StatusCode::SERVICE_UNAVAILABLE, e.to_string()).into_response(),
            &grant.shell_origin,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_one_host_label() {
        assert_eq!(slug("io.localspace.whiteboard"), "io-localspace-whiteboard");
        assert_eq!(slug("Io..Odd_Name"), "io-odd-name");
    }

    #[test]
    fn hosts_round_trip_through_the_pattern() {
        let host = host_for(DEFAULT_HOSTS, "io-localspace-whiteboard");
        assert_eq!(host, "h-io-localspace-whiteboard.localhost");
        assert_eq!(
            slug_of(DEFAULT_HOSTS, &format!("{host}:8443")).as_deref(),
            Some("io-localspace-whiteboard")
        );
        assert_eq!(slug_of(DEFAULT_HOSTS, "127.0.0.1:8443"), None);
        assert_eq!(slug_of(DEFAULT_HOSTS, "h-.localhost"), None);
        assert_eq!(slug_of(DEFAULT_HOSTS, "h-a.b.localhost"), None);
        assert_eq!(
            slug_of("h-{slug}.apps.example.com", "h-cad.apps.example.com").as_deref(),
            Some("cad")
        );
    }

    #[test]
    fn the_grant_is_the_first_path_segment() {
        assert_eq!(split_grant_path("/s/abc123/"), Some(("abc123", "")));
        assert_eq!(
            split_grant_path("/s/abc123/assets/a.css"),
            Some(("abc123", "assets/a.css"))
        );
        assert_eq!(split_grant_path("/s/abc123"), None, "no trailing slash, no page");
        assert_eq!(split_grant_path("/index.js"), None);
        assert_eq!(split_grant_path("/s//index.js"), None);
        assert_eq!(split_grant_path("/s/not-hex!/index.js"), None);
    }

    #[test]
    fn the_policy_names_the_one_shell_that_opened_the_view() {
        let policy = csp("http://127.0.0.1:8443", None);
        assert!(policy.contains("frame-ancestors http://127.0.0.1:8443"));
        assert!(policy.contains("script-src 'self';"), "{policy}");
        assert!(policy.starts_with("default-src 'none'"));
        // The generated page's import map is inline, so that one response
        // admits it by nonce; every file response stays without one.
        let page = csp("http://127.0.0.1:8443", Some("abc"));
        assert!(page.contains("script-src 'self' 'nonce-abc';"), "{page}");
    }

    #[test]
    fn the_index_maps_the_sdk_and_loads_the_entry_module_relatively() {
        let html = index_html("index.js", "Board <web> & more", "n0nce");
        assert!(html.contains(r#"<script type="importmap" nonce="n0nce">"#), "{html}");
        assert!(html.contains(r#""@localspace/harness-sdk":"./_localspace/sdk.js""#));
        assert!(html.contains(r#""@localspace/canvas":"./_localspace/canvas.js""#));
        assert!(html.contains(r#""react":"./_localspace/react.js""#));
        assert!(html.contains(r#"<link rel="stylesheet" href="./_localspace/ui.css">"#), "the UI library's tokens and styles");
        assert!(html.contains(r#"<script type="module" src="./index.js">"#));
        assert!(html.contains("<title>Board &lt;web> &amp; more</title>"));
        let map: serde_json::Value = serde_json::from_str(IMPORT_MAP).expect("the import map is JSON");
        assert_eq!(map["imports"].as_object().map(|m| m.len()), Some(7));
    }
}
