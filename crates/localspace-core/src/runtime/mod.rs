//! Harness logic runtimes. Two tiers, one protocol; the agent cannot tell them apart.
//!
//! Both run next to Core. Tier A is a wasm component under wasmtime with no ambient
//! authority. Tier B is an OS subprocess speaking MCP-shaped JSON-RPC over stdio.

pub mod native;
pub mod wasm;

use crate::manifest::Capabilities;
use anyhow::Result;
use serde_json::Value as J;
use std::sync::Arc;

/// What harness logic hands back from one call.
#[derive(Debug, Clone, Default)]
pub struct HarnessOutput {
    pub ok: bool,
    pub result: J,
    pub error: Option<String>,
    /// The harness's own words for what changed. Core still computes the
    /// authoritative diff from the document itself.
    pub diff_summary: Option<String>,
    /// A replacement document, if the call wrote one.
    pub doc: Option<J>,
    pub logs: Vec<String>,
}

impl HarnessOutput {
    pub fn failed(msg: impl Into<String>) -> Self {
        HarnessOutput {
            ok: false,
            error: Some(msg.into()),
            ..Default::default()
        }
    }

    /// Parse the JSON envelope a harness returns from `call`.
    pub fn from_json(v: &J) -> HarnessOutput {
        HarnessOutput {
            ok: v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false),
            result: v.get("result").cloned().unwrap_or(J::Null),
            error: v
                .get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string()),
            diff_summary: v
                .get("diff-summary")
                .or_else(|| v.get("diff_summary"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
            doc: None,
            logs: Vec::new(),
        }
    }
}

/// Services Core exposes to harness logic. Every method is called only after
/// the capability check in the host bindings has passed.
pub trait CoreServices: Send + Sync {
    fn model_complete(&self, prompt: &str) -> std::result::Result<String, String>;
    fn model_structured(
        &self,
        schema: &str,
        prompt: &str,
    ) -> std::result::Result<String, String>;
    fn model_embed(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, String>;
    fn docs_search(&self, query: &str) -> std::result::Result<String, String>;
    /// `harness` is passed so the gateway can check the package's own allowlist.
    fn net_fetch(
        &self,
        harness: &str,
        url: &str,
        mode: &str,
    ) -> std::result::Result<String, String>;
}

/// A `CoreServices` that grants nothing — used before a model is selected and in tests.
pub struct NoServices;

impl CoreServices for NoServices {
    fn model_complete(&self, _p: &str) -> std::result::Result<String, String> {
        Err("no model is loaded in this environment".into())
    }
    fn model_structured(&self, _s: &str, _p: &str) -> std::result::Result<String, String> {
        Err("no model is loaded in this environment".into())
    }
    fn model_embed(&self, _t: &[String]) -> std::result::Result<Vec<Vec<f32>>, String> {
        Err("no model is loaded in this environment".into())
    }
    fn docs_search(&self, _q: &str) -> std::result::Result<String, String> {
        Err("retrieval is not available in this environment".into())
    }
    fn net_fetch(&self, _h: &str, _u: &str, _m: &str) -> std::result::Result<String, String> {
        Err("egress is not available in this environment".into())
    }
}

/// One installed harness's logic, whichever tier it runs in.
pub trait HarnessRuntime: Send {
    /// The tool declarations the module itself reports.
    fn tools_json(&mut self) -> Result<String>;

    /// Run one tool. `doc` is the current document projection; the returned
    /// `HarnessOutput::doc` is the harness's replacement, if any.
    fn call(&mut self, name: &str, params: &J, doc: &J) -> Result<HarnessOutput>;

    /// Text serialization of harness state at a token budget.
    fn context(&mut self, budget: usize, focused: bool, doc: &J) -> Result<(String, bool)>;

    /// Declarative widget tree for a `widgets` view.
    fn view(&mut self, view_id: &str, doc: &J) -> Result<J>;

    /// Opaque surface command.
    fn event(&mut self, view_id: &str, payload: &[u8], doc: &J) -> Result<(Vec<u8>, Option<J>)>;

    /// Bytes the harness asked for beyond its declared budget, if it ever did.
    /// Core kills and restarts an instance that reports this, and tells the user.
    fn over_budget(&self) -> Option<u64> {
        None
    }

    /// Artifacts the agent handed to the next call, as `(id, JSON payload)`.
    /// Core has already checked the harness `accepts` each one; the harness
    /// reads them through the `artifact-get` host import.
    fn set_artifacts(&mut self, _artifacts: Vec<(String, String)>) {}
}

/// Everything a runtime needs to enforce the package's declared authority.
pub struct RuntimeConfig {
    pub harness_id: String,
    pub capabilities: Capabilities,
    pub services: Arc<dyn CoreServices>,
    /// `[resources] memory_mb.logic`: the linear-memory ceiling, enforced.
    pub logic_memory_mb: u32,
}
