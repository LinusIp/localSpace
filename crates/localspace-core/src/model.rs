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

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub prompt: String,
    pub tools: Vec<proto::ExposedTool>,
    /// GBNF for backends that accept one; ignored by those that do not.
    pub grammar: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub class: RequestClass,
}

impl ChatRequest {
    pub fn new(prompt: String) -> ChatRequest {
        ChatRequest {
            prompt,
            tools: Vec::new(),
            grammar: None,
            max_tokens: 1024,
            temperature: 0.2,
            class: RequestClass::Interactive,
        }
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
        }
    }

    pub fn with_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    pub fn with_context_len(mut self, n: u32) -> Self {
        self.context_len = n;
        self
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
            "messages": [{"role": "user", "content": req.prompt}],
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": false,
        });
        if !req.tools.is_empty() {
            body["tools"] = J::Array(req.tools.iter().map(tool_schema).collect());
            body["tool_choice"] = json!("auto");
        }

        let res = self.post("/chat/completions", body)?;
        parse_openai_reply(&res)
    }

    fn chat_streaming(
        &self,
        req: &ChatRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
        let mut body = json!({
            "model": self.model,
            "messages": [{"role": "user", "content": req.prompt}],
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
        let mut request = ureq::post(&url)
            .config()
            .timeout_global(Some(self.timeout))
            .build();
        if let Some(k) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {k}"));
        }
        let mut res = request
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("POST {url}"))?;
        let reader = std::io::BufReader::new(res.body_mut().as_reader());
        read_sse(reader, on_delta)
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

/// An OpenAI-style server-sent event stream, read to the end: text deltas go
/// to `on_delta` as they arrive, tool-call deltas are assembled by index, and
/// the last event's usage is kept.
pub fn read_sse<R: std::io::BufRead>(
    reader: R,
    on_delta: &mut dyn FnMut(&str),
) -> Result<ChatReply> {
    let mut text = String::new();
    // (id, name, arguments) per tool-call index; arguments arrive in pieces.
    let mut calls: Vec<(String, String, String)> = Vec::new();
    let mut prompt_tokens = 0u32;
    let mut completion_tokens = 0u32;
    for line in reader.lines() {
        let line = line
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("reading the model's stream")?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
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
        let reply = worker.chat(req)?;
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
        let worker = self
            .worker(role)
            .context("no model is loaded in this environment")?;
        let served_by_utility = role == WorkerRole::Utility && self.utility.is_some();
        if served_by_utility {
            self.metrics.utility_calls.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.chat_calls.fetch_add(1, Ordering::Relaxed);
        }
        let reply = worker.chat_streaming(req, on_delta)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
