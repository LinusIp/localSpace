//! Model workers and the router in front of them.
//!
//! One internal worker trait; backends sit behind it. The backend shipped here
//! speaks the OpenAI-compatible HTTP API, which is what llama.cpp's server,
//! LM Studio, mistral.rs, vLLM and SGLang all expose — so a real local model is a
//! URL away, and Core keeps no code dependency on any of them.
//!
//! The router implements §16.1's third rule: a resident small model takes the
//! calls that do not need reasoning. Conversation turns and agent steps go to the
//! chat worker; titling, summarisation, compaction, `find_capability` reranking
//! and non-reasoning harness calls go to the utility worker. The split is on
//! `/metrics`, and a deployment where the big model takes more than 60 % of calls
//! is misconfigured.

use crate::prompt::Turn;
use crate::stream::{Cut, Silence, Stop};
use anyhow::{Context, Result, bail};
use localspace_proto as proto;
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Request shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    /// A user is waiting.
    Interactive,
    /// Agent sub-steps, harness calls, summarisation.
    Background,
    Ingestion,
}

impl RequestClass {
    pub fn priority(self) -> u8 {
        match self {
            RequestClass::Interactive => 1,
            RequestClass::Background => 3,
            RequestClass::Ingestion => 5,
        }
    }
}

/// Which resident model a request belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRole {
    Chat,
    Utility,
    Embedding,
}

/// What a request puts before the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    /// One message of the person's: a harness's own question to the model.
    Plain(String),
    /// A step of the agent: the instructions as a system message, the
    /// conversation as turns, and the task ledger after the newest message
    /// (docs/DECISIONS.md, 2026-09-24 and 2026-10-06). Where the ledger goes
    /// is the worker's: see [`LedgerPlace`].
    Turns {
        system: String,
        turns: Vec<Turn>,
        ledger: String,
    },
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub said: Said,
    pub tools: Vec<proto::ExposedTool>,
    /// GBNF for backends that accept one; ignored by those that do not.
    pub grammar: Option<String>,
    /// The words of an answer that stopped, when it is carried on: they are
    /// the start of the model's reply, which it continues, and only what it
    /// adds comes back.
    pub begun: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub class: RequestClass,
}

impl ChatRequest {
    pub fn new(prompt: String) -> ChatRequest {
        ChatRequest::saying(Said::Plain(prompt))
    }

    /// A step of the agent: see [`Said::Turns`].
    pub fn with_turns(system: String, turns: Vec<Turn>, ledger: String) -> ChatRequest {
        ChatRequest::saying(Said::Turns {
            system,
            turns,
            ledger,
        })
    }

    fn saying(said: Said) -> ChatRequest {
        ChatRequest {
            said,
            tools: Vec::new(),
            grammar: None,
            begun: None,
            max_tokens: 1024,
            temperature: 0.2,
            class: RequestClass::Interactive,
        }
    }

    /// The conversation's turns; none for a plain prompt.
    pub fn conversation(&self) -> &[Turn] {
        match &self.said {
            Said::Plain(_) => &[],
            Said::Turns { turns, .. } => turns,
        }
    }

    /// Everything the request puts before the model, as text and in the
    /// order it is read, the ledger last: for a person reading it, and for
    /// tests. The engine is sent messages, never this.
    pub fn text(&self) -> String {
        match &self.said {
            Said::Plain(text) => text.clone(),
            Said::Turns {
                system,
                turns,
                ledger,
            } => {
                let mut parts = vec![system.clone()];
                parts.extend(turns.iter().map(Turn::render));
                if !ledger.is_empty() {
                    parts.push(ledger.clone());
                }
                parts.join("\n\n")
            }
        }
    }
}

/// Where the task ledger goes among the messages of an agent's step,
/// decided per model when it loads by rendering a short conversation through
/// the model's own template (`/apply-template`; docs/DECISIONS.md,
/// 2026-09-24, document 25, and 2026-10-06, document 27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedgerPlace {
    /// A system message of its own after the newest message: two turns'
    /// prompts match up to the new message.
    AfterTheNewest,
    /// At the end of the person's newest message, for a template that takes
    /// no system message but the first: two turns' prompts match up to the
    /// start of the previous message, and one exchange is read again each
    /// turn. Any template takes it, so it is where a model that was never
    /// asked has it.
    #[default]
    InTheNewest,
}

impl LedgerPlace {
    /// For the log.
    pub fn said(&self) -> &'static str {
        match self {
            LedgerPlace::AfterTheNewest => "as a system message after the newest message",
            LedgerPlace::InTheNewest => "at the end of the person's newest message",
        }
    }
}

/// What a test conversation proves of a template: the ledger arrived as a
/// system message after the person's message. The words are odd enough not
/// to occur in a template by themselves.
const LEDGER_PROBE: &str = "[task probe] notes: the ledger arrived";

