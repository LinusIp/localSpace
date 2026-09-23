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
    pub first_piece: Option<Duration>,
    pub took: Duration,
    /// The prompt's length as estimated before it was sent: the engine says
    /// the true one only at the end, which an answer cut short never reaches.
    pub prompt_estimate: usize,
}

/// A turn's work, come back through Core's queue.
pub enum Internal {
    /// A model step ended.
    Replied { turn: u64, step: StepEnd },
    /// The turn's next tool call or model step: queued, so that whatever came
    /// meanwhile, a Stop included, is handled first.
    GoOn { turn: u64 },
}
