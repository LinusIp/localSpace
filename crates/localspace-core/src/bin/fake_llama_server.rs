//! A stand-in for `llama-server`, for tests of the sidecar supervisor: it
//! takes the same flags, answers `/health` once it has "loaded", and answers
//! `/v1/chat/completions` with one fixed reply, streamed as server-sent events
//! when the request asks, as chat turns in Core do. It knows nothing about models
//! and is not shipped as anything but a test double.
//!
//! `FAKE_LLAMA_LOAD_MS` delays readiness; `FAKE_LLAMA_CRASH_AFTER_MS` makes
//! it exit after that long, so restarts can be tested. Given `LLAMA_API_KEY`,
//! it answers 401 to anything but `/health` that does not present the key,
//! as llama-server does.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = args
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(8080);
    let alias = args
        .windows(2)
        .find(|w| w[0] == "--alias")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "fake".into());
    let load_ms: u64 = std::env::var("FAKE_LLAMA_LOAD_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let crash_ms: Option<u64> = std::env::var("FAKE_LLAMA_CRASH_AFTER_MS")
        .ok()
        .and_then(|s| s.parse().ok());

    eprintln!("fake llama-server: args {:?}", &args[1..]);
    let started = Instant::now();
    if let Some(ms) = crash_ms {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            eprintln!("fake llama-server: crashing on purpose");
            std::process::exit(3);
        });
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("binding the port given");
    for stream in listener.incoming().flatten() {
        let ready = started.elapsed() >= Duration::from_millis(load_ms);
        let alias = alias.clone();
        std::thread::spawn(move || handle(stream, ready, &alias));
    }
}

fn handle(mut stream: TcpStream, ready: bool, alias: &str) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut content_length = 0usize;
    let mut presented: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line == "\n" || line.is_empty()
        {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
        if line.to_ascii_lowercase().starts_with("authorization:") {
            presented = line
                .split_once(':')
                .map(|(_, v)| v.trim().trim_start_matches("Bearer ").to_string());
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");

    if let Ok(key) = std::env::var("LLAMA_API_KEY")
        && !path.starts_with("/health")
        && presented.as_deref() != Some(key.as_str())
    {
        let json =
            r#"{"error":{"code":401,"message":"Invalid API Key","type":"authentication_error"}}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
            json.len()
        );
        return;
    }

    // Core's chat turns ask to stream: answer as llama-server does, in
    // server-sent events, the reply in two pieces, then the usage, then the end.
    let streamed = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);
    if streamed && path.starts_with("/v1/chat/completions") {
        let chunk = |delta: &str, finish: &str| {
            format!(
                r#"data: {{"id":"chatcmpl-fake","object":"chat.completion.chunk","model":"{alias}","choices":[{{"index":0,"delta":{delta},"finish_reason":{finish}}}]}}"#
            )
        };
        let events = [
            chunk(r#"{"role":"assistant","content":"hello "}"#, "null"),
            chunk(r#"{"content":"from the fake engine"}"#, "null"),
            chunk("{}", r#""stop""#),
            format!(
                r#"data: {{"id":"chatcmpl-fake","object":"chat.completion.chunk","model":"{alias}","choices":[],"usage":{{"prompt_tokens":12,"completion_tokens":6,"total_tokens":18}}}}"#
            ),
            "data: [DONE]".to_string(),
        ];
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
        );
        for event in events {
            let _ = write!(stream, "{event}\n\n");
        }
        let _ = stream.flush();
        return;
    }

    let (status, json) = if path.starts_with("/health") {
        if ready {
            (200, r#"{"status":"ok"}"#.to_string())
        } else {
            (
                503,
                r#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#
                    .to_string(),
            )
        }
    } else if path.starts_with("/v1/models") {
        (
            200,
            format!(r#"{{"object":"list","data":[{{"id":"{alias}","object":"model"}}]}}"#),
        )
    } else if path.starts_with("/v1/chat/completions") {
        (
            200,
            format!(
                r#"{{"id":"chatcmpl-fake","object":"chat.completion","model":"{alias}","choices":[{{"index":0,"message":{{"role":"assistant","content":"hello from the fake engine"}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":12,"completion_tokens":6,"total_tokens":18}}}}"#
            ),
        )
    } else {
        (404, r#"{"error":"not here"}"#.to_string())
    };
    let reason = match status {
        200 => "OK",
        503 => "Service Unavailable",
        _ => "Not Found",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
        json.len()
    );
    let _ = stream.flush();
}
