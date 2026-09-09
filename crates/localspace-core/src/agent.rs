//! The agent loop.
//!
//! One turn is: assemble the active set and the context blocks, build the prompt
//! with its stable prefix, ask the model, run whatever tools it asked for through
//! `Core::call_tool`, and repeat until it stops asking. Every write it makes is a
//! DAG commit tagged with the run, so rejecting the whole run is a branch drop
//! rather than forty undos.

use crate::{model, prompt, Core, Proposal, MAX_AGENT_STEPS};
use localspace_proto as proto;
use localspace_proto::Json;
use serde_json::Value as J;
use std::sync::atomic::Ordering;

/// Start a turn from a user message.
pub fn turn(core: &mut Core, text: &str) {
    core.transcript.push(proto::ChatMessage {
        role: proto::Role::User,
        content: text.to_string(),
        tool_calls: Vec::new(),
    });
    let run = format!("run_{}", crate::dag::now_ms());
    core.run = Some(run.clone());

    // A fresh ledger for this run. Artifacts carry over: what an earlier turn
    // produced is still addressable, which is what makes "now put those on the
    // board" work across turns.
    let carried = std::mem::take(&mut core.task.artifacts);
    core.task = proto::Task {
        id: run,
        goal: text.to_string(),
        artifacts: carried,
        ..Default::default()
    };
    core.emit_task();

    run_loop(core, 0);
}

/// Continue a turn that stopped at a confirmation gate.
pub fn resume(core: &mut Core, tool: &str, params: &J, outcome: proto::ToolOutcome) {
    record_call(core, tool, params, outcome);
    run_loop(core, 1);
}

fn run_loop(core: &mut Core, mut steps: usize) {
    loop {
        if steps >= MAX_AGENT_STEPS {
            core.notice(
                proto::NoticeLevel::Warn,
                format!("stopped after {MAX_AGENT_STEPS} tool calls in one turn"),
            );
            break;
        }

        let active = core.active_set();
        if !active.dropped.is_empty() {
            core.trace(format!(
                "tool budget {}/{}: dropped {}",
                active.token_estimate,
                active.budget,
                active.dropped.join(", ")
            ));
        }
        let blocks = core.context_blocks();
        let p = prompt::build(
            &core.cfg.profile,
            &active,
            &blocks,
            Some(&core.task),
            &core.transcript,
        );

        if p.total_tokens() > core.cfg.profile.prompt_tokens_per_step {
            core.trace(format!(
                "prompt is {} tokens, over the {}-token step budget for this profile",
                p.total_tokens(),
                core.cfg.profile.prompt_tokens_per_step
            ));
        }

        let grammar = core.grammars.compile(&active.tools);
        core.trace(format!(
            "active set {} tools, grammar {}",
            active.tools.len(),
            grammar.hash
        ));

        let request = model::ChatRequest {
            prompt: p.render(),
            tools: active.tools.clone(),
            grammar: Some(grammar.gbnf),
            max_tokens: 1024,
            temperature: 0.2,
            class: model::RequestClass::Interactive,
        };

        // Tokens reach the shell as they arrive (v2 step 3) — unless the model
        // is producing a tool call in the grammar's shape, which is not for
        // reading and is handled whole below.
        let mut streamed = 0usize;
        let mut seen = String::new();
        let mut calling: Option<bool> = None;
        let reply = {
            let router = core.router.read().unwrap();
            router.chat_streaming(model::WorkerRole::Chat, &request, &mut |delta: &str| {
                seen.push_str(delta);
                if calling.is_none() {
                    if let Some(first) = seen.trim_start().chars().next() {
                        calling = Some(first == '{');
                    }
                }
                if calling == Some(false) {
                    streamed += delta.len();
                    core.emit(proto::Event::AssistantDelta {
                        text: delta.to_string(),
                    });
                }
            })
        };

        let reply = match reply {
            Ok(r) => r,
            Err(e) => {
                let message = format!("{e:#}");
                core.notice(proto::NoticeLevel::Error, message.clone());
                core.transcript.push(proto::ChatMessage {
                    role: proto::Role::Assistant,
                    content: format!("I could not reach a model: {message}"),
                    tool_calls: Vec::new(),
                });
                core.emit(proto::Event::AssistantDone);
                break;
            }
        };

        if reply.calls.is_empty() {
            // Streamed already, unless it was held back as a possible tool call.
            if !reply.text.is_empty() && streamed == 0 {
                core.emit(proto::Event::AssistantDelta {
                    text: reply.text.clone(),
                });
            }
            core.transcript.push(proto::ChatMessage {
                role: proto::Role::Assistant,
                content: reply.text,
                tool_calls: Vec::new(),
            });
            core.emit(proto::Event::AssistantDone);
            break;
        }

        let mut paused = false;
        for call in reply.calls {
            core.emit(proto::Event::ToolCallStarted {
                id: call.id.clone(),
                tool: call.tool.clone(),
                params: Json(call.params.clone()),
            });

            {
                let router = core.router.read().unwrap();
                router.metrics.tool_calls.fetch_add(1, Ordering::Relaxed);
            }

            let outcome = core.call_tool(&call.tool, &call.params, proto::Author::Agent);
            core.task_progress(&call.tool, &outcome);

            if let proto::ToolOutcome::Error { message } = &outcome {
                // A call Core could not even attempt is a malformed call, and
                // the §16.5 budget for those is under 0.5 %.
                if message.contains("required")
                    || message.contains("should be")
                    || message.contains("no tool named")
                    || message.contains("must be one of")
                {
                    let router = core.router.read().unwrap();
                    router
                        .metrics
                        .malformed_tool_calls
                        .fetch_add(1, Ordering::Relaxed);
                }
            }

            core.emit(proto::Event::ToolCallFinished {
                id: call.id.clone(),
                tool: call.tool.clone(),
                outcome: outcome.clone(),
            });

            let awaiting = matches!(outcome, proto::ToolOutcome::AwaitingConfirm { .. });
            record_call(core, &call.tool, &call.params, outcome);
            if awaiting {
                // The user has to answer before anything else happens.
                paused = true;
                break;
            }
        }
        if paused {
            core.record_conversation();
            return;
        }
        steps += 1;
    }

    finish_run(core);
    core.record_conversation();
}

