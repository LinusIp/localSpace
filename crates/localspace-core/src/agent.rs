//! The agent loop.
//!
//! One turn is: assemble the active set and the context blocks, build the prompt
//! with its stable prefix, ask the model, run whatever tools it asked for through
//! `Core::call_tool`, and repeat until it stops asking. Every write it makes is a
//! DAG commit tagged with the run, so rejecting the whole run is a branch drop
//! rather than forty undos.
//!
//! A turn is work beside Core's queue (`crate::turns`): each model step is read
//! on a thread of its own and comes back as [`Internal::Replied`], and each tool
//! call, and the step after the last of them, comes back as [`Internal::GoOn`].
//! Where no transport runs Core (evals, tests, the command line) the same steps
//! are taken inside the request that started the turn, to its end.

use crate::stream::{Cut, Silence, Stop};
use crate::turns::{Internal, StepEnd, Turn};
use crate::{Core, MAX_AGENT_STEPS, Proposal, To, model, prompt};
use localspace_proto as proto;
use localspace_proto::{Json, TurnState};
use serde_json::Value as J;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Where an answer's events go: the events of Core's own sink, to the
/// answer's person, from whichever thread reads the model.
type Delivery = Arc<dyn Fn(proto::Event) + Send + Sync>;

/// A message in the chat on screen, answered to its end before this
/// returns, whatever runs Core: evals read the document right after it.
pub fn turn(core: &mut Core, text: &str) {
    let before = std::mem::replace(&mut core.run_inline, true);
    let _ = send(core, None, text);
    core.run_inline = before;
}

/// A person's message in one of their chats: kept, and answered now, or when
/// the answer before it is done.
pub fn send(core: &mut Core, conversation: Option<String>, text: &str) -> proto::Response {
    let conversation = match chat_of_the_caller(core, conversation) {
        Ok(conversation) => conversation,
        Err(refused) => return refusal(refused),
    };
    if turn_in(core, &conversation).is_some() {
        return refusal(
            "An answer is still being written in this chat. Stop it, or wait for it to finish.",
        );
    }
    let (user, workspace) = (core.active.user.clone(), core.workspace.clone());
    write(core, &user, &workspace, &conversation, |messages| {
        messages.push(message(proto::Role::User, text));
    });
    queue(core, conversation, false);
    drain(core);
    transcript(core)
}

/// Carry on the answer that stopped in a chat, as part of that answer: its
/// words join the ones that came.
pub fn continue_answer(core: &mut Core, conversation: Option<String>) -> proto::Response {
    let conversation = match chat_of_the_caller(core, conversation) {
        Ok(conversation) => conversation,
        Err(refused) => return refusal(refused),
    };
    if turn_in(core, &conversation).is_some() {
        return refusal("An answer is still being written in this chat.");
    }
    let (user, workspace) = (core.active.user.clone(), core.workspace.clone());
    let stopped = messages(core, &user, &workspace, &conversation)
        .last()
        .is_some_and(|m| m.role == proto::Role::Assistant && m.stopped);
    if !stopped {
        return refusal("There is no stopped answer to carry on in this chat.");
    }
    queue(core, conversation, true);
    drain(core);
    transcript(core)
}

/// Stop the answer being written, or waiting, in one of the caller's chats.
/// Stop takes effect at a safe point, never in the middle of a change: the
/// model's step ends within a fifth of a second and what came is kept; a tool
/// call at work finishes, and the next is not begun; an approval waited on is
/// withdrawn, and the call it proposed is not made. Nothing to stop is not an
/// error: a second click finds the answer stopped already.
pub fn cancel(core: &mut Core, conversation: Option<String>) -> proto::Response {
    let conversation = conversation.unwrap_or_else(|| core.conversations.current.clone());
    let Some(id) = turn_in(core, &conversation) else {
        return proto::Response::Ok;
    };
    let Some(state) = stop(core, id) else {
        return proto::Response::Ok;
    };
    // Being written, it ends at its own safe point: see `replied` and `go_on`.
    if state != TurnState::Writing {
        end(core, id, TurnState::Stopped);
    }
    drain(core);
    proto::Response::Ok
}

/// A chat that is being deleted: its answer ends now, and nothing more of it
/// is kept.
pub fn forget(core: &mut Core, conversation: &str) {
    if let Some(id) = turn_in(core, conversation) {
        stop(core, id);
        end(core, id, TurnState::Stopped);
        drain(core);
    }
}

/// The caller's chats whose answers are being written, or wait.
pub fn list(core: &Core) -> proto::Response {
    proto::Response::Turns {
        list: core
            .turns
            .iter()
            .filter(|t| t.caller.user == core.active.user)
            .map(|t| proto::TurnInfo {
                conversation: t.conversation.clone(),
                state: t.state,
            })
            .collect(),
    }
}

