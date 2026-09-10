//! A surface that holds a replica (architecture v2.1 §6.1, §6.3): it loads
//! Core's snapshot, changes its own copy, and sends sync messages; what
//! arrives at Core is the user's edit, committed in the history like any
//! other write, and Core's answer keeps the replica current. An undo in
//! Core reaches the replica the same way and stays undone. Each replica has
//! its own sync state in Core, so two frames on one board, in two windows
//! or two panels, stay in step with each other.

use automerge::sync::SyncDoc;
use automerge::transaction::Transactable;
use automerge::{AutoCommit, ReadDoc};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const BOARD: &str = "io.localspace.whiteboard";

fn harnesses() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harnesses");
    dir.join("whiteboard/logic.wasm").exists().then_some(dir)
}

/// Core with the whiteboard and one sticky on the board, and the shell's end
/// of the bridge played by the test: Core's sync messages sorted into one
/// mailbox per replica, as the shell relays them to one frame each.
struct Bench {
    core: Core,
    events: Arc<Mutex<Vec<proto::Event>>>,
    doc: String,
    mail: HashMap<String, Vec<Vec<u8>>>,
    changed: usize,
}

/// One frame's replica: its copy of the document and its end of the protocol.
struct Frame {
    peer: String,
    doc: AutoCommit,
    state: automerge::sync::State,
}

impl Bench {
    fn new() -> Option<Bench> {
        let harnesses = harnesses()?;
        let mut cfg = Config::personal("tester");
        cfg.harness_dir = Some(harnesses);
        let mut core = Core::new(cfg).expect("creating Core");
        let events: Arc<Mutex<Vec<proto::Event>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        core.set_event_sink(Box::new(move |e| sink.lock().unwrap().push(e)));
        // A commit so the document has content and a history.
        core.handle(proto::Request::CallTool {
            tool: "canvas.add_sticky".into(),
            params: proto::Json(serde_json::json!({"text": "first"})),
        });
        let doc = match core.handle(proto::Request::OpenDoc { harness: BOARD.into() }) {
            proto::Response::DocOpened { doc, .. } => doc,
            other => panic!("open: {other:?}"),
        };
        let mut bench = Bench {
            core,
            events,
            doc,
            mail: HashMap::new(),
            changed: 0,
        };
        bench.pump();
        bench.changed = 0;
        Some(bench)
    }

    /// A frame opening the board: Core's snapshot, a fresh sync state, and
    /// the first exchange.
    fn open(&mut self, peer: &str) -> Frame {
        let snapshot = match self.core.handle(proto::Request::OpenDoc { harness: BOARD.into() }) {
            proto::Response::DocOpened { snapshot, .. } => snapshot,
            other => panic!("open: {other:?}"),
        };
        let mut frame = Frame {
            peer: peer.to_string(),
            doc: AutoCommit::load(&snapshot).expect("loading the snapshot"),
            state: automerge::sync::State::new(),
        };
        self.settle(&mut frame);
        frame
    }

    /// Core's events since the last look: each sync message into its
    /// replica's mailbox, each change counted.
    fn pump(&mut self) {
        let events: Vec<proto::Event> = self.events.lock().unwrap().drain(..).collect();
        for event in events {
            match event {
                proto::Event::DocPatch { doc, peer, message } if doc == self.doc => {
                    self.mail.entry(peer).or_default().push(message);
                }
                proto::Event::DocChanged { doc } if doc == self.doc => self.changed += 1,
                _ => {}
            }
        }
    }

    /// Exchange sync messages between Core and one frame until both are quiet.
    fn settle(&mut self, frame: &mut Frame) {
        for _ in 0..8 {
            let mut moved = false;
            if let Some(message) = frame.doc.sync().generate_sync_message(&mut frame.state) {
                moved = true;
                match self.core.handle(proto::Request::DocSync {
                    doc: self.doc.clone(),
                    peer: frame.peer.clone(),
                    message: message.encode(),
                }) {
                    proto::Response::Ok => {}
                    other => panic!("sync: {other:?}"),
                }
            }
            self.pump();
            for message in self.mail.remove(&frame.peer).unwrap_or_default() {
                moved = true;
                let msg = automerge::sync::Message::decode(&message).expect("a sync message");
                frame
                    .doc
                    .sync()
                    .receive_sync_message(&mut frame.state, msg)
                    .expect("applying Core's message");
            }
            if !moved {
                break;
            }
        }
    }

    fn history(&mut self) -> Vec<proto::Commit> {
        match self.core.handle(proto::Request::GetHistory { limit: 1000 }) {
            proto::Response::History { commits } => commits,
            other => panic!("expected the history, got {other:?}"),
        }
    }

    fn json(&mut self) -> serde_json::Value {
        match self.core.handle(proto::Request::GetDocJson { harness: BOARD.into() }) {
            proto::Response::DocJson { json, .. } => json.0,
            other => panic!("{other:?}"),
        }
    }

    fn call(&mut self, tool: &str, params: serde_json::Value) {
        self.core.handle(proto::Request::CallTool {
            tool: tool.into(),
            params: proto::Json(params),
        });
    }
}

fn title_of(frame: &Frame) -> Option<String> {
    frame
        .doc
        .get(automerge::ROOT, "title")
        .expect("reading the title")
        .map(|(v, _)| v.to_string())
}

