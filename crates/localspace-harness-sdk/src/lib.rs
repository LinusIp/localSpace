#![deny(unsafe_code)]
//! What harness authors compile logic against.
//!
//! The ABI itself is `wit/harness.wit`, generated in the guest with
//! `wit_bindgen::generate!`. This crate is the small layer above it: the JSON
//! envelopes the three exports return, so an author is not hand-writing string
//! literals and guessing at field names.
//!
//! It deliberately has no dependency on the host. A harness that compiles against
//! this crate compiles for `wasm32-wasip2` with nothing else linked in.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The envelope `call` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub ok: bool,
    #[serde(default)]
    pub result: Value,
    /// What the model sees. One short line: "added 3 shapes, moved 1".
    /// Never the whole document — the context provider handles state.
    #[serde(rename = "diff-summary", skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(result: Value, diff_summary: impl Into<String>) -> ToolResult {
        ToolResult {
            ok: true,
            result,
            diff_summary: Some(diff_summary.into()),
            error: None,
        }
    }

    /// A read tool that changed nothing.
    pub fn read(result: Value, summary: impl Into<String>) -> ToolResult {
        ToolResult::ok(result, summary)
    }

    pub fn failed(message: impl Into<String>) -> ToolResult {
        ToolResult {
            ok: false,
            result: Value::Null,
            diff_summary: None,
            error: Some(message.into()),
        }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"ok\":false,\"error\":\"could not encode the result: {e}\"}}")
        })
    }
}

/// The envelope `context` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    /// A text serialization of the harness's state, sized to the budget.
    pub text: String,
    /// True when detail was elided and a zoom tool can expand it. Providers
    /// should be expandable: never dump the whole document.
    pub expandable: bool,
}

impl ContextBlock {
    pub fn new(text: impl Into<String>) -> ContextBlock {
        ContextBlock {
            text: text.into(),
            expandable: true,
        }
    }

    pub fn complete(text: impl Into<String>) -> ContextBlock {
        ContextBlock {
            text: text.into(),
            expandable: false,
        }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The same estimate Core uses for every budget, so a provider can size itself
/// against the number the host will actually measure.
pub fn estimate_tokens(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.len().div_ceil(4)
    }
}

/// Append lines until the budget is spent, then say so.
///
/// A provider that returns more than its budget is truncated by Core anyway;
/// doing it here means the harness chooses *what* to drop.
pub struct BudgetedText {
    out: String,
    budget: usize,
    truncated: bool,
}

impl BudgetedText {
    pub fn new(budget_tokens: usize) -> BudgetedText {
        BudgetedText {
            out: String::new(),
            budget: budget_tokens,
            truncated: false,
        }
    }

    /// Always written, budget or not: use for the headline a provider must emit.
    pub fn header(&mut self, line: impl AsRef<str>) {
        self.out.push_str(line.as_ref());
        self.out.push('\n');
    }

    /// Written only while there is room. Returns false once the budget is spent.
    pub fn line(&mut self, line: impl AsRef<str>) -> bool {
        let line = line.as_ref();
        if estimate_tokens(&self.out) + estimate_tokens(line) + 1 > self.budget {
            self.truncated = true;
            return false;
        }
        self.out.push_str(line);
        self.out.push('\n');
        true
    }

    pub fn spent(&self) -> bool {
        self.truncated
    }

    pub fn finish(mut self, zoom_hint: &str) -> ContextBlock {
        if self.truncated && !zoom_hint.is_empty() {
            self.out.push_str(zoom_hint);
            self.out.push('\n');
        }
        ContextBlock {
            text: self.out,
            expandable: self.truncated,
        }
    }
}

/// Widget-tree helpers for a `kind = "widgets"` view.
///
/// The tree is the `{"w": "<kind>", ...}` form documented in `wit/harness.wit`.
pub mod widgets {
    use serde_json::{json, Value};

    pub fn column(children: Vec<Value>) -> Value {
        json!({"w": "column", "children": children})
    }
    pub fn row(children: Vec<Value>) -> Value {
        json!({"w": "row", "children": children})
    }
    pub fn heading(text: &str) -> Value {
        json!({"w": "heading", "text": text})
    }
    pub fn text(text: &str) -> Value {
        json!({"w": "text", "text": text})
    }
    pub fn separator() -> Value {
        json!({"w": "separator"})
    }
    pub fn button(id: &str, label: &str) -> Value {
        json!({"w": "button", "id": id, "label": label, "enabled": true})
    }
    pub fn input(id: &str, label: &str, value: &str) -> Value {
        json!({"w": "input", "id": id, "label": label, "value": value})
    }
    pub fn checkbox(id: &str, label: &str, value: bool) -> Value {
        json!({"w": "checkbox", "id": id, "label": label, "value": value})
    }
    pub fn table(headers: &[&str], rows: Vec<Vec<String>>) -> Value {
        json!({"w": "table", "headers": headers, "rows": rows})
    }
    pub fn badge(text: &str, tone: &str) -> Value {
        json!({"w": "badge", "text": text, "tone": tone})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tool_result_encodes_the_shape_core_expects() {
        let encoded = ToolResult::ok(json!({"id": "s1"}), "added 1 sticky").encode();
        let back: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(back["ok"], true);
        assert_eq!(back["diff-summary"], "added 1 sticky");
        assert_eq!(back["result"]["id"], "s1");
        assert!(back.get("error").is_none(), "no error key on success");
    }

    #[test]
    fn a_failure_carries_the_message_and_no_result() {
        let back: Value = serde_json::from_str(&ToolResult::failed("no shape `s9`").encode()).unwrap();
        assert_eq!(back["ok"], false);
        assert_eq!(back["error"], "no shape `s9`");
    }

    #[test]
    fn budgeted_text_stops_at_the_budget_and_marks_itself_expandable() {
        let mut t = BudgetedText::new(40);
        t.header("board: 200 shapes");
        let mut written = 0;
        for i in 0..200 {
            if t.line(format!("  s{i} sticky red at (10,{i}) \"a risk worth noting\"")) {
                written += 1;
            } else {
                break;
            }
        }
        assert!(written > 0 && written < 200, "wrote {written} lines");
        let block = t.finish("  … call canvas.zoom for the rest");
        assert!(block.expandable);
        assert!(block.text.contains("canvas.zoom"));
        assert!(estimate_tokens(&block.text) <= 60, "{}", block.text);
    }

    #[test]
    fn a_provider_within_budget_is_not_marked_expandable() {
        let mut t = BudgetedText::new(500);
        t.header("board: 1 shape");
        assert!(t.line("  s1 sticky red \"only one\""));
        let block = t.finish("  … zoom");
        assert!(!block.expandable);
        assert!(!block.text.contains("zoom"));
    }

    #[test]
    fn widget_helpers_produce_the_authoring_form() {
        let tree = widgets::column(vec![
            widgets::heading("Board"),
            widgets::input("title", "Title", "Q4"),
            widgets::badge("3 shapes", "good"),
        ]);
        assert_eq!(tree["w"], "column");
        assert_eq!(tree["children"][0]["w"], "heading");
        assert_eq!(tree["children"][1]["value"], "Q4");
        assert_eq!(tree["children"][2]["tone"], "good");
    }
}