/// The person allowed the call the turn waited on: it was made, and the turn
/// goes on.
pub fn resume(
    core: &mut Core,
    turn: Option<u64>,
    tool: &str,
    params: &J,
    outcome: proto::ToolOutcome,
) {
    let Some(id) = turn.filter(|id| index(core, *id).is_some()) else {
        return;
    };
    record_call(core, id, tool, params, outcome);
    set_state(core, id, TurnState::Writing);
    post(core, Internal::GoOn { turn: id });
    drain(core);
}

/// The person declined the call the turn waited on. It is not made, and the
/// answer stops there, since a small model told to carry on proposes the
/// same change again; the proposal stays in the chat marked as declined,
/// which is what the model reads next (docs/DECISIONS.md, 2026-09-24).
pub fn declined(core: &mut Core, turn: Option<u64>, tool: &str) {
    let Some(i) = turn.and_then(|id| index(core, id)) else {
        return;
    };
    let turn = &core.turns[i];
    let (id, user, workspace, conversation) = (
        turn.id,
        turn.caller.user.clone(),
        turn.workspace.clone(),
        turn.conversation.clone(),
    );
    write(core, &user, &workspace, &conversation, |messages| {
        let waiting = messages
            .iter_mut()
            .rev()
            .flat_map(|m| m.tool_calls.iter_mut())
            .find(|c| {
                c.tool == tool && matches!(c.outcome, proto::ToolOutcome::AwaitingConfirm { .. })
            });
        if let Some(call) = waiting
            && let proto::ToolOutcome::AwaitingConfirm { prompt } = &call.outcome
        {
            let prompt = prompt.clone();
            call.outcome = proto::ToolOutcome::Declined { prompt };
        }
    });
    end(core, id, TurnState::Stopped);
    drain(core);
}

/// A turn's work, come back through Core's queue.
pub fn internal(core: &mut Core, message: Internal) {
    handle(core, message);
    drain(core);
}

fn handle(core: &mut Core, message: Internal) {
    match message {
        Internal::Replied { turn, step } => replied(core, turn, step),
        Internal::GoOn { turn } => go_on(core, turn),
    }
}

/// What a Core that no transport runs has left to do, done now.
fn drain(core: &mut Core) {
    while let Some(message) = core.inline.pop_front() {
        handle(core, message);
    }
}

/// Into Core's queue, or, where no transport runs it, onto what is left to
/// do before the request returns.
fn post(core: &mut Core, message: Internal) {
    match core.inbox.clone().filter(|_| !core.run_inline) {
        Some(inbox) => inbox(message),
        None => core.inline.push_back(message),
    }
}

/// The chat a request is about: the one it names, which must be one of the
/// caller's in the workspace they are in, or the one on screen. Refused, the
/// words that say why.
fn chat_of_the_caller(core: &Core, named: Option<String>) -> Result<String, String> {
    let id = named.unwrap_or_else(|| core.conversations.current.clone());
    if core.conversations.conversations.iter().any(|c| c.id == id) {
        Ok(id)
    } else {
        Err(format!("no conversation `{id}`"))
    }
}

/// The caller's answer in `conversation`, being written or waiting.
fn turn_in(core: &Core, conversation: &str) -> Option<u64> {
    core.turns
        .iter()
        .find(|t| t.caller.user == core.active.user && t.conversation == conversation)
        .map(|t| t.id)
}

fn index(core: &Core, id: u64) -> Option<usize> {
    core.turns.iter().position(|t| t.id == id)
}

/// Asks the turn to stop, and says where it was.
fn stop(core: &Core, id: u64) -> Option<TurnState> {
    let turn = &core.turns[index(core, id)?];
    turn.stop.stop();
    Some(turn.state)
}

/// A new answer for `conversation`, as the caller: begun now, or waiting.
fn queue(core: &mut Core, conversation: String, continuing: bool) {
    let id = core.next_turn;
    core.next_turn += 1;
    core.turns.push(Turn {
        id,
        caller: core.active.clone(),
        workspace: core.workspace.clone(),
        conversation: conversation.clone(),
        state: TurnState::WaitsForAnotherChat,
        begun: false,
        steps: 0,
        stop: Stop::default(),
        calls: VecDeque::new(),
        continuing,
    });
    let state = where_it_goes(core, core.turns.len() - 1);
    if let Some(i) = index(core, id) {
        core.turns[i].state = state;
    }
    core.emit(proto::Event::TurnChanged {
        conversation,
        state,
    });
    if state == TurnState::Writing {
        begin(core, id);
    }
}