#[test]
fn a_replica_s_edit_lands_as_the_user_s_commit_and_core_s_edit_reaches_the_replica() {
    let Some(mut bench) = Bench::new() else { return };
    let mut frame = bench.open("frame-1");
    let before = bench.history().len();

    // The user edits on the board: a title, in the replica.
    frame.doc.put(automerge::ROOT, "title", "Launch plan").expect("a change");
    bench.settle(&mut frame);

    let commits = bench.history();
    assert_eq!(commits.len(), before + 1, "one commit for the replica's edit");
    assert_eq!(commits[0].tool, "surface:sync");
    assert_eq!(commits[0].harness, BOARD);
    assert_eq!(commits[0].author, proto::Author::User);
    assert_eq!(bench.json()["title"], "Launch plan");

    // A sync message with nothing new in it is not a commit.
    bench.settle(&mut frame);
    assert_eq!(bench.history().len(), before + 1);

    // The agent's edit in Core reaches the replica the same way.
    bench.call("canvas.set_title", serde_json::json!({"title": "Shipping plan"}));
    bench.settle(&mut frame);
    assert_eq!(title_of(&frame).as_deref(), Some("\"Shipping plan\""));
}

#[test]
fn an_undo_in_core_reaches_the_replica_as_a_change_and_stays_undone() {
    let Some(mut bench) = Bench::new() else { return };
    let mut frame = bench.open("frame-1");

    // The user's edit on the board, committed.
    frame.doc.put(automerge::ROOT, "title", "Launch plan").expect("a change");
    bench.settle(&mut frame);
    let committed = bench.history();
    assert_eq!(committed[0].tool, "surface:sync");

    // Ctrl+Z in the frame: the environment's undo. The replica holds the
    // undone change; it must receive the revert, not send the change back.
    assert!(matches!(bench.core.handle(proto::Request::Undo), proto::Response::Ok));
    bench.settle(&mut frame);
    let json = bench.json();
    // The logic names a new board "Board"; the undo goes back to that.
    assert_eq!(json["title"], "Board", "Core's document is back to its title from before the edit: {json}");
    assert_eq!(title_of(&frame).as_deref(), Some("\"Board\""), "the replica followed the undo");
    assert_eq!(json["shapes"].as_array().map(Vec::len), Some(1), "the sticky from before the edit stays");
    assert_eq!(bench.history().len(), committed.len(), "an undo moves the head; nothing was committed again");
    bench.settle(&mut frame);
    assert_eq!(bench.history().len(), committed.len(), "and the replica stays quiet");

    // Redo brings the edit back to both.
    assert!(matches!(bench.core.handle(proto::Request::Redo), proto::Response::Ok));
    bench.settle(&mut frame);
    assert_eq!(bench.json()["title"], "Launch plan");
    assert_eq!(title_of(&frame).as_deref(), Some("\"Launch plan\""));
    assert_eq!(bench.history().len(), committed.len());
}

#[test]
fn two_frames_on_one_board_keep_their_own_sync_states_and_stay_in_step() {
    let Some(mut bench) = Bench::new() else { return };
    let mut a = bench.open("window-a");
    let mut b = bench.open("window-b");
    let before = bench.history().len();

    // An edit in one window reaches Core, and Core writes it to the other
    // window under that window's own state.
    a.doc.put(automerge::ROOT, "title", "From a").expect("a change");
    bench.settle(&mut a);
    assert_eq!(bench.json()["title"], "From a");
    assert!(
        bench.mail.get("window-b").is_some_and(|m| !m.is_empty()),
        "Core wrote to the other window as well"
    );
    bench.settle(&mut b);
    assert_eq!(title_of(&b).as_deref(), Some("\"From a\""));

    // And back the other way.
    b.doc.put(automerge::ROOT, "title", "From b").expect("a change");
    bench.settle(&mut b);
    bench.settle(&mut a);
    assert_eq!(bench.json()["title"], "From b");
    assert_eq!(title_of(&a).as_deref(), Some("\"From b\""));

    // Two edits, two commits: catching up is never an edit.
    let commits = bench.history();
    assert_eq!(commits.len(), before + 2);
    assert!(
        commits[..2]
            .iter()
            .all(|c| c.tool == "surface:sync" && c.author == proto::Author::User),
        "{commits:?}"
    );
}

#[test]
fn a_frame_that_closed_is_sent_nothing_more_and_the_others_still_are() {
    let Some(mut bench) = Bench::new() else { return };
    let _a = bench.open("window-a");
    let _b = bench.open("window-b");
    match bench.core.handle(proto::Request::DocSyncEnd {
        doc: bench.doc.clone(),
        peer: "window-a".into(),
    }) {
        proto::Response::Ok => {}
        other => panic!("end: {other:?}"),
    }
    bench.mail.clear();
    bench.call("canvas.set_title", serde_json::json!({"title": "After a closed"}));
    bench.pump();
    assert!(!bench.mail.contains_key("window-a"), "no message for a closed frame");
    assert!(
        bench.mail.get("window-b").is_some_and(|m| !m.is_empty()),
        "the open one is still kept current"
    );
}

#[test]
fn a_reader_of_the_json_is_told_of_each_change_with_no_replica_open() {
    let Some(mut bench) = Bench::new() else { return };
    bench.call("canvas.set_title", serde_json::json!({"title": "Read me"}));
    bench.pump();
    assert_eq!(bench.changed, 1, "one DocChanged for one change");
    assert!(bench.mail.is_empty(), "no replica, no sync message");
}
