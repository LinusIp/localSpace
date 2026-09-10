//! A surface that holds a replica (architecture v2.1 §6.1, §6.3): it loads
//! Core's snapshot, changes its own copy, and sends sync messages; what
//! arrives at Core is the user's edit, committed in the history like any
//! other write, and Core's answer keeps the replica current.

use automerge::sync::SyncDoc;
use automerge::transaction::Transactable;
use automerge::{AutoCommit, ReadDoc};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type Events = Arc<Mutex<Vec<proto::Event>>>;

fn drain(events: &Events) -> Vec<proto::Event> {
    events.lock().unwrap().drain(..).collect()
}

fn harnesses() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harnesses");
    dir.join("whiteboard/logic.wasm").exists().then_some(dir)
}

fn history(core: &mut Core) -> Vec<proto::Commit> {
    match core.handle(proto::Request::GetHistory { limit: 1000 }) {
        proto::Response::History { commits } => commits,
        other => panic!("expected the history, got {other:?}"),
    }
}

/// Exchange sync messages until both sides are quiet.
fn settle(core: &mut Core, events: &Events, doc_id: &str, replica: &mut AutoCommit, state: &mut automerge::sync::State) {
    for _ in 0..8 {
        let mut moved = false;
        if let Some(message) = replica.sync().generate_sync_message(state) {
            moved = true;
            match core.handle(proto::Request::DocSync {
                doc: doc_id.to_string(),
                message: message.encode(),
            }) {
                proto::Response::Ok => {}
                other => panic!("sync: {other:?}"),
            }
        }
        // Core answers with a patch event; the shell relays it as a sync
        // message. Here the test plays the shell.
        for event in drain(events) {
            if let proto::Event::DocPatch { doc, message } = event {
                if doc == doc_id {
                    moved = true;
                    let msg = automerge::sync::Message::decode(&message).expect("a sync message");
                    replica.sync().receive_sync_message(state, msg).expect("applying Core's message");
                }
            }
        }
        if !moved {
            break;
        }
    }
}

#[test]
fn a_replica_s_edit_lands_as_the_user_s_commit_and_core_s_edit_reaches_the_replica() {
    let Some(harnesses) = harnesses() else { return };
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(harnesses);
    let mut core = Core::new(cfg).expect("creating Core");
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    core.set_event_sink(Box::new(move |e| sink.lock().unwrap().push(e)));
    // A commit so the document has content and a history.
    core.handle(proto::Request::CallTool {
        tool: "canvas.add_sticky".into(),
        params: proto::Json(serde_json::json!({"text": "first"})),
    });
    drain(&events);

    let (doc_id, snapshot) = match core.handle(proto::Request::OpenDoc {
        harness: "io.localspace.whiteboard".into(),
    }) {
        proto::Response::DocOpened { doc, snapshot, .. } => (doc, snapshot),
        other => panic!("open: {other:?}"),
    };
    let mut replica = AutoCommit::load(&snapshot).expect("loading the snapshot");
    let mut state = automerge::sync::State::new();
    settle(&mut core, &events, &doc_id, &mut replica, &mut state);
    let before = history(&mut core).len();

    // The user edits on the board: a title, in the replica.
    replica.put(automerge::ROOT, "title", "Launch plan").expect("a change");
    settle(&mut core, &events, &doc_id, &mut replica, &mut state);

    let commits = history(&mut core);
    assert_eq!(commits.len(), before + 1, "one commit for the replica's edit");
    assert_eq!(commits[0].tool, "surface:sync");
    assert_eq!(commits[0].harness, "io.localspace.whiteboard");
    assert_eq!(commits[0].author, proto::Author::User);
    let json = match core.handle(proto::Request::GetDocJson {
        harness: "io.localspace.whiteboard".into(),
    }) {
        proto::Response::DocJson { json, .. } => json.0,
        other => panic!("{other:?}"),
    };
    assert_eq!(json["title"], "Launch plan");

    // A sync message with nothing new in it is not a commit.
    settle(&mut core, &events, &doc_id, &mut replica, &mut state);
    assert_eq!(history(&mut core).len(), before + 1);

    // The agent's edit in Core reaches the replica the same way.
    core.handle(proto::Request::CallTool {
        tool: "canvas.set_title".into(),
        params: proto::Json(serde_json::json!({"title": "Shipping plan"})),
    });
    settle(&mut core, &events, &doc_id, &mut replica, &mut state);
    let title = replica.get(automerge::ROOT, "title").expect("reading").map(|(v, _)| v.to_string());
    assert_eq!(title.as_deref(), Some("\"Shipping plan\""));
}