/// Where the answer at `i` stands: behind an older one of its person's
/// (one answer at a time for a person, even with a slot free: on a shared
/// server a slot is someone else's turn), behind the answers that fill the
/// engine's slots, or written now.
fn where_it_goes(core: &Core, i: usize) -> TurnState {
    let user = &core.turns[i].caller.user;
    if core.turns[..i].iter().any(|t| &t.caller.user == user) {
        TurnState::WaitsForAnotherChat
    } else if core
        .turns
        .iter()
        .filter(|t| t.state == TurnState::Writing)
        .count()
        >= core.cfg.slots.max(1)
    {
        TurnState::WaitsForTheModel
    } else {
        TurnState::Writing
    }
}

/// Whoever waits and may start now, starts; the others are told where they
/// stand when that changes.
fn schedule(core: &mut Core) {
    loop {
        let next = (0..core.turns.len()).find_map(|i| {
            let turn = &core.turns[i];
            let now = where_it_goes(core, i);
            (turn.waiting() && now != turn.state).then(|| (turn.id, turn.caller.clone(), now))
        });
        let Some((id, caller, state)) = next else {
            return;
        };
        core.activate(&caller);
        set_state(core, id, state);
        if state == TurnState::Writing {
            begin(core, id);
        }
    }
}

/// The turn leaves the queue: its run and its ledger are made, and the model
/// is asked.
fn begin(core: &mut Core, id: u64) {
    let Some(i) = index(core, id) else {
        return;
    };
    core.turns[i].begun = true;
    let turn = &core.turns[i];
    let (caller, workspace, conversation) = (
        turn.caller.clone(),
        turn.workspace.clone(),
        turn.conversation.clone(),
    );
    core.activate(&caller);
    let goal = messages(core, &caller.user, &workspace, &conversation)
        .iter()
        .rev()
        .find(|m| m.role == proto::Role::User)
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let run = format!("run_{}", crate::dag::now_ms());
    core.run = Some(run.clone());

    // A fresh ledger for this run. Artifacts carry over: what an earlier turn
    // produced is still addressable, which is what makes "now put those on the
    // board" work across turns.
    let carried = std::mem::take(&mut core.task.artifacts);
    core.task = proto::Task {
        id: run,
        goal,
        artifacts: carried,
        ..Default::default()
    };
    core.emit_task();
    ask_the_model(core, id);
}

/// A model step: the prompt is built here, as the turn's person, and the
/// model is read beside Core's queue.
fn ask_the_model(core: &mut Core, id: u64) {
    let Some(i) = index(core, id) else {
        return;
    };
    let turn = &core.turns[i];
    if turn.stop.asked() {
        end(core, id, TurnState::Stopped);
        return;
    }
    if core.workspace != turn.workspace {
        core.notice(
            proto::NoticeLevel::Info,
            "The answer stopped when you moved to another workspace.",
        );
        end(core, id, TurnState::Stopped);
        return;
    }
    if turn.steps >= MAX_AGENT_STEPS {
        core.notice(
            proto::NoticeLevel::Warn,
            format!("stopped after {MAX_AGENT_STEPS} tool calls in one turn"),
        );
        end(core, id, TurnState::Done);
        return;
    }
    let (user, workspace, conversation, continuing, stop) = (
        turn.caller.user.clone(),
        turn.workspace.clone(),
        turn.conversation.clone(),
        turn.continuing && turn.steps == 0,
        turn.stop.clone(),
    );

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
    let mut messages = messages(core, &user, &workspace, &conversation);
    // Carrying on an answer that stopped: its words are handed to the model
    // as the start of its reply, which it continues, and the step offers no
    // tool. Asked in words to go on from where it stopped, a model a laptop
    // runs began its answer again (Qwen2.5 3B, 2026-09-23). An answer that
    // stopped before its first word is simply answered.
    let begun = if continuing {
        messages
            .pop_if(|m| m.role == proto::Role::Assistant && m.stopped)
            .map(|m| m.content)
            .filter(|words| !words.is_empty())
    } else {
        None
    };
    let p = prompt::build(
        &core.cfg.profile,
        &active,
        &blocks,
        Some(&core.task),
        &messages,
    );
    if p.total_tokens() > core.cfg.profile.prompt_tokens_per_step {
        core.trace(format!(
            "prompt is {} tokens, over the {}-token step budget for this profile",
            p.total_tokens(),
            core.cfg.profile.prompt_tokens_per_step
        ));
    }
    let (tools, grammar) = if begun.is_some() {
        (Vec::new(), None)
    } else {
        let grammar = core.grammars.compile(&active.tools);
        core.trace(format!(
            "active set {} tools, grammar {}",
            active.tools.len(),
            grammar.hash
        ));
        (active.tools.clone(), Some(grammar.gbnf))
    };
    let request = model::ChatRequest {
        prompt: p.render(),
        tools,
        grammar,
        begun,
        max_tokens: 1024,
        temperature: 0.2,
        class: model::RequestClass::Interactive,
    };
    let prompt_estimate = p.total_tokens();
    // Taken out of the router, so that nothing holds it while the answer is
    // written: a model changed meanwhile would otherwise wait for the answer.
    let streamer = core.router.read().unwrap().for_a_turn();
    let silence = core.cfg.silence;
    let deliver = delivery(core, &user);

    match core.inbox.clone().filter(|_| !core.run_inline) {
        Some(inbox) => {
            let reading = std::thread::Builder::new()
                .name("answer".into())
                .spawn(move || {
                    let step = read_the_model(
                        streamer.as_ref(),
                        &request,
                        &stop,
                        silence,
                        &deliver,
                        &conversation,
                        prompt_estimate,
                    );
                    inbox(Internal::Replied { turn: id, step });
                });
            if let Err(e) = reading {
                let step = StepEnd {
                    reply: Err(anyhow::anyhow!("the model could not be read: {e}")),
                    shown: String::new(),
                    first_piece: None,
                    took: Duration::ZERO,
                    prompt_estimate,
                };
                post(core, Internal::Replied { turn: id, step });
            }
        }
        None => {
            let step = read_the_model(
                streamer.as_ref(),
                &request,
                &stop,
                silence,
                &deliver,
                &conversation,
                prompt_estimate,
            );
            post(core, Internal::Replied { turn: id, step });
        }
    }
}

