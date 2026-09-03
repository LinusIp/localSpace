//! `tools.json` — the typed actions a harness exposes to the agent, and the
//! install-time lint that keeps them inside the model's attention budget.

use anyhow::{bail, Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};

/// Hard budget on total tool-description tokens in context (spec §9).
pub const TOOL_TOKEN_BUDGET: usize = 1500;
/// A single summary may not exceed this many words.
pub const MAX_SUMMARY_WORDS: usize = 25;
/// A tool's parameter schema may not exceed this many top-level properties.
pub const MAX_PARAM_PROPS: usize = 8;
/// A harness may mark at most this many tools as front door.
pub const MAX_FRONT_DOOR: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,
    pub summary: String,
    #[serde(default = "empty_schema")]
    pub params: serde_json::Value,
    #[serde(default = "default_kind")]
    pub kind: ToolKind,
    #[serde(default)]
    pub front_door: bool,
    #[serde(default)]
    pub undoable: bool,
    #[serde(default)]
    pub confirm: Confirm,
    #[serde(default)]
    pub cost_hint: CostHint,
}

fn empty_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

fn default_kind() -> ToolKind {
    ToolKind::Read
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Read,
    Write,
    Compute,
}

impl From<ToolKind> for proto::ToolKind {
    fn from(k: ToolKind) -> Self {
        match k {
            ToolKind::Read => proto::ToolKind::Read,
            ToolKind::Write => proto::ToolKind::Write,
            ToolKind::Compute => proto::ToolKind::Compute,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confirm {
    #[default]
    Never,
    Destructive,
    Always,
}

impl From<Confirm> for proto::Confirm {
    fn from(c: Confirm) -> Self {
        match c {
            Confirm::Never => proto::Confirm::Never,
            Confirm::Destructive => proto::Confirm::Destructive,
            Confirm::Always => proto::Confirm::Always,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostHint {
    #[default]
    Instant,
    Seconds,
    /// Runs async and returns a job id.
    Long,
}

impl From<CostHint> for proto::CostHint {
    fn from(c: CostHint) -> Self {
        match c {
            CostHint::Instant => proto::CostHint::Instant,
            CostHint::Seconds => proto::CostHint::Seconds,
            CostHint::Long => proto::CostHint::Long,
        }
    }
}

impl ToolDecl {
    /// Tokens this tool costs when it is in the active set: name + summary + schema.
    pub fn token_cost(&self) -> usize {
        proto::estimate_tokens(&self.name)
            + proto::estimate_tokens(&self.summary)
            + proto::estimate_tokens(&self.params.to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolSet {
    pub tools: Vec<ToolDecl>,
}

impl ToolSet {
    pub fn parse(text: &str) -> Result<ToolSet> {
        let tools: Vec<ToolDecl> =
            serde_json::from_str(text).context("tools.json must be a JSON array of tool declarations")?;
        let set = ToolSet { tools };
        set.lint()?;
        Ok(set)
    }

    pub fn get(&self, name: &str) -> Option<&ToolDecl> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn front_door(&self) -> impl Iterator<Item = &ToolDecl> {
        self.tools.iter().filter(|t| t.front_door)
    }

    /// Install-time gate. Core rejects the package rather than shipping a harness
    /// that will quietly blow the model's context at runtime.
    pub fn lint(&self) -> Result<()> {
        if self.tools.is_empty() {
            bail!("tools.json declares no tools; a harness with no tools is not agent-usable");
        }

        let mut seen = std::collections::HashSet::new();
        let mut front_door = 0usize;
        let mut total = 0usize;

        for t in &self.tools {
            if !seen.insert(t.name.clone()) {
                bail!("duplicate tool name `{}`", t.name);
            }
            if t.name.trim().is_empty() {
                bail!("a tool has an empty name");
            }

            let words = t.summary.split_whitespace().count();
            if words == 0 {
                bail!("tool `{}` has no summary", t.name);
            }
            if words > MAX_SUMMARY_WORDS {
                bail!(
                    "tool `{}` summary is {words} words, over the {MAX_SUMMARY_WORDS}-word budget",
                    t.name
                );
            }

            let props = t
                .params
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|o| o.len())
                .unwrap_or(0);
            if props > MAX_PARAM_PROPS {
                bail!(
                    "tool `{}` has {props} top-level params, over the limit of {MAX_PARAM_PROPS}",
                    t.name
                );
            }
            if t.params.get("type").and_then(|v| v.as_str()) != Some("object") {
                bail!("tool `{}` params schema must have \"type\": \"object\"", t.name);
            }

            if t.front_door {
                front_door += 1;
            }

            // A write that can neither be undone nor questioned is not installable.
            if t.kind == ToolKind::Write && !t.undoable && t.confirm == Confirm::Never {
                bail!(
                    "tool `{}` is a write but is not undoable and has confirm = \"never\"",
                    t.name
                );
            }

            total += t.token_cost();
        }

        if front_door > MAX_FRONT_DOOR {
            bail!("{front_door} tools marked front_door, the limit is {MAX_FRONT_DOOR}");
        }
        if total > TOOL_TOKEN_BUDGET {
            bail!(
                "tool descriptions total ~{total} tokens, over the {TOOL_TOKEN_BUDGET}-token budget"
            );
        }
        Ok(())
    }

    pub fn token_cost(&self) -> usize {
        self.tools.iter().map(|t| t.token_cost()).sum()
    }

    /// Cost of only the front-door tools — what a pinned harness contributes.
    pub fn front_door_cost(&self) -> usize {
        self.front_door().map(|t| t.token_cost()).sum()
    }
}

/// Validate a call's params against the tool's JSON Schema.
///
/// Deliberately a small checker over the subset the lint already constrains
/// (object schemas, `required`, primitive `type`, `enum`) rather than a full
/// draft-2020-12 implementation: it runs on every tool call, and a harness
/// cannot install a schema outside this subset.
pub fn validate_params(schema: &serde_json::Value, params: &serde_json::Value) -> Result<()> {
    let obj = match params.as_object() {
        Some(o) => o,
        None => bail!("params must be a JSON object"),
    };
    if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
        for r in req {
            if let Some(key) = r.as_str() {
                if !obj.contains_key(key) {
                    bail!("missing required parameter `{key}`");
                }
            }
        }
    }
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return Ok(()),
    };
    for (key, value) in obj {
        let Some(spec) = props.get(key) else {
            if schema.get("additionalProperties") == Some(&serde_json::Value::Bool(false)) {
                bail!("unknown parameter `{key}`");
            }
            continue;
        };
        if let Some(ty) = spec.get("type").and_then(|t| t.as_str()) {
            let ok = match ty {
                "string" => value.is_string(),
                "number" => value.is_number(),
                "integer" => value.is_i64() || value.is_u64(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                "null" => value.is_null(),
                _ => true,
            };
            if !ok {
                bail!("parameter `{key}` should be {ty}");
            }
        }
        if let Some(allowed) = spec.get("enum").and_then(|e| e.as_array()) {
            if !allowed.contains(value) {
                let names: Vec<String> = allowed.iter().map(|v| v.to_string()).collect();
                bail!("parameter `{key}` must be one of {}", names.join(", "));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "summary": summary,
            "params": {"type": "object", "properties": {}},
            "kind": "read"
        })
    }

    #[test]
    fn parses_a_well_formed_set() {
        let text = serde_json::to_string(&serde_json::json!([
            tool("canvas.list", "List the frames and shapes on the board."),
            {
                "name": "canvas.add_shape",
                "summary": "Add a rectangle, ellipse or arrow to the canvas.",
                "params": {"type": "object", "properties": {"kind": {"type": "string"}}, "required": ["kind"]},
                "kind": "write",
                "front_door": true,
                "undoable": true,
                "confirm": "never",
                "cost_hint": "instant"
            }
        ]))
        .unwrap();
        let set = ToolSet::parse(&text).unwrap();
        assert_eq!(set.tools.len(), 2);
        assert_eq!(set.front_door().count(), 1);
    }

    #[test]
    fn rejects_an_overlong_summary() {
        let long = (0..30).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let text = serde_json::to_string(&serde_json::json!([tool("a.b", &long)])).unwrap();
        let err = ToolSet::parse(&text).unwrap_err().to_string();
        assert!(err.contains("word budget") || err.contains("-word budget"), "got: {err}");
    }

    #[test]
    fn rejects_more_than_three_front_doors() {
        let mut arr = Vec::new();
        for i in 0..4 {
            arr.push(serde_json::json!({
                "name": format!("a.t{i}"),
                "summary": "does a thing",
                "params": {"type": "object", "properties": {}},
                "kind": "read",
                "front_door": true
            }));
        }
        let err = ToolSet::parse(&serde_json::to_string(&arr).unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.contains("front_door"), "got: {err}");
    }

    #[test]
    fn rejects_an_unrecoverable_write() {
        let text = serde_json::to_string(&serde_json::json!([{
            "name": "a.wipe",
            "summary": "Erase everything.",
            "params": {"type": "object", "properties": {}},
            "kind": "write",
            "undoable": false,
            "confirm": "never"
        }]))
        .unwrap();
        let err = ToolSet::parse(&text).unwrap_err().to_string();
        assert!(err.contains("undoable"), "got: {err}");
    }

    #[test]
    fn rejects_too_many_params() {
        let mut props = serde_json::Map::new();
        for i in 0..9 {
            props.insert(format!("p{i}"), serde_json::json!({"type": "string"}));
        }
        let text = serde_json::to_string(&serde_json::json!([{
            "name": "a.big",
            "summary": "Too many knobs.",
            "params": {"type": "object", "properties": props},
            "kind": "read"
        }]))
        .unwrap();
        let err = ToolSet::parse(&text).unwrap_err().to_string();
        assert!(err.contains("top-level params"), "got: {err}");
    }

    #[test]
    fn param_validation_catches_the_common_model_mistakes() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string", "enum": ["rect", "ellipse"]},
                "w": {"type": "number"}
            },
            "required": ["kind"]
        });
        validate_params(&schema, &serde_json::json!({"kind": "rect", "w": 10.0})).unwrap();

        let missing = validate_params(&schema, &serde_json::json!({"w": 10.0}));
        assert!(missing.unwrap_err().to_string().contains("required"));

        let bad_enum = validate_params(&schema, &serde_json::json!({"kind": "triangle"}));
        assert!(bad_enum.unwrap_err().to_string().contains("one of"));

        let bad_type = validate_params(&schema, &serde_json::json!({"kind": "rect", "w": "wide"}));
        assert!(bad_type.unwrap_err().to_string().contains("number"));
    }
}