/// Where this engine's model takes the ledger: a three-message conversation
/// with it as a system message after the person's is rendered by the
/// engine's `/apply-template`. Refused, or with the ledger missing from what
/// comes back, it goes into the person's newest message instead.
/// `root_url` is the engine's own address, without `/v1`.
pub fn ledger_place(root_url: &str, api_key: Option<&str>) -> (LedgerPlace, String) {
    let url = format!("{}/apply-template", root_url.trim_end_matches('/'));
    let body = json!({"messages": [
        {"role": "system", "content": "You are the assistant."},
        {"role": "user", "content": "Hello."},
        {"role": "system", "content": LEDGER_PROBE}
    ]});
    let mut request = ureq::post(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    if let Some(key) = api_key {
        request = request.header("Authorization", &format!("Bearer {key}"));
    }
    let rendered: Result<J> = request
        .send_json(&body)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|mut res| {
            res.body_mut()
                .read_json::<J>()
                .map_err(|e| anyhow::anyhow!("{e}"))
        });
    match rendered {
        Ok(answer) => match answer["prompt"].as_str() {
            Some(prompt) if prompt.contains(LEDGER_PROBE) => (
                LedgerPlace::AfterTheNewest,
                "its template keeps a system message after the person's".to_string(),
            ),
            Some(_) => (
                LedgerPlace::InTheNewest,
                "its template leaves out a system message after the person's".to_string(),
            ),
            None => (
                LedgerPlace::InTheNewest,
                "the engine rendered no prompt for a test conversation".to_string(),
            ),
        },
        Err(e) => (
            LedgerPlace::InTheNewest,
            format!("its template refused a system message after the person's ({e})"),
        ),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatReply {
    pub text: String,
    pub calls: Vec<ProposedCall>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ProposedCall {
    pub id: String,
    pub tool: String,
    pub params: J,
}

pub trait ModelWorker: Send + Sync {
    fn info(&self) -> proto::ModelInfo;
    fn chat(&self, req: &ChatRequest) -> Result<ChatReply>;
    /// Like `chat`, but each piece of text reaches `on_delta` as the model
    /// produces it (v2 step 3). A backend that cannot stream answers whole.
    fn chat_streaming(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
        let reply = self.chat(req)?;
        if !reply.text.is_empty() {
            on_delta(&reply.text);
        }
        Ok(reply)
    }
    /// `chat_streaming`, which `stop` ends from another thread and which
    /// ends by itself when the model is silent for longer than `silence`
    /// allows; an answer that ends early is an error caused by a [`Cut`].
    /// A backend that cannot be stopped part-way is stopped when its answer
    /// comes whole, and nothing of it is passed on.
    fn chat_streaming_until(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
        stop: &Stop,
        _silence: Silence,
    ) -> Result<ChatReply> {
        let reply = self.chat_streaming(req, &mut |piece| {
            if !stop.asked() {
                on_delta(piece);
            }
        })?;
        if stop.asked() {
            return Err(Cut::Stopped.into());
        }
        Ok(reply)
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

// ---------------------------------------------------------------------------
// OpenAI-compatible worker
// ---------------------------------------------------------------------------

pub struct OpenAiWorker {
    base_url: String,
    model: String,
    api_key: Option<String>,
    context_len: u32,
    supports_vision: bool,
    timeout: Duration,
    ledger: LedgerPlace,
}

impl OpenAiWorker {
    pub fn new(base_url: &str, model: &str) -> OpenAiWorker {
        OpenAiWorker {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: None,
            context_len: 32768,
            supports_vision: false,
            timeout: Duration::from_secs(180),
            ledger: LedgerPlace::default(),
        }
    }

    pub fn with_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    /// Where this model takes the task ledger: see [`ledger_place`].
    pub fn with_ledger(mut self, place: LedgerPlace) -> Self {
        self.ledger = place;
        self
    }

    pub fn with_context_len(mut self, n: u32) -> Self {
        self.context_len = n;
        self
    }

    /// How long one request may take; three minutes unless said otherwise.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// A model behind TLS, connected under Advanced: `ureq` reads it, stopped
    /// at its next piece, with no limit on how long a model that is writing
    /// may take, only on the wait for its first word.
    fn read_over_tls(
        &self,
        url: &str,
        body: &J,
        silence: Silence,
        stop: &Stop,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
        let mut request = ureq::post(url)
            .config()
            .timeout_global(None)
            .timeout_recv_response(Some(silence.before_first_word))
            .build();
        if let Some(k) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {k}"));
        }
        let mut res = request
            .send_json(body)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("POST {url}"))?;
        let reader = std::io::BufReader::new(res.body_mut().as_reader());
        read_sse(reader, on_delta, stop)
    }

    /// Ask the endpoint what it has loaded. Used by the model picker.
    pub fn list_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let mut req = ureq::get(&url);
        if let Some(k) = api_key {
            req = req.header("Authorization", &format!("Bearer {k}"));
        }
        let body: J = req
            .call()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("GET {url}"))?
            .body_mut()
            .read_json()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(body["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn post(&self, path: &str, body: J) -> Result<J> {
        let url = format!("{}{path}", self.base_url);
        let mut req = ureq::post(&url)
            .config()
            .timeout_global(Some(self.timeout))
            .build();
        if let Some(k) = &self.api_key {
            req = req.header("Authorization", &format!("Bearer {k}"));
        }
        let mut res = req
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("POST {url}"))?;
        res.body_mut()
            .read_json()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("decoding the model response")
    }
}

impl ModelWorker for OpenAiWorker {
    fn info(&self) -> proto::ModelInfo {
        proto::ModelInfo {
            id: self.model.clone(),
            backend: format!("openai-compatible @ {}", self.base_url),
            context_len: self.context_len,
            supports_tools: true,
            supports_vision: self.supports_vision,
            loaded: true,
        }
    }

    fn chat(&self, req: &ChatRequest) -> Result<ChatReply> {
        let mut body = json!({
            "model": self.model,
            "messages": messages(req, self.ledger),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": false,
        });
        if !req.tools.is_empty() {
            body["tools"] = J::Array(req.tools.iter().map(tool_schema).collect());
            body["tool_choice"] = json!("auto");
        }

        let res = self.post("/chat/completions", body)?;
        let mut again = SaidAgain::new(req.begun.as_deref().unwrap_or_default());
        Ok(again.carried_on(parse_openai_reply(&res)?))
    }

    fn chat_streaming(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
        self.chat_streaming_until(req, on_delta, &Stop::default(), Silence::ANSWER)
    }

    fn chat_streaming_until(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
        stop: &Stop,
        silence: Silence,
    ) -> Result<ChatReply> {
        let mut body = json!({
            "model": self.model,
            "messages": messages(req, self.ledger),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if !req.tools.is_empty() {
            body["tools"] = J::Array(req.tools.iter().map(tool_schema).collect());
            body["tool_choice"] = json!("auto");
        }
        let url = format!("{}/chat/completions", self.base_url);
        let mut again = SaidAgain::new(req.begun.as_deref().unwrap_or_default());
        let read = {
            let mut pass = |piece: &str| again.pass(piece, on_delta);
            if url.starts_with("http://") {
                // The engine, and a model on the organisation's network: read
                // on a socket of our own, which a stop shuts and a silence ends.
                crate::stream::post_and_read(
                    &url,
                    self.api_key.as_deref(),
                    &body,
                    silence,
                    stop,
                    &mut pass,
                )
            } else {
                self.read_over_tls(&url, &body, silence, stop, &mut pass)
            }
        };
        // Whatever was held back as perhaps the handed words, said again, is
        // passed on however the answer ended, unless it was exactly them.
        again.finish(on_delta);
        Ok(again.carried_on(read?))
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let res = self.post("/embeddings", json!({"model": self.model, "input": texts}))?;
        let data = res["data"].as_array().context("no embedding data")?;
        Ok(data
            .iter()
            .map(|d| {
                d["embedding"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect())
    }
}

fn tool_schema(t: &proto::ExposedTool) -> J {
    json!({
        "type": "function",
        "function": {
            "name": t.name.replace('.', "__"),
            "description": t.summary,
            "parameters": t.params.0,
        }
    })
}

pub fn parse_openai_reply(res: &J) -> Result<ChatReply> {
    if let Some(err) = res.get("error") {
        bail!(
            "model returned an error: {}",
            err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        );
    }
    let choice = res["choices"].get(0).context("model returned no choices")?;
    let message = &choice["message"];
    let text = message["content"].as_str().unwrap_or_default().to_string();

    let mut calls = Vec::new();
    if let Some(tc) = message["tool_calls"].as_array() {
        for (i, c) in tc.iter().enumerate() {
            let name = c["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .replace("__", ".");
            let raw = c["function"]["arguments"].as_str().unwrap_or("{}");
            let params: J = serde_json::from_str(raw).unwrap_or(json!({}));
            calls.push(ProposedCall {
                id: c["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("call_{i}")),
                tool: name,
                params,
            });
        }
    }

    // Backends without native tool calling emit the grammar's shape as content.
    if calls.is_empty()
        && let Some(c) = parse_grammar_call(&text)
    {
        calls.push(c);
    }

    Ok(ChatReply {
        text,
        calls,
        prompt_tokens: res["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: res["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
    })
}

/// The messages of a request. A plain prompt is one message of the person's.
/// An agent's step is the instructions as a system message, then the
/// conversation as turns, a call as the model's own call and the tool's reply,
/// in the shape the model's template gives them, then the ledger where this
/// model takes it. When an answer that stopped is carried on, its words come
/// last, as the start of the model's reply: the engine continues a last
/// message of the assistant's instead of answering after it (llama-server's
/// prefill, on by default).
fn messages(req: &ChatRequest, ledger_place: LedgerPlace) -> J {
    let mut out: Vec<J> = Vec::new();
    match &req.said {
        Said::Plain(text) => out.push(json!({"role": "user", "content": text})),
        Said::Turns {
            system,
            turns,
            ledger,
        } => {
            out.push(json!({"role": "system", "content": system}));
            let newest = turns.iter().rposition(|t| matches!(t, Turn::Person(_)));
            for (i, turn) in turns.iter().enumerate() {
                match turn {
                    Turn::Person(text) => {
                        let content = if ledger_place == LedgerPlace::InTheNewest
                            && Some(i) == newest
                            && !ledger.is_empty()
                        {
                            format!("{text}\n\n{ledger}")
                        } else {
                            text.clone()
                        };
                        out.push(json!({"role": "user", "content": content}));
                    }
                    Turn::Answer(text) => out.push(json!({"role": "assistant", "content": text})),
                    Turn::Call {
                        id,
                        tool,
                        params,
                        result,
                    } => {
                        out.push(json!({
                            "role": "assistant",
                            "content": "",
                            "tool_calls": [{
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": tool.replace('.', "__"),
                                    "arguments": params.to_string()
                                }
                            }]
                        }));
                        out.push(json!({"role": "tool", "tool_call_id": id, "content": result}));
                    }
                }
            }
            if ledger_place == LedgerPlace::AfterTheNewest && !ledger.is_empty() {
                out.push(json!({"role": "system", "content": ledger}));
            }
        }
    }
    if let Some(begun) = req.begun.as_deref().filter(|b| !b.is_empty()) {
        out.push(json!({"role": "assistant", "content": begun}));
    }
    J::Array(out)
}

/// An answer carried on, as it comes back. llama-server (b10869) begins its
/// stream with the words it was handed, before what the model adds; that is
/// its behaviour, not a promise, and a new pin may change it. The words are
/// dropped only when the stream begins with exactly them, never by length:
/// a wrong guess would cut real words out of an answer with nobody noticing
/// (docs/DECISIONS.md, 2026-09-24). When it does not, nothing is dropped and
/// the log says so, once.
struct SaidAgain<'a> {
    begun: &'a str,
    /// What came while it could still be the handed words, said again.
    held: String,
    seed: Seed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seed {
    /// Nothing was handed over.
    None,
    /// Not yet known whether the engine says them again.
    Waiting,
    /// It did, exactly: they are not passed on.
    SaidAgain,
    /// It did not: everything is passed on.
    NotSaidAgain,
}

impl<'a> SaidAgain<'a> {
    fn new(begun: &'a str) -> SaidAgain<'a> {
        SaidAgain {
            begun,
            held: String::new(),
            seed: if begun.is_empty() {
                Seed::None
            } else {
                Seed::Waiting
            },
        }
    }

    fn pass(&mut self, piece: &str, on_delta: &mut dyn FnMut(&str)) {
        if self.seed != Seed::Waiting {
            on_delta(piece);
            return;
        }
        self.held.push_str(piece);
        // Perhaps still the handed words, said again: wait for more.
        if self.held.len() < self.begun.len() && self.begun.starts_with(self.held.as_str()) {
            return;
        }
        let held = std::mem::take(&mut self.held);
        match held.strip_prefix(self.begun) {
            Some(added) => {
                self.seed = Seed::SaidAgain;
                if !added.is_empty() {
                    on_delta(added);
                }
            }
            None => {
                self.not_said_again();
                on_delta(&held);
            }
        }
    }

    /// The stream is over: what was held back was not the handed words
    /// after all, only as far as they went, and is passed on.
    fn finish(&mut self, on_delta: &mut dyn FnMut(&str)) {
        if self.seed == Seed::Waiting && !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            self.not_said_again();
            on_delta(&held);
        }
    }

    /// The whole reply, with what the model added as its text: the handed
    /// words come off only where they were said again, exactly. A reply
    /// that came whole is looked at here.
    fn carried_on(&mut self, mut reply: ChatReply) -> ChatReply {
        if self.seed == Seed::Waiting {
            if reply.text.starts_with(self.begun) {
                self.seed = Seed::SaidAgain;
            } else {
                self.not_said_again();
            }
        }
        if self.seed == Seed::SaidAgain
            && let Some(added) = reply.text.strip_prefix(self.begun)
        {
            reply.text = added.to_string();
        }
        reply
    }

    fn not_said_again(&mut self) {
        self.seed = Seed::NotSaidAgain;
        tracing::info!(
            "continue: the engine did not begin its reply with the {} characters it was handed; nothing was dropped",
            self.begun.chars().count()
        );
    }
}

/// An OpenAI-style server-sent event stream, read to the end: text deltas go
/// to `on_delta` as they arrive, tool-call deltas are assembled by index, and
/// the last event's usage is kept.
///
/// Lines are cut from buffered bytes before they are decoded, so a character
/// whose bytes came in separate reads is whole. `stop` is looked at every
/// line. A stream that ends before the model said it was finished (`[DONE]`,
/// or a reason for finishing) was cut, and says so: [`Cut::Lost`].
pub fn read_sse<R: std::io::BufRead>(
    reader: R,
    on_delta: &mut dyn FnMut(&str),
    stop: &Stop,
) -> Result<ChatReply> {
    let mut text = String::new();
    // (id, name, arguments) per tool-call index; arguments arrive in pieces.
    let mut calls: Vec<(String, String, String)> = Vec::new();
    let mut prompt_tokens = 0u32;
    let mut completion_tokens = 0u32;
    let mut finished = false;
    for line in reader.lines() {
        if stop.asked() {
            return Err(Cut::Stopped.into());
        }
        // The read error itself is kept, for a reader that tells a silence
        // from a lost connection by it.
        let line = line.context("reading the model's stream")?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            finished = true;
            break;
        }
        let Ok(event) = serde_json::from_str::<J>(data) else {
            continue;
        };
        if let Some(err) = event.get("error") {
            bail!(
                "model returned an error: {}",
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
            );
        }
        if let Some(usage) = event.get("usage").filter(|u| !u.is_null()) {
            prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as u32;
            completion_tokens = usage["completion_tokens"].as_u64().unwrap_or(0) as u32;
        }
        let Some(choice) = event["choices"].get(0) else {
            continue;
        };
        if choice["finish_reason"].is_string() {
            finished = true;
        }
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str()
            && !piece.is_empty()
        {
            text.push_str(piece);
            on_delta(piece);
        }
        if let Some(pieces) = delta["tool_calls"].as_array() {
            for tc in pieces {
                let index = tc["index"].as_u64().unwrap_or(0) as usize;
                while calls.len() <= index {
                    calls.push((String::new(), String::new(), String::new()));
                }
                if let Some(id) = tc["id"].as_str() {
                    calls[index].0 = id.to_string();
                }
                if let Some(name) = tc["function"]["name"].as_str() {
                    calls[index].1.push_str(name);
                }
                if let Some(args) = tc["function"]["arguments"].as_str() {
                    calls[index].2.push_str(args);
                }
            }
        }
    }

    if stop.asked() {
        return Err(Cut::Stopped.into());
    }
    if !finished {
        return Err(Cut::Lost.into());
    }
    let mut proposed = Vec::new();
    for (i, (id, name, args)) in calls.into_iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let params: J = serde_json::from_str(&args).unwrap_or(json!({}));
        proposed.push(ProposedCall {
            id: if id.is_empty() {
                format!("call_{i}")
            } else {
                id
            },
            tool: name.replace("__", "."),
            params,
        });
    }
    if proposed.is_empty()
        && let Some(c) = parse_grammar_call(&text)
    {
        proposed.push(c);
    }
    Ok(ChatReply {
        text,
        calls: proposed,
        prompt_tokens,
        completion_tokens,
    })
}

/// `{"tool": "...", "params": {...}}` — what the GBNF in `grammar.rs` admits.
pub fn parse_grammar_call(text: &str) -> Option<ProposedCall> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    let v: J = serde_json::from_str(&text[start..=end]).ok()?;
    let tool = v.get("tool")?.as_str()?.to_string();
    Some(ProposedCall {
        id: "call_0".into(),
        tool,
        params: v.get("params").cloned().unwrap_or(json!({})),
    })
}

/// A reply that is nothing but a tool call in the model's own shape,
/// `{"name": "...", "arguments": {...}}`, read as the call it is.
///
/// The engine reads that shape itself when the model wraps it in its tags.
/// Once in the nineteen turns of the message script the 14B left the tags
/// out, the engine handed the JSON back as the answer, and a person was shown
/// `{"name": "task.note", "arguments": …}` (docs/DECISIONS.md, 2026-09-19).
/// Read narrowly, so that JSON a person asked for is never taken for a call:
/// the **entire** reply is one object, it has these two members and no
/// other, and the name is a tool **on offer in this very request**, written
/// with its dot or as the engine is given it.
pub fn parse_bare_call(text: &str, tools: &[proto::ExposedTool]) -> Option<ProposedCall> {
    let value: J = serde_json::from_str(text.trim()).ok()?;
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let name = object.get("name")?.as_str()?.replace("__", ".");
    let params = match object.get("arguments")? {
        J::Object(arguments) => J::Object(arguments.clone()),
        // As the OpenAI shape carries them: JSON inside a string.
        J::String(inside) => serde_json::from_str::<J>(inside)
            .ok()
            .filter(|v| v.is_object())?,
        _ => return None,
    };
    tools.iter().any(|t| t.name == name).then(|| ProposedCall {
        id: "call_0".into(),
        tool: name,
        params,
    })
}

/// The tool, and its arguments, of a reply that is, or begins with, a block
/// in the shape of a tool call: `name` and `arguments`, or Core's own `tool`
/// and `params`, bare or in a code fence. A person never sees one as an
/// answer (docs/DECISIONS.md, 2026-10-07): one that names a tool not on
/// offer is an attempted call, refused to the model (see `agent`). Asked for
/// a translation with the whiteboard installed, the 14B wrote
/// `{"name": "translate_text", "arguments": {…}}`, for a tool that does not
/// exist.
pub fn call_shaped(text: &str) -> Option<(String, J)> {
    let mut rest = text.trim_start();
    if let Some(fenced) = rest.strip_prefix("```") {
        rest = fenced
            .trim_start_matches(|c: char| c.is_ascii_alphanumeric())
            .trim_start();
    }
    if !rest.starts_with('{') {
        return None;
    }
    // The first value only: what follows it may be anything.
    let value = serde_json::Deserializer::from_str(rest)
        .into_iter::<J>()
        .next()?
        .ok()?;
    // Read narrowly, as `parse_bare_call` is: two members and no other, a
    // name that is an identifier, and arguments that are an object.
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let (name, arguments) = match (object.get("name"), object.get("arguments")) {
        (Some(name), Some(arguments)) => (name, arguments),
        _ => (object.get("tool")?, object.get("params")?),
    };
    let name = name.as_str()?;
    let identifier = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    let arguments = match arguments {
        J::Object(_) => arguments.clone(),
        J::String(inside) => serde_json::from_str::<J>(inside)
            .ok()
            .filter(J::is_object)?,
        _ => return None,
    };
    identifier.then(|| (name.to_string(), arguments))
}

/// Every worker's reply passes here: see [`parse_bare_call`].
fn with_a_bare_call_read(mut reply: ChatReply, req: &ChatRequest) -> ChatReply {
    if reply.calls.is_empty()
        && let Some(call) = parse_bare_call(&reply.text, &req.tools)
    {
        reply.calls.push(call);
    }
    reply
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Metrics {
    pub chat_calls: AtomicU64,
    pub utility_calls: AtomicU64,
    pub malformed_tool_calls: AtomicU64,
    pub tool_calls: AtomicU64,
    pub prompt_tokens: AtomicU64,
}

impl Metrics {
    /// §16.5 budget: at least 40 % of calls should land on the utility model.
    pub fn utility_share(&self) -> f32 {
        let u = self.utility_calls.load(Ordering::Relaxed) as f32;
        let c = self.chat_calls.load(Ordering::Relaxed) as f32;
        if u + c == 0.0 { 0.0 } else { u / (u + c) }
    }

    /// §16.5 budget: under 0.5 %.
    pub fn malformed_rate(&self) -> f32 {
        let bad = self.malformed_tool_calls.load(Ordering::Relaxed) as f32;
        let all = self.tool_calls.load(Ordering::Relaxed) as f32;
        if all == 0.0 { 0.0 } else { bad / all }
    }
}

pub struct Router {
    pub chat: Option<Arc<dyn ModelWorker>>,
    pub utility: Option<Arc<dyn ModelWorker>>,
    pub embedding: Option<Arc<dyn ModelWorker>>,
    pub metrics: Arc<Metrics>,
}

impl Default for Router {
    fn default() -> Self {
        Router {
            chat: None,
            utility: None,
            embedding: None,
            metrics: Arc::new(Metrics::default()),
        }
    }
}

impl Router {
    /// Which worker serves this role, falling back to the chat worker.
    pub fn worker(&self, role: WorkerRole) -> Option<Arc<dyn ModelWorker>> {
        match role {
            WorkerRole::Chat => self.chat.clone(),
            WorkerRole::Utility => self.utility.clone().or_else(|| self.chat.clone()),
            WorkerRole::Embedding => self
                .embedding
                .clone()
                .or_else(|| self.utility.clone())
                .or_else(|| self.chat.clone()),
        }
    }

    pub fn chat(&self, role: WorkerRole, req: &ChatRequest) -> Result<ChatReply> {
        let worker = self
            .worker(role)
            .context("no model is loaded in this environment")?;
        // Count against the worker that actually served it: a utility request
        // that fell back to the chat worker is a chat call, and should show as one.
        let served_by_utility = role == WorkerRole::Utility && self.utility.is_some();
        if served_by_utility {
            self.metrics.utility_calls.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.chat_calls.fetch_add(1, Ordering::Relaxed);
        }
        let reply = with_a_bare_call_read(worker.chat(req)?, req);
        self.metrics
            .prompt_tokens
            .fetch_add(reply.prompt_tokens as u64, Ordering::Relaxed);
        Ok(reply)
    }

    /// `chat`, with the text streamed to `on_delta` as it is produced.
    pub fn chat_streaming(
        &self,
        role: WorkerRole,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
        self.chat_streaming_until(role, req, on_delta, &Stop::default(), Silence::ANSWER)
    }

    /// `chat_streaming`, ended by `stop` or by a silence longer than
    /// `silence` allows: see [`ModelWorker::chat_streaming_until`].
    pub fn chat_streaming_until(
        &self,
        role: WorkerRole,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
        stop: &Stop,
        silence: Silence,
    ) -> Result<ChatReply> {
        let worker = self
            .worker(role)
            .context("no model is loaded in this environment")?;
        let served_by_utility = role == WorkerRole::Utility && self.utility.is_some();
        if served_by_utility {
            self.metrics.utility_calls.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.chat_calls.fetch_add(1, Ordering::Relaxed);
        }
        let reply = with_a_bare_call_read(
            worker.chat_streaming_until(req, on_delta, stop, silence)?,
            req,
        );
        self.metrics
            .prompt_tokens
            .fetch_add(reply.prompt_tokens as u64, Ordering::Relaxed);
        Ok(reply)
    }

    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let worker = self
            .worker(WorkerRole::Embedding)
            .context("no embedding model is loaded")?;
        worker.embed(texts)
    }

    pub fn info(&self) -> Option<proto::ModelInfo> {
        self.chat.as_ref().map(|w| w.info())
    }

    /// What an answer is read with: the chat worker and the counters, taken
    /// out of the router, so that nothing holds the router while an answer
    /// is written. Stopping or changing the engine takes the router for
    /// writing, and would otherwise wait for the answer to end. `None` with
    /// no model loaded.
    pub fn for_a_turn(&self) -> Option<Streamer> {
        Some(Streamer {
            worker: self.worker(WorkerRole::Chat)?,
            metrics: self.metrics.clone(),
        })
    }
}

/// The chat worker, as an answer reads it: see [`Router::for_a_turn`].
pub struct Streamer {
    worker: Arc<dyn ModelWorker>,
    metrics: Arc<Metrics>,
}

impl Streamer {
    /// [`Router::chat_streaming_until`], with the worker taken out.
    pub fn chat_streaming_until(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
        stop: &Stop,
        silence: Silence,
    ) -> Result<ChatReply> {
        self.metrics.chat_calls.fetch_add(1, Ordering::Relaxed);
        let reply = with_a_bare_call_read(
            self.worker
                .chat_streaming_until(req, on_delta, stop, silence)?,
            req,
        );
        self.metrics
            .prompt_tokens
            .fetch_add(reply.prompt_tokens as u64, Ordering::Relaxed);
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What an answer carried on passes to the person, piece by piece, to
    /// the stream's end.
    fn passed(begun: &str, pieces: &[&str]) -> Vec<String> {
        let mut again = SaidAgain::new(begun);
        let mut passed = Vec::new();
        for piece in pieces {
            again.pass(piece, &mut |p| passed.push(p.to_string()));
        }
        again.finish(&mut |p| passed.push(p.to_string()));
        passed
    }

    #[test]
    fn an_answer_carried_on_passes_only_what_the_model_adds() {
        // Said again in the first piece, as llama-server does.
        assert_eq!(
            passed("Once upon", &["Once upon a", " time"]),
            [" a", " time"]
        );
        // Said again across pieces.
        assert_eq!(
            passed("Once upon", &["Once", " up", "on a", " time"]),
            [" a", " time"]
        );
        // Not said again: everything passes.
        assert_eq!(passed("Once upon", &[" a", " time"]), [" a", " time"]);
        // Said again, and nothing added.
        assert!(passed("Once upon", &["Once upon"]).is_empty());
        // Nothing begun.
        assert_eq!(passed("", &["Hello"]), ["Hello"]);
    }

    /// Only an exact match is dropped: a start that is the handed words
    /// only as far as it goes, or that turns away from them, is all passed
    /// on, and kept.
    #[test]
    fn nothing_is_dropped_unless_the_handed_words_come_back_exactly() {
        // Part of the words, then the end of the stream.
        assert_eq!(passed("Once upon", &["Once up"]), ["Once up"]);
        // A start that turns away from them.
        assert_eq!(passed("Once upon", &["Once", " more"]), ["Once more"]);
        assert_eq!(passed("Once upon", &["Once upOn a"]), ["Once upOn a"]);

        let reply = |text: &str| ChatReply {
            text: text.into(),
            ..Default::default()
        };
        // A reply that came whole.
        let mut again = SaidAgain::new("Once upon");
        assert_eq!(again.carried_on(reply("Once upon a time")).text, " a time");
        let mut again = SaidAgain::new("Once upon");
        assert_eq!(again.carried_on(reply(" a time")).text, " a time");
        let mut again = SaidAgain::new("Once upon");
        assert_eq!(again.carried_on(reply("Once up")).text, "Once up");
        // After a stream that did not say them again, the reply keeps all.
        let mut again = SaidAgain::new("Once upon");
        again.pass("Once more", &mut |_| {});
        assert_eq!(again.carried_on(reply("Once more")).text, "Once more");
        // After one that did, only what was added.
        let mut again = SaidAgain::new("Once upon");
        again.pass("Once upon a", &mut |_| {});
        assert_eq!(again.carried_on(reply("Once upon a")).text, " a");
    }

    #[test]
    fn the_words_of_an_answer_carried_on_are_the_start_of_the_models_reply() {
        let place = LedgerPlace::default();
        let mut req = ChatRequest::new("the prompt".into());
        assert_eq!(
            messages(&req, place),
            json!([{"role": "user", "content": "the prompt"}])
        );
        req.begun = Some("Once upon".into());
        assert_eq!(
            messages(&req, place),
            json!([
                {"role": "user", "content": "the prompt"},
                {"role": "assistant", "content": "Once upon"}
            ])
        );
        // An answer stopped before its first word begins nothing.
        req.begun = Some(String::new());
        assert_eq!(messages(&req, place).as_array().map(Vec::len), Some(1));
    }

    fn a_step(turns: Vec<Turn>) -> ChatRequest {
        ChatRequest::with_turns(
            "the instructions".into(),
            turns,
            "[task run_1]\nnotes:\n  - blue".into(),
        )
    }

    /// The prompt as a system message and turns (docs/DECISIONS.md,
    /// 2026-09-24): the instructions first, a call as the model's own call
    /// and the tool's reply, and the ledger where the model takes it.
    #[test]
    fn a_step_is_the_instructions_then_the_turns_then_the_ledger() {
        let req = a_step(vec![
            Turn::Person("note the risks".into()),
            Turn::Call {
                id: "t1".into(),
                tool: "canvas.add_sticky".into(),
                params: json!({"text": "risks"}),
                result: "ok: added 1 shape".into(),
            },
        ]);
        assert_eq!(
            messages(&req, LedgerPlace::AfterTheNewest),
            json!([
                {"role": "system", "content": "the instructions"},
                {"role": "user", "content": "note the risks"},
                {"role": "assistant", "content": "", "tool_calls": [{
                    "id": "t1",
                    "type": "function",
                    "function": {"name": "canvas__add_sticky", "arguments": "{\"text\":\"risks\"}"}
                }]},
                {"role": "tool", "tool_call_id": "t1", "content": "ok: added 1 shape"},
                {"role": "system", "content": "[task run_1]\nnotes:\n  - blue"}
            ])
        );
        // A template that takes no late system message: the ledger ends the
        // person's newest message.
        let sent = messages(&req, LedgerPlace::InTheNewest);
        let sent = sent.as_array().unwrap();
        assert_eq!(sent.len(), 4);
        assert_eq!(
            sent[1],
            json!({"role": "user", "content": "note the risks\n\n[task run_1]\nnotes:\n  - blue"})
        );
    }

    /// Doc 22's test, at the level of what is sent: two turns' messages match
    /// up to the new message with the ledger after the newest; with the ledger
    /// in the person's newest message, up to the start of the previous one.
    #[test]
    fn two_turns_prompts_match_up_to_the_new_message() {
        let first = vec![Turn::Person("hello".into())];
        let second = vec![
            Turn::Person("hello".into()),
            Turn::Answer("Hi! What can I do?".into()),
            Turn::Person("add a shape".into()),
        ];
        let after = |turns: &[Turn]| messages(&a_step(turns.to_vec()), LedgerPlace::AfterTheNewest);
        let (one, two) = (after(&first), after(&second));
        let (one, two) = (one.as_array().unwrap(), two.as_array().unwrap());
        // Everything of the first turn but its ledger begins the second.
        assert_eq!(one[..one.len() - 1], two[..one.len() - 1]);
        assert_eq!(two.last().unwrap()["role"], "system");

        let inside = |turns: &[Turn]| messages(&a_step(turns.to_vec()), LedgerPlace::InTheNewest);
        let (one, two) = (inside(&first), inside(&second));
        let (one, two) = (one.as_array().unwrap(), two.as_array().unwrap());
        // The previous message carried the ledger, so the match ends there.
        assert_eq!(one[..1], two[..1]);
        assert_ne!(one[1], two[1]);
        assert_eq!(two[1], json!({"role": "user", "content": "hello"}));
    }

    #[test]
    fn a_step_with_nothing_in_the_ledger_sends_no_ledger() {
        let req = ChatRequest::with_turns(
            "the instructions".into(),
            vec![Turn::Person("hi".into())],
            String::new(),
        );
        for place in [LedgerPlace::AfterTheNewest, LedgerPlace::InTheNewest] {
            assert_eq!(
                messages(&req, place),
                json!([
                    {"role": "system", "content": "the instructions"},
                    {"role": "user", "content": "hi"}
                ])
            );
        }
    }

    struct Fake {
        id: &'static str,
    }

    impl ModelWorker for Fake {
        fn info(&self) -> proto::ModelInfo {
            proto::ModelInfo {
                id: self.id.into(),
                backend: "test".into(),
                context_len: 8192,
                supports_tools: true,
                supports_vision: false,
                loaded: true,
            }
        }
        fn chat(&self, _req: &ChatRequest) -> Result<ChatReply> {
            Ok(ChatReply {
                text: self.id.to_string(),
                prompt_tokens: 10,
                ..Default::default()
            })
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2]).collect())
        }
    }

    #[test]
    fn native_tool_calls_are_parsed_and_denamespaced() {
        let res = json!({
            "choices": [{"message": {
                "content": "",
                "tool_calls": [{
                    "id": "c1",
                    "function": {"name": "canvas__add_shape", "arguments": "{\"kind\":\"rect\"}"}
                }]
            }}],
            "usage": {"prompt_tokens": 120, "completion_tokens": 8}
        });
        let reply = parse_openai_reply(&res).unwrap();
        assert_eq!(reply.calls.len(), 1);
        assert_eq!(reply.calls[0].tool, "canvas.add_shape");
        assert_eq!(reply.calls[0].params["kind"], "rect");
        assert_eq!(reply.prompt_tokens, 120);
    }

    #[test]
    fn a_backend_without_tool_calling_still_works_through_the_grammar_shape() {
        let res = json!({
            "choices": [{"message": {
                "content": "{\"tool\": \"canvas.add_shape\", \"params\": {\"kind\": \"ellipse\"}}"
            }}]
        });
        let reply = parse_openai_reply(&res).unwrap();
        assert_eq!(reply.calls.len(), 1);
        assert_eq!(reply.calls[0].tool, "canvas.add_shape");
        assert_eq!(reply.calls[0].params["kind"], "ellipse");
    }

    fn offered(names: &[&str]) -> Vec<proto::ExposedTool> {
        names
            .iter()
            .map(|name| proto::ExposedTool {
                harness: "core".into(),
                name: name.to_string(),
                summary: String::new(),
                params: proto::Json(json!({"type": "object"})),
                kind: proto::ToolKind::Write,
                confirm: proto::Confirm::Never,
                cost_hint: proto::CostHint::Instant,
                undoable: false,
                reason: proto::ExposureReason::CoreBuiltin,
            })
            .collect()
    }

    /// What the 14B wrote on 2026-09-19, to the letter.
    const RECORDED_BARE_CALL: &str = r#"{"name": "task.note", "arguments": {"text": "Translate into German: Wo ist der n\u00e4chste Bahnhof?"}}"#;

    #[test]
    fn a_reply_that_is_nothing_but_a_call_to_a_tool_on_offer_is_read_as_the_call() {
        let tools = offered(&["find_capability", "task.note"]);
        let call = parse_bare_call(RECORDED_BARE_CALL, &tools).unwrap();
        assert_eq!(call.tool, "task.note");
        assert_eq!(
            call.params["text"],
            "Translate into German: Wo ist der nächste Bahnhof?"
        );
        // As the engine is given the name, and with the arguments in a string.
        let call = parse_bare_call(
            r#" {"name": "task__note", "arguments": "{\"text\": \"x\"}"} "#,
            &tools,
        )
        .unwrap();
        assert_eq!(call.tool, "task.note");
        assert_eq!(call.params["text"], "x");
    }

    #[test]
    fn json_a_person_asked_for_is_never_taken_for_a_call() {
        let tools = offered(&["task.note"]);
        // A tool that is not on offer in this request.
        assert!(parse_bare_call(RECORDED_BARE_CALL, &offered(&["find_capability"])).is_none());
        // Words around it: the reply is not nothing but the call.
        let around = format!("Here is the call: {RECORDED_BARE_CALL}");
        assert!(parse_bare_call(&around, &tools).is_none());
        // Another member, a missing one, arguments that are not an object.
        for text in [
            r#"{"name": "task.note", "arguments": {}, "id": 1}"#,
            r#"{"name": "task.note"}"#,
            r#"{"name": "task.note", "arguments": [1, 2]}"#,
            r#"{"name": "task.note", "arguments": "not json"}"#,
            r#"{"name": "Ada", "arguments": {"age": 36}}"#,
            r#"[{"name": "task.note", "arguments": {}}]"#,
            "",
        ] {
            assert!(parse_bare_call(text, &tools).is_none(), "{text}");
        }
    }

    /// What the 14B wrote on 2026-10-06, asked for a translation with the
    /// whiteboard installed: a call to a tool that does not exist.
    const TRANSLATE_TEXT: &str = r#"{"name": "translate_text", "arguments": {"text": "Where is the nearest train station?", "target_language": "German"}}"#;

    #[test]
    fn an_answer_that_is_or_begins_with_a_call_is_call_shaped() {
        let (name, arguments) = call_shaped(TRANSLATE_TEXT).unwrap();
        assert_eq!(name, "translate_text");
        assert_eq!(arguments["target_language"], "German");
        // Words after it, a code fence, Core's own shape.
        for text in [
            format!("{TRANSLATE_TEXT}\nThat should do it."),
            format!("```json\n{TRANSLATE_TEXT}\n```"),
            r#"{"tool": "web.search", "params": {"query": "weather in Lisbon"}}"#.to_string(),
        ] {
            assert!(call_shaped(&text).is_some(), "{text}");
        }
        // Words first, other JSON, a name that is not an identifier, a third
        // member, words alone.
        for text in [
            format!("Here it is: {TRANSLATE_TEXT}"),
            r#"{"city": "Lisbon", "temperature": 21}"#.to_string(),
            r#"{"name": "Ada Pellow", "arguments": {"year": 1912}}"#.to_string(),
            r#"{"name": "x", "arguments": {}, "id": 1}"#.to_string(),
            "Wo ist der nächste Bahnhof?".to_string(),
        ] {
            assert!(call_shaped(&text).is_none(), "{text}");
        }
    }

    #[test]
    fn the_router_reads_a_bare_call_whichever_worker_wrote_it() {
        struct Bare;
        impl ModelWorker for Bare {
            fn info(&self) -> proto::ModelInfo {
                Fake { id: "bare" }.info()
            }
            fn chat(&self, _req: &ChatRequest) -> Result<ChatReply> {
                Ok(ChatReply {
                    text: RECORDED_BARE_CALL.to_string(),
                    ..Default::default()
                })
            }
            fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(Vec::new())
            }
        }
        let router = Router {
            chat: Some(Arc::new(Bare)),
            ..Default::default()
        };
        let mut request = ChatRequest::new(String::new());
        request.tools = offered(&["task.note"]);
        let reply = router.chat(WorkerRole::Chat, &request).unwrap();
        assert_eq!(reply.calls.len(), 1);
        assert_eq!(reply.calls[0].tool, "task.note");
        let mut shown = String::new();
        let streamed = router
            .chat_streaming(WorkerRole::Chat, &request, &mut |delta| {
                shown.push_str(delta)
            })
            .unwrap();
        assert_eq!(streamed.calls.len(), 1);
        // The same words with the tool not on offer stay words.
        request.tools = offered(&["find_capability"]);
        let reply = router.chat(WorkerRole::Chat, &request).unwrap();
        assert!(reply.calls.is_empty());
        assert_eq!(reply.text, RECORDED_BARE_CALL);
    }

    #[test]
    fn plain_prose_produces_no_tool_call() {
        let res = json!({"choices": [{"message": {"content": "I added the shape."}}]});
        let reply = parse_openai_reply(&res).unwrap();
        assert!(reply.calls.is_empty());
        assert_eq!(reply.text, "I added the shape.");
    }

    #[test]
    fn an_error_body_surfaces_as_an_error() {
        let res = json!({"error": {"message": "context length exceeded"}});
        let err = parse_openai_reply(&res).unwrap_err().to_string();
        assert!(err.contains("context length exceeded"));
    }

    #[test]
    fn utility_requests_go_to_the_utility_worker() {
        let router = Router {
            chat: Some(Arc::new(Fake { id: "big" })),
            utility: Some(Arc::new(Fake { id: "small" })),
            ..Default::default()
        };
        let req = ChatRequest::new("x".into());
        assert_eq!(
            router.chat(WorkerRole::Utility, &req).unwrap().text,
            "small"
        );
        assert_eq!(router.chat(WorkerRole::Chat, &req).unwrap().text, "big");
        assert!((router.metrics.utility_share() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn without_a_utility_worker_the_fallback_is_counted_as_a_chat_call() {
        let router = Router {
            chat: Some(Arc::new(Fake { id: "big" })),
            ..Default::default()
        };
        let req = ChatRequest::new("x".into());
        assert_eq!(router.chat(WorkerRole::Utility, &req).unwrap().text, "big");
        assert_eq!(
            router.metrics.utility_share(),
            0.0,
            "no utility model was used"
        );
    }

    #[test]
    fn with_no_model_at_all_the_router_says_so_plainly() {
        let router = Router::default();
        let err = router
            .chat(WorkerRole::Chat, &ChatRequest::new("x".into()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no model is loaded"));
    }
}