/// Reads one model step, handing its words to the person as they come,
/// unless the model is writing a tool call in the grammar's shape, which is
/// not for reading and is handled whole.
fn read_the_model(
    streamer: Option<&model::Streamer>,
    request: &model::ChatRequest,
    stop: &Stop,
    silence: Silence,
    deliver: &Delivery,
    conversation: &str,
    prompt_estimate: usize,
) -> StepEnd {
    let asked = Instant::now();
    let mut first_piece: Option<Duration> = None;
    let mut seen = String::new();
    let mut shown = String::new();
    let mut calling: Option<bool> = None;
    let reply = match streamer {
        None => Err(anyhow::anyhow!("no model is loaded in this environment")),
        Some(streamer) => streamer.chat_streaming_until(
            request,
            &mut |delta: &str| {
                first_piece.get_or_insert_with(|| asked.elapsed());
                seen.push_str(delta);
                if calling.is_none()
                    && let Some(first) = seen.trim_start().chars().next()
                {
                    calling = Some(first == '{');
                }
                if calling == Some(false) {
                    shown.push_str(delta);
                    deliver(proto::Event::AssistantDelta {
                        conversation: conversation.to_string(),
                        text: delta.to_string(),
                    });
                }
            },
            stop,
            silence,
        ),
    };
    StepEnd {
        reply,
        shown,
        first_piece,
        took: asked.elapsed(),
        prompt_estimate,
    }
}

/// A model step ended.
fn replied(core: &mut Core, id: u64, step: StepEnd) {
    // Ended meanwhile: its chat was deleted.
    let Some(i) = index(core, id) else {
        return;
    };
    let caller = core.turns[i].caller.clone();
    core.activate(&caller);
    let (steps, conversation) = (core.turns[i].steps, core.turns[i].conversation.clone());

    // For the log, the measure of an answer and never a word of it: how
    // long until the first piece came, how many tokens at what rate, and
    // which tools were asked for. It is what a tester's "it felt slow" is
    // checked against (docs/DECISIONS.md, 2026-09-19, after day 4). An
    // answer cut short says why, and how long its prompt was, so that the
    // silences allowed can be set from what they meet.
    match &step.reply {
        Ok(r) => {
            let tools: Vec<&str> = r.calls.iter().map(|c| c.tool.as_str()).collect();
            tracing::info!(
                "answer: step {steps}, {}, prompt of {} tokens, tools asked for: {}",
                measure_of_an_answer(
                    step.first_piece.map(|d| d.as_secs_f32()),
                    step.took.as_secs_f32(),
                    r.completion_tokens
                ),
                r.prompt_tokens,
                if tools.is_empty() {
                    "none".to_string()
                } else {
                    tools.join(", ")
                }
            );
        }
        Err(e) => {
            if let Some(cut) = e.downcast_ref::<Cut>() {
                core.trace(format!(
                    "answer: step {steps}, cut: {cut}, after {:.1} s and {} words shown, prompt of about {} tokens",
                    step.took.as_secs_f32(),
                    step.shown.split_whitespace().count(),
                    step.prompt_estimate
                ));
            }
        }
    }

    let reply = match step.reply {
        Ok(reply) => reply,
        Err(e) => {
            match e.downcast_ref::<Cut>() {
                // What came is kept, marked as stopped.
                Some(cut) => {
                    keep(core, id, &step.shown, true, true);
                    let how = if *cut == Cut::Stopped {
                        TurnState::Stopped
                    } else {
                        TurnState::Cut
                    };
                    end(core, id, how);
                }
                None => {
                    let said = format!("{e:#}");
                    core.notice(proto::NoticeLevel::Error, said.clone());
                    keep(
                        core,
                        id,
                        &format!("I could not reach a model: {said}"),
                        false,
                        false,
                    );
                    end(core, id, TurnState::Done);
                }
            }
            return;
        }
    };

    if reply.calls.is_empty() {
        // Streamed already, unless it was held back as a possible tool call.
        if !reply.text.is_empty() && step.shown.is_empty() {
            core.emit(proto::Event::AssistantDelta {
                conversation,
                text: reply.text.clone(),
            });
        }
        keep(core, id, &reply.text, false, true);
        end(core, id, TurnState::Done);
        return;
    }
    if let Some(i) = index(core, id) {
        core.turns[i].calls = reply.calls.into();
    }
    post(core, Internal::GoOn { turn: id });
}

