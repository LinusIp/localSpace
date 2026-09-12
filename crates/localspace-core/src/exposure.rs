//! Tool exposure — the scaling problem (spec §9).
//!
//! Twelve harnesses times fifteen tools is 180 tools, and every description is
//! paid for on every turn. The active set is computed per turn:
//!
//! 1. Focused harness: all of its tools.
//! 2. Pinned or touched harnesses: front-door tools only.
//! 3. Everything else: not in context, reachable through `find_capability`.
//!
//! Two ordering rules matter and pull in opposite directions:
//! *ranking* decides what survives the budget (focus first, then pins), while
//! *emission* is always sorted by harness id so the prompt prefix — and with it
//! the worker's KV prefix cache — survives a focus change (§16.1).

use crate::profile::ModelProfile;
use crate::registry::Registry;
use crate::tools::ToolDecl;
use localspace_proto as proto;
use localspace_proto::{ExposureReason, Json};

/// Core's own always-present tools. These are not owned by any harness.
pub const CORE_HARNESS: &str = "core";

pub struct Exposure<'a> {
    pub registry: &'a Registry,
    pub profile: &'a ModelProfile,
    pub focus: Option<&'a str>,
    pub pinned: &'a [String],
    /// Harnesses this conversation has already touched.
    pub touched: &'a [String],
    pub network: proto::NetworkMode,
}

