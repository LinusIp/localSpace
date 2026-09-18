//! Tier B — a native subprocess speaking MCP-shaped JSON-RPC 2.0 over stdio.
//!
//! Same protocol as Tier A from the agent's point of view. Because the wire format
//! is MCP, an existing MCP server is already a headless harness: it answers
//! `tools/list` and `tools/call`, ignores the three `harness/*` extensions, and
//! Core falls back to a generic context block for it.
//!
//! Isolation status, stated plainly: the child is spawned with a cleared
//! environment and its working directory pinned to the package directory, and
//! Tier B is refused outright unless the environment permits it and the user has
//! approved the package's `native_reason`. The OS-level sandbox the spec calls for
//! (Windows job object + AppContainer, Linux landlock + seccomp, macOS sandbox
//! profile) is NOT yet applied — see `docs/STATUS.md`.

use super::{HarnessOutput, HarnessRuntime, RuntimeConfig};
use anyhow::{Context, Result, bail};
use serde_json::{Value as J, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

/// `_meta` keys used to carry the harness document alongside a standard MCP call.
const META_DOC: &str = "localspace/doc";
const META_DIFF: &str = "localspace/diff-summary";

pub struct NativeHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    harness_id: String,
    /// False when the server answered `harness/context` with "method not found":
    /// a plain MCP server, which Core then describes generically.
    supports_extensions: bool,
}

impl NativeHarness {
    pub fn spawn(program: &Path, args: &[String], cfg: RuntimeConfig) -> Result<NativeHarness> {
        let dir = program
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut child = crate::child::command(program)
            .args(args)
            .current_dir(&dir)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawning Tier B harness {}", program.display()))?;

        let stdin = child.stdin.take().context("child stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("child stdout")?);

        let mut h = NativeHarness {
            child,
            stdin,
            stdout,
            next_id: 1,
            harness_id: cfg.harness_id,
            supports_extensions: true,
        };

        h.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "localSpace Core", "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .context("MCP initialize failed")?;
        h.notify("notifications/initialized", json!({}))?;
        Ok(h)
    }

    fn request(&mut self, method: &str, params: J) -> Result<J> {
        let id = self.next_id;
        self.next_id += 1;
        let line = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }))?;
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()?;

        // Skip notifications and responses to other ids.
        for _ in 0..64 {
            let mut buf = String::new();
            let n = self.stdout.read_line(&mut buf)?;
            if n == 0 {
                bail!("Tier B harness {} closed its stdout", self.harness_id);
            }
            let Ok(msg) = serde_json::from_str::<J>(buf.trim()) else {
                continue;
            };
            if msg.get("id").and_then(|v| v.as_u64()) != Some(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                let message = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                if code == -32601 {
                    bail!("METHOD_NOT_FOUND: {message}");
                }
                bail!("{method} failed: {message}");
            }
            return Ok(msg.get("result").cloned().unwrap_or(J::Null));
        }
        bail!("no response to `{method}` from {}", self.harness_id)
    }

    fn notify(&mut self, method: &str, params: J) -> Result<()> {
        let line = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "method": method, "params": params
        }))?;
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()?;
        Ok(())
    }
}

impl Drop for NativeHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn meta_doc(doc: &J) -> J {
    json!({ META_DOC: doc })
}

impl HarnessRuntime for NativeHarness {
    fn tools_json(&mut self) -> Result<String> {
        let res = self.request("tools/list", json!({}))?;
        let tools = res.get("tools").cloned().unwrap_or(json!([]));
        // Translate MCP's tool shape into a localSpace tools.json array.
        let mapped: Vec<J> = tools
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|t| {
                let annotations = t.get("annotations").cloned().unwrap_or(json!({}));
                let read_only = annotations
                    .get("readOnlyHint")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let destructive = annotations
                    .get("destructiveHint")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(!read_only);
                json!({
                    "name": t.get("name").cloned().unwrap_or(json!("")),
                    "summary": t.get("description").and_then(|d| d.as_str()).unwrap_or("MCP tool."),
                    "params": t.get("inputSchema").cloned()
                        .unwrap_or(json!({"type": "object", "properties": {}})),
                    "kind": if read_only { "read" } else { "write" },
                    "undoable": false,
                    "confirm": if destructive { "destructive" } else { "never" },
                    "cost_hint": "seconds",
                })
            })
            .collect();
        Ok(serde_json::to_string(&mapped)?)
    }

    fn call(&mut self, name: &str, params: &J, doc: &J) -> Result<HarnessOutput> {
        let res = self.request(
            "tools/call",
            json!({"name": name, "arguments": params, "_meta": meta_doc(doc)}),
        );
        let res = match res {
            Ok(r) => r,
            Err(e) => return Ok(HarnessOutput::failed(e.to_string())),
        };

        let is_error = res
            .get("isError")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let structured = res.get("structuredContent").cloned();
        let text = res
            .get("content")
            .and_then(|c| c.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        let meta = res.get("_meta").cloned().unwrap_or(J::Null);
        Ok(HarnessOutput {
            ok: !is_error,
            result: structured.unwrap_or(J::String(text.clone())),
            error: if is_error { Some(text) } else { None },
            diff_summary: meta
                .get(META_DIFF)
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
            doc: meta.get(META_DOC).cloned(),
            logs: Vec::new(),
        })
    }

    fn context(&mut self, budget: usize, focused: bool, doc: &J) -> Result<(String, bool)> {
        if !self.supports_extensions {
            return Ok((String::new(), false));
        }
        match self.request(
            "harness/context",
            json!({"budget_tokens": budget, "focus": focused, "_meta": meta_doc(doc)}),
        ) {
            Ok(res) => Ok((
                res.get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string(),
                res.get("expandable")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false),
            )),
            Err(e) if e.to_string().starts_with("METHOD_NOT_FOUND") => {
                // A plain MCP server. Legal; Core describes it generically.
                self.supports_extensions = false;
                Ok((String::new(), false))
            }
            Err(e) => Err(e),
        }
    }

    fn view(&mut self, view_id: &str, doc: &J) -> Result<J> {
        self.request(
            "harness/view",
            json!({"view": view_id, "_meta": meta_doc(doc)}),
        )
    }

    fn event(&mut self, view_id: &str, payload: &[u8], doc: &J) -> Result<(Vec<u8>, Option<J>)> {
        let res = self.request(
            "harness/event",
            json!({"view": view_id, "payload": payload, "_meta": meta_doc(doc)}),
        )?;
        let reply: Vec<u8> = res
            .get("payload")
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default();
        let doc_out = res.get("_meta").and_then(|m| m.get(META_DOC)).cloned();
        Ok((reply, doc_out))
    }
}