/// The turn's next tool call, one at a time through Core's queue; or, with
/// none left, its next model step.
fn go_on(core: &mut Core, id: u64) {
    let Some(i) = index(core, id) else {
        return;
    };
    let caller = core.turns[i].caller.clone();
    core.activate(&caller);
    // The safe point: the call before this one finished, the next not begun.
    if core.turns[i].stop.asked() {
        end(core, id, TurnState::Stopped);
        return;
    }
    if core.workspace != core.turns[i].workspace {
        core.notice(
            proto::NoticeLevel::Info,
            "The answer stopped when you moved to another workspace.",
        );
        end(core, id, TurnState::Stopped);
        return;
    }
    let Some(call) = core.turns[i].calls.pop_front() else {
        core.turns[i].steps += 1;
        ask_the_model(core, id);
        return;
    };
    let conversation = core.turns[i].conversation.clone();

    core.emit(proto::Event::ToolCallStarted {
        conversation: conversation.clone(),
        id: call.id.clone(),
        tool: call.tool.clone(),
        params: Json(call.params.clone()),
    });
    {
        let router = core.router.read().unwrap();
        router.metrics.tool_calls.fetch_add(1, Ordering::Relaxed);
    }
    core.turn_at_work = Some(id);
    let outcome = core.call_tool(&call.tool, &call.params, proto::Author::Agent);
    core.turn_at_work = None;
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
        conversation,
        id: call.id.clone(),
        tool: call.tool.clone(),
        outcome: outcome.clone(),
    });

    let awaiting = matches!(outcome, proto::ToolOutcome::AwaitingConfirm { .. });
    record_call(core, id, &call.tool, &call.params, outcome);
    if awaiting {
        // The person has to answer before anything else happens.
        set_state(core, id, TurnState::AwaitsApproval);
        return;
    }
    post(core, Internal::GoOn { turn: id });
}

/// The turn ends. An approval it waited on is withdrawn, and the call that
/// approval would have made is not made; its run is closed; its chat is
/// told; and whoever waited on it may start.
fn end(core: &mut Core, id: u64, how: TurnState) {
    let Some(i) = index(core, id) else {
        return;
    };
    let turn = core.turns.remove(i);
    core.activate(&turn.caller);
    let withdrawn: Vec<String> = core
        .pending
        .iter()
        .filter(|(_, pending)| pending.turn == Some(id))
        .map(|(key, _)| key.clone())
        .collect();
    for key in &withdrawn {
        core.pending.remove(key);
        core.emit(proto::Event::ApprovalWithdrawn { id: key.clone() });
    }
    if !withdrawn.is_empty() {
        core.notice(
            proto::NoticeLevel::Info,
            "Stopped. The change that waited for your approval was not made.",
        );
    }
    if turn.begun {
        finish_run(core);
    }
    core.emit(proto::Event::AssistantDone {
        conversation: turn.conversation.clone(),
    });
    core.emit(proto::Event::TurnChanged {
        conversation: turn.conversation,
        state: how,
    });
    schedule(core);
}

fn set_state(core: &mut Core, id: u64, state: TurnState) {
    let Some(i) = index(core, id) else {
        return;
    };
    if core.turns[i].state == state {
        return;
    }
    core.turns[i].state = state;
    let conversation = core.turns[i].conversation.clone();
    core.emit(proto::Event::TurnChanged {
        conversation,
        state,
    });
}