impl Exposure<'_> {
    /// The tools the model will see this turn, plus what had to be dropped.
    pub fn active_set(&self) -> proto::ActiveSet {
        let mut candidates: Vec<(u8, proto::ExposedTool)> = Vec::new();

        // Rank 0 — Core builtins. Always present, never dropped.
        for t in self.core_tools() {
            candidates.push((0, t));
        }

        // Rank 1 — the focused harness, in full.
        if let Some(focus) = self.focus
            && let Some(h) = self.registry.get(focus)
            && h.enabled
        {
            for tool in &h.tools.tools {
                candidates.push((1, expose(h.id(), tool, ExposureReason::Focused)));
            }
        }

        // Rank 2 — pinned harnesses, front door only.
        // Rank 3 — harnesses this conversation has touched, front door only.
        for h in self.registry.iter() {
            if !h.enabled || Some(h.id()) == self.focus {
                continue;
            }
            let pinned = self.pinned.iter().any(|p| p == h.id());
            let touched = self.touched.iter().any(|t| t == h.id());
            if !pinned && !touched {
                continue;
            }
            let (rank, reason) = if pinned {
                (2, ExposureReason::Pinned)
            } else {
                (3, ExposureReason::Touched)
            };
            for tool in h.tools.front_door() {
                candidates.push((rank, expose(h.id(), tool, reason)));
            }
        }

        // Enforce the budget by dropping whole harnesses, lowest rank last-first.
        let budget = self.profile.tool_budget_tokens;
        let mut dropped: Vec<String> = Vec::new();
        loop {
            let total: usize = candidates.iter().map(|(_, t)| token_cost(t)).sum();
            if total <= budget {
                break;
            }
            // Find the lowest-priority harness still present (highest rank number),
            // breaking ties by the largest token cost.
            let victim = candidates
                .iter()
                .filter(|(rank, _)| *rank >= 2)
                .max_by_key(|(rank, t)| (*rank, token_cost(t)))
                .map(|(_, t)| t.harness.clone());

            if let Some(victim) = victim {
                candidates.retain(|(_, t)| t.harness != victim);
                dropped.push(victim);
                continue;
            }

            // Only Core builtins and the focused harness are left, and they still
            // do not fit. Trim the focused harness rather than silently overrun:
            // its front doors are the tools it most wants reachable, so they and
            // the builtins stay, and the widest of the rest goes first.
            let front_doors: Vec<&str> = self
                .focus
                .and_then(|f| self.registry.get(f))
                .map(|h| h.tools.front_door().map(|t| t.name.as_str()).collect())
                .unwrap_or_default();
            let widest = |keep_front_doors: bool| {
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, (rank, t))| {
                        *rank == 1 && !(keep_front_doors && front_doors.contains(&t.name.as_str()))
                    })
                    .max_by_key(|(_, (_, t))| token_cost(t))
                    .map(|(i, (_, t))| (i, t.name.clone()))
            };
            let trim = widest(true).or_else(|| widest(false));
            let Some((index, name)) = trim else {
                break; // Core builtins alone. Nothing left that may be dropped.
            };
            candidates.remove(index);
            dropped.push(name);
        }

        // Emission order: by harness id, then tool name. Stable across focus changes.
        let mut tools: Vec<proto::ExposedTool> = candidates.into_iter().map(|(_, t)| t).collect();
        tools.sort_by(|a, b| a.harness.cmp(&b.harness).then(a.name.cmp(&b.name)));

        let token_estimate = tools.iter().map(token_cost).sum();
        let grammar_hash = crate::grammar::active_set_hash(&tools);

        proto::ActiveSet {
            tools,
            token_estimate,
            budget,
            dropped,
            grammar_hash,
        }
    }

    /// Core's built-in tools. `web.*` is absent entirely under `airgapped`, so the
    /// model never proposes a search it cannot run.
    fn core_tools(&self) -> Vec<proto::ExposedTool> {
        let mut out = vec![builtin(
            "find_capability",
            "Search every installed harness for a tool that meets a stated need.",
            serde_json::json!({
                "type": "object",
                "properties": {"need": {"type": "string"}},
                "required": ["need"]
            }),
            proto::ToolKind::Read,
        )];

        // The ledger's two writes (spec §18.4): a plan across harnesses, and a
        // note for later. Cheap enough to be present on every turn.
        out.push(builtin(
            "task.plan",
            "Write the plan: one step per harness, in order, with the intent of each.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "harness": {"type": "string"},
                                "intent": {"type": "string"}
                            },
                            "required": ["harness", "intent"]
                        }
                    }
                },
                "required": ["steps"]
            }),
            proto::ToolKind::Write,
        ));
        out.push(builtin(
            "task.note",
            "Keep a short note in the task ledger: a decision, or an open question.",
            serde_json::json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            }),
            proto::ToolKind::Write,
        ));

        if self.network != proto::NetworkMode::Airgapped {
            out.push(builtin(
                "web.search",
                "Search the web. Results are titles, URLs and snippets.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "recency": {"type": "string", "enum": ["day", "week", "month", "year"]},
                        "site": {"type": "string"}
                    },
                    "required": ["query"]
                }),
                proto::ToolKind::Read,
            ));
            out.push(builtin(
                "web.fetch",
                "Fetch one URL and cache it in the workspace as a cited document.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "mode": {"type": "string", "enum": ["text", "raw"]}
                    },
                    "required": ["url"]
                }),
                proto::ToolKind::Read,
            ));
        }
        out
    }
}

fn expose(harness: &str, tool: &ToolDecl, reason: ExposureReason) -> proto::ExposedTool {
    proto::ExposedTool {
        harness: harness.to_string(),
        name: tool.name.clone(),
        summary: tool.summary.clone(),
        params: Json(tool.params.clone()),
        kind: tool.kind.into(),
        confirm: tool.confirm.into(),
        cost_hint: tool.cost_hint.into(),
        undoable: tool.undoable,
        reason,
    }
}

fn builtin(
    name: &str,
    summary: &str,
    params: serde_json::Value,
    kind: proto::ToolKind,
) -> proto::ExposedTool {
    proto::ExposedTool {
        harness: CORE_HARNESS.into(),
        name: name.into(),
        summary: summary.into(),
        params: Json(params),
        kind,
        confirm: proto::Confirm::Never,
        cost_hint: proto::CostHint::Seconds,
        undoable: false,
        reason: ExposureReason::CoreBuiltin,
    }
}

