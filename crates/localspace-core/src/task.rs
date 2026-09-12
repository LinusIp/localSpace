//! The task ledger (spec §18.1): the one block of context that follows the
//! agent across harnesses.
//!
//! When the focused harness changes, the tools change; the ledger does not. It
//! carries the goal, the plan, and every artifact produced so far — each a DAG
//! reference, not a copy — so the agent in the planning board knows the outline
//! it is importing is `art_1`, produced by the whiteboard at a pinned commit,
//! with the summary the whiteboard's own provider wrote for it.

use crate::context::truncate_to_budget;
use localspace_proto as proto;

/// Render the ledger for the prompt, within the profile's ledger budget.
pub fn render(task: &proto::Task, budget_tokens: usize) -> String {
    if task.goal.is_empty() && task.plan.is_empty() && task.artifacts.is_empty() {
        return String::new();
    }
    let mut out = format!("[task {}]\ngoal: {}\n", task.id, task.goal);

    if !task.plan.is_empty() {
        out.push_str("plan:\n");
        for (i, step) in task.plan.iter().enumerate() {
            let mark = match step.status {
                proto::StepStatus::Pending => "[ ]",
                proto::StepStatus::Active => "[>]",
                proto::StepStatus::Done => "[x]",
                proto::StepStatus::Failed => "[!]",
            };
            out.push_str(&format!(
                "  {}. {mark} {} — {}\n",
                i + 1,
                step.harness,
                step.intent
            ));
        }
    }

    if !task.artifacts.is_empty() {
        out.push_str("artifacts:\n");
        for a in &task.artifacts {
            let at = if a.commit.is_empty() {
                String::new()
            } else {
                format!(" @{}", a.commit.chars().take(7).collect::<String>())
            };
            out.push_str(&format!(
                "  {} {} from {}{at}: {}\n",
                a.id, a.kind, a.produced_by, a.summary
            ));
        }
    }

    if !task.notes.is_empty() {
        out.push_str("notes:\n");
        for n in &task.notes {
            out.push_str(&format!("  - {n}\n"));
        }
    }
    if !task.citations.is_empty() {
        out.push_str("citations:\n");
        for c in &task.citations {
            out.push_str(&format!("  - {c}\n"));
        }
    }

    truncate_to_budget(out.trim_end(), budget_tokens)
}

/// `art_1`, `art_2`, … — unique within a ledger, and readable in a prompt.
pub fn next_artifact_id(task: &proto::Task) -> String {
    let max = task
        .artifacts
        .iter()
        .filter_map(|a| a.id.strip_prefix("art_")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("art_{}", max + 1)
}

/// Which installed harnesses could take an artifact of this kind. Used to
/// phrase a refusal as a suggestion ("the planning board accepts outline.v1").
pub fn who_accepts<'a>(registry: &'a crate::registry::Registry, kind: &str) -> Vec<&'a str> {
    registry
        .iter()
        .filter(|h| h.enabled && h.manifest.contributes.accepts.iter().any(|k| k == kind))
        .map(|h| h.id())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> proto::Task {
        proto::Task {
            id: "run_1".into(),
            goal: "put the risks on the planning board".into(),
            plan: vec![
                proto::Step {
                    harness: "io.localspace.whiteboard".into(),
                    intent: "export the risk stickies".into(),
                    status: proto::StepStatus::Done,
                },
                proto::Step {
                    harness: "io.localspace.planner".into(),
                    intent: "import them as cards".into(),
                    status: proto::StepStatus::Active,
                },
            ],
            artifacts: vec![proto::Artifact {
                id: "art_1".into(),
                kind: "outline.v1".into(),
                doc: "io_localspace_whiteboard".into(),
                commit: "c1a2b3c4d5e6".into(),
                summary: "3 red stickies in frame Risks".into(),
                produced_by: "io.localspace.whiteboard".into(),
                fields: proto::Json::object(),
            }],
            notes: vec!["FX exposure needs a mitigation owner".into()],
            citations: Vec::new(),
        }
    }

    #[test]
    fn the_ledger_reads_as_one_compact_block() {
        let text = render(&task(), 600);
        assert!(text.starts_with("[task run_1]"));
        assert!(text.contains("goal: put the risks"));
        assert!(text.contains("1. [x] io.localspace.whiteboard"));
        assert!(text.contains("2. [>] io.localspace.planner"));
        assert!(text.contains("art_1 outline.v1 from io.localspace.whiteboard @c1a2b3c"));
        assert!(text.contains("- FX exposure"));
    }

    #[test]
    fn an_empty_ledger_renders_nothing_so_the_prefix_stays_clean() {
        assert_eq!(render(&proto::Task::default(), 600), "");
    }

    #[test]
    fn the_ledger_respects_its_budget() {
        let mut t = task();
        for i in 0..200 {
            t.notes
                .push(format!("note number {i} with enough words to cost tokens"));
        }
        let text = render(&t, 120);
        assert!(
            proto::estimate_tokens(&text) <= 130,
            "{}",
            proto::estimate_tokens(&text)
        );
        assert!(text.contains("truncated"));
        // The goal always survives: it is the first line.
        assert!(text.contains("goal:"));
    }

    #[test]
    fn artifact_ids_count_up_from_the_highest_present() {
        let mut t = task();
        assert_eq!(next_artifact_id(&t), "art_2");
        t.artifacts.clear();
        assert_eq!(next_artifact_id(&t), "art_1");
    }
}