/// The answer's words, into its chat. `stopped`: it ended before the model
/// finished. Carrying on an answer that stopped, they join it, when `join`.
fn keep(core: &mut Core, id: u64, text: &str, stopped: bool, join: bool) {
    let Some(i) = index(core, id) else {
        return;
    };
    let turn = &core.turns[i];
    let (user, workspace, conversation) = (
        turn.caller.user.clone(),
        turn.workspace.clone(),
        turn.conversation.clone(),
    );
    let join = join && turn.continuing;
    write(core, &user, &workspace, &conversation, |messages| {
        if join
            && let Some(last) = messages
                .iter_mut()
                .rev()
                .find(|m| m.role == proto::Role::Assistant)
            && last.stopped
        {
            last.content.push_str(text);
            last.stopped = stopped;
            return;
        }
        let mut answer = message(proto::Role::Assistant, text);
        answer.stopped = stopped;
        messages.push(answer);
    });
}

fn record_call(core: &mut Core, id: u64, tool: &str, params: &J, outcome: proto::ToolOutcome) {
    let Some(i) = index(core, id) else {
        return;
    };
    let turn = &core.turns[i];
    let (user, workspace, conversation) = (
        turn.caller.user.clone(),
        turn.workspace.clone(),
        turn.conversation.clone(),
    );
    write(core, &user, &workspace, &conversation, |messages| {
        let record = proto::ToolCallRecord {
            id: format!("t{}", messages.len()),
            tool: tool.to_string(),
            params: Json(params.clone()),
            outcome,
        };
        messages.push(proto::ChatMessage {
            role: proto::Role::Tool,
            content: String::new(),
            tool_calls: vec![record],
            stopped: false,
        });
    });
}

/// The messages of a chat as they stand, wherever its person is now: the
/// chat on screen is Core's transcript; another of the same workspace is in
/// the working set; one of a workspace they left is in the database.
fn messages(
    core: &Core,
    user: &str,
    workspace: &str,
    conversation: &str,
) -> Vec<proto::ChatMessage> {
    if core.active.user == user && core.workspace == workspace {
        if core.conversations.current == conversation {
            return core.transcript.clone();
        }
        if let Some(found) = core.conversations.messages_of(conversation) {
            return found.to_vec();
        }
    }
    crate::conversations::messages_elsewhere(&core.store, user, workspace, conversation)
        .unwrap_or_default()
}

/// A change to a chat, written through, wherever its person is now.
fn write(
    core: &mut Core,
    user: &str,
    workspace: &str,
    conversation: &str,
    change: impl FnOnce(&mut Vec<proto::ChatMessage>),
) {
    let now = crate::dag::now_ms();
    if core.active.user == user && core.workspace == workspace {
        if core.conversations.current == conversation {
            change(&mut core.transcript);
            core.record_conversation();
            return;
        }
        if core.conversations.messages_of(conversation).is_some() {
            core.conversations.update(conversation, now, change);
            return;
        }
    }
    crate::conversations::update_elsewhere(&core.store, user, workspace, conversation, now, change);
}

/// Where the events of an answer go from the thread that reads the model:
/// to its person.
fn delivery(core: &Core, user: &str) -> Delivery {
    let events = core.events.clone();
    let user = user.to_string();
    Arc::new(move |event| {
        if let Some(sink) = &events {
            sink(To::User(user.clone()), event);
        }
    })
}

fn transcript(core: &Core) -> proto::Response {
    proto::Response::Transcript {
        messages: core.transcript.clone(),
    }
}

fn refusal(text: impl Into<String>) -> proto::Response {
    proto::Response::Error {
        message: text.into(),
    }
}

fn message(role: proto::Role, text: &str) -> proto::ChatMessage {
    proto::ChatMessage {
        role,
        content: text.to_string(),
        tool_calls: Vec::new(),
        stopped: false,
    }
}