pub fn token_cost(t: &proto::ExposedTool) -> usize {
    proto::estimate_tokens(&t.name)
        + proto::estimate_tokens(&t.summary)
        + proto::estimate_tokens(&t.params.to_string())
}

// ---------------------------------------------------------------------------
// find_capability
// ---------------------------------------------------------------------------

/// Rank every installed tool against a stated need.
///
/// Scoring is BM25-style lexical matching over tool names and summaries. Where an
/// embedding model is loaded, Core reranks the top hits on the utility worker;
/// that path lives in `Core::find_capability` so this function stays pure.
pub fn rank_capabilities(registry: &Registry, need: &str) -> Vec<proto::CapabilityHit> {
    let terms = tokenize(need);
    if terms.is_empty() {
        return Vec::new();
    }

    // Document frequency across all installed tools.
    let mut docs: Vec<(String, String, String, Vec<String>)> = Vec::new();
    for h in registry.iter() {
        if !h.enabled {
            continue;
        }
        for t in &h.tools.tools {
            let text = format!(
                "{} {} {}",
                t.name.replace(['.', '_'], " "),
                t.summary,
                h.manifest.harness.title
            );
            docs.push((
                h.id().to_string(),
                t.name.clone(),
                t.summary.clone(),
                tokenize(&text),
            ));
        }
    }
    if docs.is_empty() {
        return Vec::new();
    }

    let n = docs.len() as f32;
    let avg_len = docs.iter().map(|d| d.3.len()).sum::<usize>() as f32 / n;

    let mut hits: Vec<proto::CapabilityHit> = docs
        .iter()
        .map(|(harness, tool, summary, tokens)| {
            let len = tokens.len() as f32;
            let mut score = 0.0_f32;
            for term in &terms {
                let tf = tokens.iter().filter(|t| *t == term).count() as f32;
                if tf == 0.0 {
                    continue;
                }
                let df = docs.iter().filter(|d| d.3.contains(term)).count().max(1) as f32;
                let idf = (((n - df + 0.5) / (df + 0.5)) + 1.0).ln();
                // BM25 with k1 = 1.2, b = 0.75.
                score += idf * (tf * 2.2) / (tf + 1.2 * (0.25 + 0.75 * len / avg_len));
            }
            proto::CapabilityHit {
                harness: harness.clone(),
                tool: tool.clone(),
                summary: summary.clone(),
                score,
            }
        })
        .filter(|h| h.score > 0.0)
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(8);
    hits
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .filter(|w| !STOP.contains(w))
        .map(|w| w.trim_end_matches('s').to_string())
        .collect()
}

