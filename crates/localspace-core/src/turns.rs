//! An answer being written, as work beside Core's queue (1.6 and 1.7 of the
//! plan after Test A; docs/DECISIONS.md, 2026-09-23).
//!
//! Until then a turn was one request. Core does everything on one thread, one
//! request at a time, so an answer took Core up until it ended and every
//! other request waited behind it: another chat, the list of chats, a
//! download's Stop, and Stop itself, which ran after the answer and then only
//! forgot the turn's id. On a server that was every person's app. Now a
//! message starts a turn and returns at once. The model is read on a thread
//! of its own and its words go out as events tagged with their chat; what it
//! asks of tools comes back through Core's queue one call at a time, so that
//! a Stop sent meanwhile is read first and no document change is left half
//! made. At most one answer a person at a time; as many people at once as
//! the engine has slots.

use crate::Caller;
use crate::model::{ChatReply, ProposedCall};
use crate::stream::Stop;
use localspace_proto as proto;
use std::collections::VecDeque;
use std::time::Duration;

/// One answer: being written, or waiting to be.
pub struct Turn {
    pub id: u64,
    /// Whose it is: everything it does, it does as them.
    pub caller: Caller,
    /// The workspace and the chat it answers in, whichever the person has on
    /// screen meanwhile.
    pub workspace: String,
    pub conversation: String,
    pub state: proto::TurnState,
    /// It has left the queue: its run and its ledger were made.
    pub begun: bool,
    /// Tool calls it has made.
    pub steps: usize,
    pub stop: Stop,
    /// The tool calls of the model's last reply that are still to run.
    pub calls: VecDeque<ProposedCall>,
    /// It carries on an answer that stopped: its words join that answer.
    pub continuing: bool,
    /// A call the model wrote into its answer to a tool that is not on offer,
    /// refused to it in one line: the model reads it on its one more try, and
    /// it is kept nowhere (docs/DECISIONS.md, 2026-10-07).
    pub refused: Option<crate::prompt::Turn>,
}

impl Turn {
    /// Being written or waiting on its person: it holds their one answer.
    pub fn live(&self) -> bool {
        matches!(
            self.state,
            proto::TurnState::Writing | proto::TurnState::AwaitsApproval
        )
    }

    pub fn waiting(&self) -> bool {
        matches!(
            self.state,
            proto::TurnState::WaitsForAnotherChat | proto::TurnState::WaitsForTheModel
        )
    }
}

/// What a model step ended with, handed back to Core's thread.
pub struct StepEnd {
    pub reply: anyhow::Result<ChatReply>,
    /// The text shown to the person as it came. A tool call the model wrote
    /// in the grammar's shape is held back and is not part of it.
    pub shown: String,
    /// The model's thinking as it came, before and apart from the words:
    /// kept with the answer, a stopped one included, which the reply of a
    /// step cut short no longer holds (docs/DECISIONS.md, 2026-10-10).
    pub thought: String,
    /// When the thinking's first and last pieces came, from the request.
    pub thought_span: Option<(Duration, Duration)>,
    /// How many pieces of the answer's words came (one token each, as the
    /// engine streams them): what the log's two speeds are counted from.
    pub word_pieces: usize,
    /// The answer, when Core stopped it for saying the same stretch a third
    /// time in a row: its words up to the end of the stretch's first saying
    /// (docs/DECISIONS.md, 2026-10-10).
    pub repeated: Option<String>,
    pub first_piece: Option<Duration>,
    pub took: Duration,
    /// The prompt's length as estimated before it was sent: the engine says
    /// the true one only at the end, which an answer cut short never reaches.
    pub prompt_estimate: usize,
    /// Whether the step asked the model to think (`Some`), or left it to the
    /// model's own default (`None`).
    pub thinking: Option<bool>,
}

/// The measure of one model step, kept per chat for whoever hosts Core
/// in-process (`localspace measure`): the figures, never a word of the
/// answer (docs/DECISIONS.md, 2026-10-08).
#[derive(Debug, Clone, Default)]
pub struct AnswerMeasure {
    /// Whether the step asked the model to think, or left it its default.
    pub thinking_asked: Option<bool>,
    /// Until the first visible word of the answer.
    pub first_word: Option<Duration>,
    /// The whole step.
    pub took: Duration,
    /// The thinking that came before the answer: how many words, and when
    /// it began and ended, from the request.
    pub thinking_words: usize,
    pub thinking_began: Option<Duration>,
    pub thinking_ended: Option<Duration>,
    /// The engine's own figures, where it gave them.
    pub timings: Option<crate::model::Timings>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// How the step ended early, if it did: stopped, a silence, the engine
    /// gone, or an error in words.
    pub cut: Option<String>,
    /// The tools the model asked for in this step.
    pub tools: Vec<String>,
    /// The thinking ran to its budget and the engine ended it so that the
    /// answer came (`model::THINKING_BUDGET_SPENT` closes the thinking).
    pub budget_spent: bool,
    /// Core stopped the answer for repeating itself: what the kit counts
    /// as a loop (docs/DECISIONS.md, 2026-10-10).
    pub repeated: bool,
}

impl AnswerMeasure {
    pub fn of(step: &StepEnd) -> AnswerMeasure {
        let mut measure = AnswerMeasure {
            thinking_asked: step.thinking,
            first_word: step.first_piece,
            took: step.took,
            // A loop is stopped from Core: the reply is then a stop.
            repeated: step.repeated.is_some(),
            ..Default::default()
        };
        match &step.reply {
            Ok(reply) => {
                measure.thinking_words = reply.reasoning.split_whitespace().count();
                measure.thinking_began = reply.reasoning_began;
                measure.thinking_ended = reply.reasoning_ended;
                measure.budget_spent = reply
                    .reasoning
                    .trim_end()
                    .ends_with(crate::model::THINKING_BUDGET_SPENT);
                measure.timings = reply.timings;
                measure.prompt_tokens = reply.prompt_tokens;
                measure.completion_tokens = reply.completion_tokens;
                measure.tools = reply.calls.iter().map(|c| c.tool.clone()).collect();
            }
            Err(e) => {
                measure.cut = Some(match e.downcast_ref::<crate::stream::Cut>() {
                    Some(cut) => cut.to_string(),
                    None => format!("{e:#}"),
                });
            }
        }
        measure
    }
}

/// A turn's work, come back through Core's queue.
pub enum Internal {
    /// A model step ended. Boxed: the step carries the reply with its
    /// measures, and the other message is a number.
    Replied { turn: u64, step: Box<StepEnd> },
    /// The turn's next tool call or model step: queued, so that whatever came
    /// meanwhile, a Stop included, is handled first.
    GoOn { turn: u64 },
}