/// "first piece after 1.2 s, 180 tokens in 14.9 s (13.1 tokens a second while
/// writing)". The rate is taken over the time in which pieces came, which is
/// what a person watches; a reply that came whole, or too fast to tell the
/// two apart, is measured over all of its time and says so.
fn measure_of_an_answer(first_piece: Option<f32>, took: f32, tokens: u32) -> String {
    match first_piece {
        Some(first) if took - first >= 0.5 => format!(
            "first piece after {first:.1} s, {tokens} tokens in {took:.1} s ({:.1} tokens a second while writing)",
            tokens as f32 / (took - first)
        ),
        _ => format!(
            "{tokens} tokens in {took:.1} s ({:.1} tokens a second over all of it)",
            tokens as f32 / took.max(0.05)
        ),
    }
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

    #[test]
    fn the_measure_of_an_answer_never_divides_by_a_moment() {
        assert_eq!(
            measure_of_an_answer(Some(1.2), 14.9, 180),
            "first piece after 1.2 s, 180 tokens in 14.9 s (13.1 tokens a second while writing)"
        );
        // A tool call comes whole: there is no "while writing" to speak of.
        assert_eq!(
            measure_of_an_answer(None, 0.3, 25),
            "25 tokens in 0.3 s (83.3 tokens a second over all of it)"
        );
        // Pieces, and too fast to tell the first from the last.
        assert_eq!(
            measure_of_an_answer(Some(0.04), 0.2, 13),
            "13 tokens in 0.2 s (65.0 tokens a second over all of it)"
        );
        assert!(measure_of_an_answer(None, 0.0, 0).contains("0 tokens"));
    }

    /// A worker that replays a fixed script of replies, so the loop itself is
    /// what is under test rather than a model.
    struct Script {
        replies: Mutex<Vec<ChatReply>>,
        seen: Mutex<Vec<ChatRequest>>,
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
            self.seen.lock().unwrap().push(req.clone());
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
        assert_eq!(
            router.metrics.malformed_tool_calls.load(Ordering::Relaxed),
            1
        );
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

        let calls = core.transcript.iter().flat_map(|m| &m.tool_calls).count();
        assert!(calls <= MAX_AGENT_STEPS, "ran {calls} tool calls");
    }

    #[test]
    fn a_model_that_cannot_be_reached_is_reported_not_hidden() {
        let mut core = Core::ephemeral("anna").unwrap(); // no worker at all
        turn(&mut core, "hello");
        let last = core.transcript.last().unwrap();
        assert_eq!(last.role, proto::Role::Assistant);
        assert!(
            last.content.contains("could not reach a model"),
            "{}",
            last.content
        );
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

        let requests = seen.seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let prompt = &requests[0].prompt;
        assert!(prompt.contains("[tools]"));
        assert!(prompt.contains("find_capability"));
        assert!(prompt.contains("[conversation]"));
        assert!(prompt.contains("hello"));
    }

    /// A Core whose transport is this test: what its turns post back is
    /// collected, and the test hands each message in when it chooses, so
    /// that a Stop can be put exactly between two steps.
    fn with_a_hand_on_the_queue(core: &mut Core) -> Arc<Mutex<Vec<Internal>>> {
        let collected: Arc<Mutex<Vec<Internal>>> = Arc::default();
        let into = collected.clone();
        core.set_inbox(Box::new(move |message| into.lock().unwrap().push(message)));
        collected
    }

    fn next(collected: &Mutex<Vec<Internal>>) -> Internal {
        let began = std::time::Instant::now();
        loop {
            if let Some(message) = {
                let mut all = collected.lock().unwrap();
                (!all.is_empty()).then(|| all.remove(0))
            } {
                return message;
            }
            assert!(
                began.elapsed() < std::time::Duration::from_secs(10),
                "nothing came back"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn events_of(core: &mut Core) -> Arc<Mutex<Vec<proto::Event>>> {
        let events: Arc<Mutex<Vec<proto::Event>>> = Arc::default();
        let into = events.clone();
        core.set_event_sink(Box::new(move |_to, event| into.lock().unwrap().push(event)));
        events
    }

    fn two_notes() -> ChatReply {
        ChatReply {
            text: String::new(),
            calls: vec![
                ProposedCall {
                    id: "c0".into(),
                    tool: "task.note".into(),
                    params: serde_json::json!({"text": "the first"}),
                },
                ProposedCall {
                    id: "c1".into(),
                    tool: "task.note".into(),
                    params: serde_json::json!({"text": "the second"}),
                },
            ],
            prompt_tokens: 100,
            completion_tokens: 20,
        }
    }

    fn states(events: &Mutex<Vec<proto::Event>>) -> Vec<TurnState> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                proto::Event::TurnChanged { state, .. } => Some(*state),
                _ => None,
            })
            .collect()
    }

    /// Stop while a tool call is at work: that call finishes, and the turn
    /// stops before the next one, never in the middle of a change.
    #[test]
    fn a_stop_between_two_tool_calls_lets_the_first_finish_and_begins_no_other() {
        let mut core = core_with(Script::new(vec![two_notes()]));
        let collected = with_a_hand_on_the_queue(&mut core);
        let events = events_of(&mut core);

        assert!(matches!(
            core.handle(proto::Request::SendMessage {
                text: "note two things".into(),
                conversation: None,
            }),
            proto::Response::Transcript { .. }
        ));
        // The model's reply: two calls, queued one at a time.
        let replied = next(&collected);
        core.internal(replied);
        let first = next(&collected);
        core.internal(first);
        assert_eq!(core.task.notes, ["the first"]);

        // The person stops it now: the second call is never begun.
        assert!(matches!(
            core.handle(proto::Request::CancelTurn { conversation: None }),
            proto::Response::Ok
        ));
        let second = next(&collected);
        core.internal(second);
        assert_eq!(core.task.notes, ["the first"], "no second note");
        let recorded = core.transcript.iter().flat_map(|m| &m.tool_calls).count();
        assert_eq!(recorded, 1, "{:#?}", core.transcript);
        assert_eq!(states(&events).last(), Some(&TurnState::Stopped));
        assert!(matches!(
            core.handle(proto::Request::ListTurns),
            proto::Response::Turns { list } if list.is_empty()
        ));
    }

    /// One answer at a time for a person: a second message in the same chat
    /// is refused while its answer is written, and one in another chat waits.
    #[test]
    fn a_chat_being_answered_takes_no_second_message_and_another_chat_waits() {
        let mut core = core_with(Script::new(vec![
            ChatReply {
                text: "first".into(),
                ..Default::default()
            },
            ChatReply {
                text: "second".into(),
                ..Default::default()
            },
        ]));
        let collected = with_a_hand_on_the_queue(&mut core);
        let events = events_of(&mut core);
        let first_chat = core.conversations.current.clone();
        core.handle(proto::Request::SendMessage {
            text: "one".into(),
            conversation: None,
        });
        match core.handle(proto::Request::SendMessage {
            text: "again".into(),
            conversation: None,
        }) {
            proto::Response::Error { message } => {
                assert!(message.contains("still being written"), "{message}");
            }
            other => panic!("a second message in the same chat: {other:?}"),
        }

        core.handle(proto::Request::NewConversation);
        let second_chat = core.conversations.current.clone();
        core.handle(proto::Request::SendMessage {
            text: "two".into(),
            conversation: None,
        });
        assert_eq!(
            states(&events),
            [TurnState::Writing, TurnState::WaitsForAnotherChat]
        );

        // The first answer comes back and ends; the waiting one starts by
        // itself, and its answer lands in its own chat.
        let replied = next(&collected);
        core.internal(replied);
        let replied = next(&collected);
        core.internal(replied);
        assert_eq!(
            states(&events),
            [
                TurnState::Writing,
                TurnState::WaitsForAnotherChat,
                TurnState::Done,
                TurnState::Writing,
                TurnState::Done
            ]
        );
        let first = core
            .conversations
            .messages_of(&first_chat)
            .unwrap()
            .to_vec();
        assert_eq!(first.last().unwrap().content, "first");
        assert_eq!(core.conversations.current, second_chat);
        assert_eq!(core.transcript.last().unwrap().content, "second");
    }

    /// A stopped answer stays in the chat, and the model reads it on the
    /// next turn, so that "go on from there" can be answered.
    #[test]
    fn a_stopped_answer_is_part_of_what_the_model_reads_next() {
        let script = Script::new(vec![ChatReply {
            text: "And then the dragon woke.".into(),
            ..Default::default()
        }]);
        let seen = script.clone();
        let mut core = core_with(script);
        let mut stopped = message(proto::Role::Assistant, "Once upon a time");
        stopped.stopped = true;
        core.transcript = vec![message(proto::Role::User, "Tell me a long story."), stopped];
        core.record_conversation();

        turn(&mut core, "go on from there");
        let prompt = seen.seen.lock().unwrap()[0].prompt.clone();
        assert!(prompt.contains("Once upon a time"), "{prompt}");
        assert!(prompt.contains(prompt::STOPPED_HERE), "{prompt}");
        assert!(core.transcript[1].stopped, "it stays marked");
    }

    /// Continue carries on in the same answer: the model is handed its words
    /// as the start of its reply, with no tool offered; what it adds joins
    /// them, and the answer is no longer marked as stopped.
    #[test]
    fn continuing_a_stopped_answer_joins_its_words_to_it() {
        let script = Script::new(vec![ChatReply {
            text: " they lived happily.".into(),
            ..Default::default()
        }]);
        let seen = script.clone();
        let mut core = core_with(script);
        let mut stopped = message(proto::Role::Assistant, "Once upon a time");
        stopped.stopped = true;
        core.transcript = vec![message(proto::Role::User, "Tell me a story."), stopped];
        core.record_conversation();

        assert!(matches!(
            core.handle(proto::Request::ContinueAnswer { conversation: None }),
            proto::Response::Transcript { .. }
        ));
        assert_eq!(core.transcript.len(), 2, "{:#?}", core.transcript);
        let answer = &core.transcript[1];
        assert_eq!(answer.content, "Once upon a time they lived happily.");
        assert!(!answer.stopped);
        let request = seen.seen.lock().unwrap()[0].clone();
        assert_eq!(request.begun.as_deref(), Some("Once upon a time"));
        assert!(request.tools.is_empty() && request.grammar.is_none());
        assert!(
            !request.prompt.contains(prompt::STOPPED_HERE),
            "the stopped words are the reply's start, not part of the chat read: {}",
            request.prompt
        );

        // With nothing stopped, there is nothing to carry on.
        match core.handle(proto::Request::ContinueAnswer { conversation: None }) {
            proto::Response::Error { message } => {
                assert!(message.contains("no stopped answer"), "{message}")
            }
            other => panic!("{other:?}"),
        }
    }
}