const STOP: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "you", "can", "are", "was", "how", "get", "set",
    "use", "make", "want", "need", "into", "from",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;
    use crate::registry::Installed;
    use crate::tools::ToolSet;
    use std::path::PathBuf;

    fn harness(id: &str, tools: serde_json::Value) -> Installed {
        let manifest = Manifest::parse(&format!(
            r#"
[harness]
id = "{id}"
version = "1.0.0"
api = "^1.0"
title = "{id}"
publisher = "p"

[contributes]
tools = "tools.json"
context_provider = true
"#
        ))
        .unwrap();
        Installed {
            manifest,
            tools: ToolSet::parse(&tools.to_string()).unwrap(),
            dir: PathBuf::from("."),
            enabled: true,
            degraded: None,
            runtime: None,
            doc_id: id.replace('.', "_"),
            last_used: std::time::Instant::now(),
            idle_unload: std::time::Duration::from_secs(300),
            types: Default::default(),
        }
    }

    fn tool(name: &str, summary: &str, front_door: bool) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "summary": summary,
            "params": {"type": "object", "properties": {}},
            "kind": "read",
            "front_door": front_door
        })
    }

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.insert(harness(
            "io.localspace.whiteboard",
            serde_json::json!([
                tool("canvas.list", "List frames and shapes on the board.", true),
                tool(
                    "canvas.add_shape",
                    "Add a rectangle, ellipse or arrow.",
                    true
                ),
                tool("canvas.move", "Move a shape to new coordinates.", false),
                tool("canvas.delete", "Delete a shape from the board.", false),
            ]),
        ));
        r.insert(harness(
            "io.localspace.planner",
            serde_json::json!([
                tool("board.columns", "List columns and their WIP counts.", true),
                tool("board.add_card", "Add a card to a column.", false),
            ]),
        ));
        r.insert(harness(
            "io.localspace.physics",
            serde_json::json!([
                tool(
                    "sim.run",
                    "Run the rigid-body simulation and report results.",
                    true
                ),
                tool("sim.add_body", "Add a rigid body to the scene.", false),
            ]),
        ));
        r
    }

    fn exposure<'a>(
        reg: &'a Registry,
        profile: &'a ModelProfile,
        focus: Option<&'a str>,
        pinned: &'a [String],
        touched: &'a [String],
    ) -> Exposure<'a> {
        Exposure {
            registry: reg,
            profile,
            focus,
            pinned,
            touched,
            network: proto::NetworkMode::Ask,
        }
    }

    #[test]
    fn the_focused_harness_contributes_all_its_tools() {
        let reg = registry();
        let p = ModelProfile::server();
        let set = exposure(&reg, &p, Some("io.localspace.whiteboard"), &[], &[]).active_set();
        let names: Vec<&str> = set.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"canvas.move"), "{names:?}");
        assert!(names.contains(&"canvas.delete"));
    }

    #[test]
    fn an_unfocused_harness_contributes_only_its_front_door() {
        let reg = registry();
        let p = ModelProfile::server();
        let pinned = vec!["io.localspace.planner".to_string()];
        let set = exposure(&reg, &p, Some("io.localspace.whiteboard"), &pinned, &[]).active_set();
        let names: Vec<&str> = set.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"board.columns"), "front door is present");
        assert!(
            !names.contains(&"board.add_card"),
            "non-front-door must stay out"
        );
    }

    #[test]
    fn an_untouched_unpinned_harness_is_not_in_context_at_all() {
        let reg = registry();
        let p = ModelProfile::server();
        let set = exposure(&reg, &p, Some("io.localspace.whiteboard"), &[], &[]).active_set();
        assert!(
            !set.tools.iter().any(|t| t.harness.contains("physics")),
            "physics should be reachable only through find_capability"
        );
    }

    #[test]
    fn tools_are_emitted_in_canonical_order_so_the_prefix_cache_survives_focus() {
        // §16.1. Focusing a different harness must not reshuffle the prompt.
        let reg = registry();
        let p = ModelProfile::server();
        let pinned = vec![
            "io.localspace.planner".to_string(),
            "io.localspace.physics".to_string(),
        ];
        let a = exposure(&reg, &p, Some("io.localspace.planner"), &pinned, &[]).active_set();
        let b = exposure(&reg, &p, Some("io.localspace.physics"), &pinned, &[]).active_set();

        let harness_order = |s: &proto::ActiveSet| -> Vec<String> {
            let mut v: Vec<String> = s.tools.iter().map(|t| t.harness.clone()).collect();
            v.dedup();
            v
        };
        assert_eq!(harness_order(&a), harness_order(&b));
        assert!(
            harness_order(&a).windows(2).all(|w| w[0] <= w[1]),
            "sorted by id"
        );
    }

    #[test]
    fn airgapped_removes_the_web_tools_from_the_model_s_view() {
        let reg = registry();
        let p = ModelProfile::server();
        let mut e = exposure(&reg, &p, None, &[], &[]);
        e.network = proto::NetworkMode::Airgapped;
        let set = e.active_set();
        assert!(!set.tools.iter().any(|t| t.name.starts_with("web.")));
        // find_capability is always there.
        assert!(set.tools.iter().any(|t| t.name == "find_capability"));

        e.network = proto::NetworkMode::Ask;
        let set = e.active_set();
        assert!(set.tools.iter().any(|t| t.name == "web.search"));
    }

    #[test]
    fn the_budget_drops_pinned_harnesses_before_the_focused_one() {
        let reg = registry();
        let focused_only = exposure(
            &reg,
            &ModelProfile::server(),
            Some("io.localspace.whiteboard"),
            &[],
            &[],
        )
        .active_set()
        .token_estimate;
        // Room for the builtins and the whole focused harness, but nothing more.
        let tight = ModelProfile {
            tool_budget_tokens: focused_only,
            ..ModelProfile::small()
        };
        let pinned = vec![
            "io.localspace.planner".to_string(),
            "io.localspace.physics".to_string(),
        ];
        let set =
            exposure(&reg, &tight, Some("io.localspace.whiteboard"), &pinned, &[]).active_set();

        assert!(
            !set.dropped.is_empty(),
            "the trace must say what was dropped"
        );
        assert!(
            set.tools.iter().any(|t| t.name == "canvas.move"),
            "a pinned harness goes before any part of the focused one"
        );
        assert!(
            !set.dropped
                .contains(&"io.localspace.whiteboard".to_string())
        );
    }

    /// What Core's own always-present tools cost. Budgets in these tests are set
    /// relative to it, so they keep meaning if a builtin's description changes.
    fn builtin_cost(reg: &Registry) -> usize {
        exposure(reg, &ModelProfile::server(), None, &[], &[])
            .active_set()
            .token_estimate
    }

    #[test]
    fn a_focused_harness_that_will_not_fit_is_trimmed_not_overrun() {
        // A workstation profile can be tighter than one rich harness. Rather than
        // silently blow the budget, Core drops that harness's widest tools and
        // keeps its front doors, saying which went.
        let reg = registry();
        let tiny = ModelProfile {
            // Room for the builtins and a little else, but not the whole harness.
            tool_budget_tokens: builtin_cost(&reg) + 30,
            ..ModelProfile::small()
        };
        let set = exposure(&reg, &tiny, Some("io.localspace.whiteboard"), &[], &[]).active_set();

        assert!(
            set.token_estimate <= tiny.tool_budget_tokens,
            "still over budget at {} of {}",
            set.token_estimate,
            tiny.tool_budget_tokens
        );
        assert!(!set.dropped.is_empty(), "the trace must say what went");

        let names: Vec<&str> = set.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"find_capability"),
            "a Core builtin is never dropped: {names:?}"
        );
        // Front doors are what the harness most wants reachable, so a non-front-door
        // tool goes before one of them.
        assert!(
            set.dropped
                .iter()
                .any(|d| d == "canvas.move" || d == "canvas.delete"),
            "dropped {:?}",
            set.dropped
        );
    }

    #[test]
    fn the_budget_comes_from_the_profile() {
        let reg = registry();
        let set = exposure(
            &reg,
            &ModelProfile::w32(),
            Some("io.localspace.whiteboard"),
            &[],
            &[],
        )
        .active_set();
        assert_eq!(set.budget, 2500);
        let set = exposure(
            &reg,
            &ModelProfile::server(),
            Some("io.localspace.whiteboard"),
            &[],
            &[],
        )
        .active_set();
        assert_eq!(set.budget, 4000);
    }

    #[test]
    fn find_capability_reaches_a_harness_that_is_not_in_context() {
        let reg = registry();
        let hits = rank_capabilities(&reg, "simulate rigid bodies and report the result");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].harness, "io.localspace.physics", "{hits:#?}");
        assert!(hits[0].tool.starts_with("sim."));
    }

    #[test]
    fn find_capability_ranks_the_right_harness_for_a_drawing_need() {
        let reg = registry();
        let hits = rank_capabilities(&reg, "draw a rectangle on a canvas");
        assert_eq!(hits[0].harness, "io.localspace.whiteboard", "{hits:#?}");
    }

    #[test]
    fn a_need_that_matches_nothing_returns_nothing() {
        let reg = registry();
        assert!(rank_capabilities(&reg, "xylophone tuning").is_empty());
        assert!(rank_capabilities(&reg, "").is_empty());
    }
}
