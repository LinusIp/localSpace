//! A stand-in for `llama-server`, for tests of the sidecar supervisor: it
//! takes the same flags, answers `/health` once it has "loaded", and answers
//! `/v1/chat/completions` with one fixed reply, streamed as server-sent events
//! when the request asks, as chat turns in Core do. It knows nothing about models
//! and is not shipped as anything but a test double.
//!
//! `FAKE_LLAMA_LOAD_MS` delays readiness; `FAKE_LLAMA_CRASH_AFTER_MS` makes
//! it exit after that long, so restarts can be tested. Given `LLAMA_API_KEY`,
//! it answers 401 to anything but `/health` that does not present the key,
//! as llama-server does. A model stub that begins `fits N layers` makes it
//! give up while loading when `-ngl` asks for more, as llama-server does on a
//! graphics card that refuses; one that contains `chat fails` makes every
//! chat completion answer 500; one that contains `loads in N ms` is slow to
//! load, as a large model is, without a variable every test would share.
//! How a streamed answer comes is said by the stub too: `streams slowly`
//! (twenty words, a pause between each), `stalls before the first piece`,
//! `stalls after N pieces`, `dies after N pieces` (the connection closes
//! mid-answer), `writes in Russian` (every event's bytes in two writes, split
//! inside a letter). An answer begun for it (the last message is the
//! assistant's) is carried on, its words said again first, as llama-server's
//! prefill does. Asked for `--list-devices`, it lists none and ends, as
//! llama-server does on a computer with no graphics card, so that a server
//! run on it looks at the machine without waiting. Each request is named in
//! the log.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// How the answers of this start come, from its model stub.
#[derive(Clone, Default)]
struct Style {
    chat_fails: bool,
    slowly: bool,
    stall_before_first: bool,
    stall_after: Option<usize>,
    die_after: Option<usize>,
    russian: bool,
}

impl Style {
    fn of(stub: &str) -> Style {
        let count = |phrase: &str| {
            stub.split(phrase)
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        };
        Style {
            chat_fails: stub.contains("chat fails"),
            slowly: stub.contains("streams slowly"),
            stall_before_first: stub.contains("stalls before the first piece"),
            stall_after: count("stalls after "),
            die_after: count("dies after "),
            russian: stub.contains("writes in Russian"),
        }
    }
}

/// Long enough for any test to have given up on the answer.
const STALL: Duration = Duration::from_secs(600);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list-devices") {
        let _ = write!(std::io::stdout(), "Available devices:\n  (none)\n");
        return;
    }
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
    let named = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let stub = named("-m")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let style = Style::of(&stub);
    let load_ms: u64 = stub
        .split("loads in ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        .unwrap_or(load_ms);
    let fits: Option<u32> = Some(stub.as_str()).and_then(|text| {
        text.strip_prefix("fits ")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    });
    let asked: Option<u32> = named("-ngl").and_then(|n| n.parse().ok());
    if let (Some(fits), Some(asked)) = (fits, asked)
        && asked > fits
    {
        eprintln!("fake llama-server: {asked} layers do not fit; giving up");
        std::process::exit(1);
    }
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
        let style = style.clone();
        std::thread::spawn(move || handle(stream, ready, &alias, &style));
    }
}

fn handle(mut stream: TcpStream, ready: bool, alias: &str, style: &Style) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    if !request_line.contains("/health") {
        eprintln!("fake llama-server: request {}", request_line.trim());
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

    if style.chat_fails && path.starts_with("/v1/chat/completions") {
        let json = r#"{"error":{"code":500,"message":"on purpose","type":"server_error"}}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
            json.len()
        );
        return;
    }

    // Core's chat turns ask to stream: answer as llama-server does, in
    // server-sent events, the reply in two pieces, then the usage, then the end.
    let request = serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default();
    let streamed = request["stream"].as_bool().unwrap_or(false);
    let begun = request["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .filter(|last| last["role"] == "assistant")
        .and_then(|last| last["content"].as_str())
        .unwrap_or_default()
        .to_string();
    if streamed && path.starts_with("/v1/chat/completions") {
        let chunk = |delta: &str, finish: &str| {
            format!(
                r#"data: {{"id":"chatcmpl-fake","object":"chat.completion.chunk","model":"{alias}","choices":[{{"index":0,"delta":{delta},"finish_reason":{finish}}}]}}"#
            )
        };
        let pieces: Vec<String> = if style.russian {
            ["Привет, ", "мир! ", "Как ", "дела?"]
                .map(String::from)
                .to_vec()
        } else if style.slowly || style.stall_after.is_some() || style.die_after.is_some() {
            (1..=20).map(|i| format!("word{i} ")).collect()
        } else {
            ["hello ", "from the fake engine"]
                .map(String::from)
                .to_vec()
        };
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.flush();
        let _ = stream.set_nodelay(true);
        if style.stall_before_first {
            std::thread::sleep(STALL);
            return;
        }
        for (i, piece) in pieces.iter().enumerate() {
            if style.die_after == Some(i) {
                eprintln!("fake llama-server: going away mid-answer");
                return;
            }
            if style.stall_after == Some(i) {
                std::thread::sleep(STALL);
                return;
            }
            let delta = if i == 0 {
                serde_json::json!({"role": "assistant", "content": format!("{begun}{piece}")})
            } else {
                serde_json::json!({"content": piece})
            };
            let event = format!("{}\n\n", chunk(&delta.to_string(), "null")).into_bytes();
            if style.russian {
                // The first byte of the first letter written in two bytes,
                // then the rest: one letter split between two writes.
                let at = event
                    .iter()
                    .position(|&b| b >= 0xC0)
                    .map_or(event.len(), |at| at + 1);
                if stream.write_all(&event[..at]).is_err() {
                    return;
                }
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(30));
                if stream.write_all(&event[at..]).is_err() {
                    return;
                }
            } else if stream.write_all(&event).is_err() {
                // The reader went away: a stop, as the real engine sees one.
                eprintln!("fake llama-server: the reader went away");
                return;
            }
            let _ = stream.flush();
            if style.slowly {
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        let tail = [
            chunk("{}", r#""stop""#),
            format!(
                r#"data: {{"id":"chatcmpl-fake","object":"chat.completion.chunk","model":"{alias}","choices":[],"usage":{{"prompt_tokens":12,"completion_tokens":{},"total_tokens":18}}}}"#,
                pieces.len()
            ),
            "data: [DONE]".to_string(),
        ];
        for event in tail {
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
