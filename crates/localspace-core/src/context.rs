//! Context providers — the part most plugin systems miss (spec §4.3).
//!
//! Core calls every relevant harness before each model turn and asks for a text
//! serialization of its state at a token budget. Two efficiency rules from §16.1
//! are enforced here rather than left to harness authors:
//!
//! * A provider is not re-run when its document has not changed since the last
//!   turn. The cache key is the document's content hash, so unchanged blocks come
//!   back byte-identical, which is what keeps the worker's prefix cache warm.
//! * Blocks are budgeted per model profile and truncated on a line boundary, with
//!   an explicit marker, rather than being allowed to overrun.

use crate::docs::DocStore;
use crate::profile::ModelProfile;
use crate::registry::Registry;
use localspace_proto as proto;
use std::collections::HashMap;

#[derive(Default)]
pub struct ProviderCache {
    /// (harness, doc hash, budget) -> block text
    entries: HashMap<(String, String, usize), proto::ContextBlock>,
    pub hits: u64,
    pub misses: u64,
}

impl ProviderCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }
}

/// Gather the context blocks for one turn.
///
/// `focus` gets the focused budget; each pinned or touched harness gets the
/// smaller pinned budget; the total is capped by the profile's context budget.
#[allow(clippy::too_many_arguments)]
pub fn assemble(
    registry: &mut Registry,
    docs: &mut DocStore,
    cache: &mut ProviderCache,
    profile: &ModelProfile,
    focus: Option<&str>,
    also: &[String],
    doc_ids: &std::collections::HashMap<String, proto::DocId>,
) -> Vec<proto::ContextBlock> {
    let mut wanted: Vec<(String, usize)> = Vec::new();
    if let Some(f) = focus {
        wanted.push((f.to_string(), profile.focused_context_tokens));
    }
    for id in also {
        if Some(id.as_str()) == focus {
            continue;
        }
        wanted.push((id.clone(), profile.pinned_context_tokens));
    }

    let mut blocks = Vec::new();
    let mut spent = 0usize;

    for (id, budget) in wanted {
        if spent >= profile.context_budget_tokens {
            break;
        }
        let budget = budget.min(profile.context_budget_tokens - spent);

        let Some(h) = registry.get(&id) else { continue };
        if !h.enabled || !h.manifest.contributes.context_provider {
            continue;
        }
        let Some(doc_id) = doc_ids.get(&id).cloned() else {
            continue;
        };
        let doc = docs.json(&doc_id).unwrap_or(serde_json::Value::Null);
        let doc_hash = blake3::hash(doc.to_string().as_bytes()).to_hex()[..16].to_string();

        let key = (id.clone(), doc_hash, budget);
        if let Some(cached) = cache.entries.get(&key) {
            cache.hits += 1;
            spent += cached.tokens;
            blocks.push(cached.clone());
            continue;
        }
        cache.misses += 1;

        let Some(h) = registry.get_mut(&id) else {
            continue;
        };
        let focused = Some(id.as_str()) == focus;
        let (text, expandable) = match h.runtime.as_mut() {
            Some(rt) => rt
                .context(budget, focused, &doc)
                .unwrap_or_else(|e| (format!("[context provider failed: {e}]"), false)),
            None => (String::new(), false),
        };
        if text.trim().is_empty() {
            continue;
        }

        let text = truncate_to_budget(&text, budget);
        let block = proto::ContextBlock {
            harness: id.clone(),
            tokens: proto::estimate_tokens(&text),
            text,
            expandable,
        };
        spent += block.tokens;
        cache.entries.insert(key, block.clone());
        blocks.push(block);
    }

    blocks
}

/// Cut a provider's text to its budget on a line boundary, saying so.
pub fn truncate_to_budget(text: &str, budget_tokens: usize) -> String {
    if proto::estimate_tokens(text) <= budget_tokens {
        return text.to_string();
    }
    let marker =
        "\n[truncated to fit the context budget — call the harness's zoom tool for detail]";
    let room = budget_tokens.saturating_sub(proto::estimate_tokens(marker));
    let mut out = String::new();
    for line in text.lines() {
        let candidate = proto::estimate_tokens(&out) + proto::estimate_tokens(line) + 1;
        if candidate > room {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(marker.trim_start_matches('\n'));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_within_budget_is_returned_unchanged() {
        let text = "frames: 2\nshapes: 7";
        assert_eq!(truncate_to_budget(text, 100), text);
    }

    #[test]
    fn overlong_text_is_cut_on_a_line_boundary_and_says_so() {
        let text = (0..200)
            .map(|i| format!("shape {i}: rect at (10, {i}) labelled something"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_to_budget(&text, 100);
        assert!(
            proto::estimate_tokens(&out) <= 110,
            "got {} tokens",
            proto::estimate_tokens(&out)
        );
        assert!(out.contains("truncated to fit the context budget"));
        // Cut on a boundary: no half-written line before the marker.
        let body = out.split("[truncated").next().unwrap();
        assert!(
            body.lines()
                .all(|l| l.is_empty() || l.starts_with("shape "))
        );
    }

    #[test]
    fn the_cache_reports_hits_and_misses() {
        let mut cache = ProviderCache::new();
        assert_eq!(cache.hit_rate(), 0.0);
        cache.hits = 8;
        cache.misses = 2;
        assert!((cache.hit_rate() - 0.8).abs() < 1e-6);
    }
}