fn record_call(core: &mut Core, tool: &str, params: &J, outcome: proto::ToolOutcome) {
    let record = proto::ToolCallRecord {
        id: format!("t{}", core.transcript.len()),
        tool: tool.to_string(),
        params: Json(params.clone()),
        outcome,
    };
    core.transcript.push(proto::ChatMessage {
        role: proto::Role::Tool,
        content: String::new(),
        tool_calls: vec![record],
    });
}

/// Close the run. In a shared workspace an agent's writes become a proposal
/// rather than landing on the head.
fn finish_run(core: &mut Core) {
    let Some(run) = core.run.take() else { return };

    // Steps the agent was working in are done when the turn ends.
    let mut settled = false;
    for step in core.task.plan.iter_mut() {
        if step.status == proto::StepStatus::Active {
            step.status = proto::StepStatus::Done;
            settled = true;
        }
    }
    if settled {
        core.emit_task();
    }

    let commits = core.dag.history(256).unwrap_or_default();
    let mine: Vec<&proto::Commit> = commits
        .iter()
        .filter(|c| c.run.as_deref() == Some(run.as_str()))
        .collect();
    if mine.is_empty() {
        return;
    }

    let mut docs: Vec<String> = mine.iter().map(|c| c.doc.clone()).collect();
    docs.sort();
    docs.dedup();

    let proposal_docs: Vec<String> = docs
        .iter()
        .filter(|d| core.access.agent_writes(d) == crate::acl::AgentWrites::Proposal)
        .cloned()
        .collect();

    if !proposal_docs.is_empty() {
        let summary = mine
            .iter()
            .map(|c| c.diff_summary.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        core.proposals.push(Proposal {
            run: run.clone(),
            docs: proposal_docs,
            summary: summary.clone(),
            by: core.cfg.user.clone(),
        });
        core.notice(
            proto::NoticeLevel::Info,
            format!("1 proposal from your agent: {summary} — apply or discard it"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChatReply, ChatRequest, ModelWorker, ProposedCall};
    use anyhow::Result;
    use std::sync::{Arc, Mutex};

    /// A worker that replays a fixed script of replies, so the loop itself is
    /// what is under test rather than a model.
    struct Script {
        replies: Mutex<Vec<ChatReply>>,
        seen: Mutex<Vec<String>>,
    }

    impl Script {
        fn new(replies: Vec<ChatReply>) -> Arc<Script> {
            Arc::new(Script {
                replies: Mutex::new(replies),
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    impl ModelWorker for Script {
        fn info(&self) -> proto::ModelInfo {
            proto::ModelInfo {
                id: "script".into(),
                backend: "test".into(),
                context_len: 8192,
                supports_tools: true,
                supports_vision: false,
                loaded: true,
            }
        }
        fn chat(&self, req: &ChatRequest) -> Result<ChatReply> {
            self.seen.lock().unwrap().push(req.prompt.clone());
            let mut r = self.replies.lock().unwrap();
            if r.is_empty() {
                Ok(ChatReply {
                    text: "done".into(),
                    ..Default::default()
                })
            } else {
                Ok(r.remove(0))
            }
        }
        fn embed(&self, _t: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(Vec::new())
        }
    }

    fn call(tool: &str, params: serde_json::Value) -> ChatReply {
        ChatReply {
            text: String::new(),
            calls: vec![ProposedCall {
                id: "c0".into(),
                tool: tool.into(),
                params,
            }],
            prompt_tokens: 100,
            completion_tokens: 10,
        }
    }

    fn core_with(script: Arc<Script>) -> Core {
        let core = Core::ephemeral("anna").unwrap();
        core.router.write().unwrap().chat = Some(script);
        core
    }

    #[test]
    fn a_turn_with_no_tool_calls_answers_and_stops() {
        let script = Script::new(vec![ChatReply {
            text: "There is nothing to change.".into(),
            ..Default::default()
        }]);
        let mut core = core_with(script);
        turn(&mut core, "what is on the board?");

        let last = core.transcript.last().unwrap();
        assert_eq!(last.role, proto::Role::Assistant);
        assert_eq!(last.content, "There is nothing to change.");
    }

    #[test]
    fn a_tool_the_model_invents_is_refused_and_counted_as_malformed() {
        let script = Script::new(vec![
            call("does.not.exist", serde_json::json!({})),
            ChatReply {
                text: "I could not do that.".into(),
                ..Default::default()
            },
        ]);
        let mut core = core_with(script);
        turn(&mut core, "do something impossible");

        let refused = core
            .transcript
            .iter()
            .flat_map(|m| &m.tool_calls)
            .any(|c| matches!(&c.outcome, proto::ToolOutcome::Error { message } if message.contains("no tool named")));
        assert!(refused, "{:#?}", core.transcript);

        let router = core.router.read().unwrap();
        assert_eq!(router.metrics.malformed_tool_calls.load(Ordering::Relaxed), 1);
        assert_eq!(router.metrics.tool_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn the_loop_stops_after_the_step_ceiling() {
        // A model that asks for the same tool forever must not run forever.
        let replies: Vec<ChatReply> = (0..100)
            .map(|_| call("find_capability", serde_json::json!({"need": "anything"})))
            .collect();
        let mut core = core_with(Script::new(replies));
        turn(&mut core, "loop please");

        let calls = core
            .transcript
            .iter()
            .flat_map(|m| &m.tool_calls)
            .count();
        assert!(calls <= MAX_AGENT_STEPS, "ran {calls} tool calls");
    }

    #[test]
    fn a_model_that_cannot_be_reached_is_reported_not_hidden() {
        let mut core = Core::ephemeral("anna").unwrap(); // no worker at all
        turn(&mut core, "hello");
        let last = core.transcript.last().unwrap();
        assert_eq!(last.role, proto::Role::Assistant);
        assert!(last.content.contains("could not reach a model"), "{}", last.content);
    }

    #[test]
    fn the_prompt_the_model_sees_carries_the_tools_and_the_state() {
        let script = Script::new(vec![ChatReply {
            text: "ok".into(),
            ..Default::default()
        }]);
        let seen = script.clone();
        let mut core = core_with(script);
        turn(&mut core, "hello");

        let prompts = seen.seen.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("[tools]"));
        assert!(prompts[0].contains("find_capability"));
        assert!(prompts[0].contains("[conversation]"));
        assert!(prompts[0].contains("hello"));
    }
}
