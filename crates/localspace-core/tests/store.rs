//! One database under the data directory (deployment §3.4): a schema
//! version, a forward-only migration after a copy, and documents with ids
//! of their own — a workspace may hold several per harness while the UI
//! shows the oldest (Pilot 1, Phase A, answers 6 and 7).

use localspace_core::store::{FILE, SCHEMA_VERSION, Store, V1_FILE};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use serde_json::json;
use std::path::{Path, PathBuf};

const WHITEBOARD: &str = "io.localspace.whiteboard";

fn repo_dir(name: &str, marker: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    dir.join(marker).exists().then_some(dir)
}

/// A personal Core on `data`, loading the whiteboard from the repository
/// with the registry as its catalog.
fn config(data: &Path) -> Option<Config> {
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    cfg.data_dir = Some(data.to_path_buf());
    Some(cfg)
}

fn board_doc(core: &mut Core) -> String {
    match core.handle(proto::Request::GetDocJson {
        harness: WHITEBOARD.into(),
    }) {
        proto::Response::DocJson { doc, .. } => doc,
        other => panic!("GetDocJson failed: {other:?}"),
    }
}

fn documents(core: &mut Core) -> Vec<proto::DocumentInfo> {
    match core.handle(proto::Request::ListDocuments) {
        proto::Response::Documents { documents } => documents,
        other => panic!("ListDocuments failed: {other:?}"),
    }
}

fn add_sticky(core: &mut Core) {
    match core.handle(proto::Request::CallTool {
        tool: "canvas.add_sticky".into(),
        params: proto::Json(json!({"text": "kept", "fill": "yellow"})),
    }) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { .. }) => {}
        other => panic!("add_sticky failed: {other:?}"),
    }
}

#[test]
fn a_fresh_install_gives_the_whiteboard_a_document_of_its_own_and_keeps_it() {
    let data = tempfile::tempdir().unwrap();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    let first = {
        let mut core = Core::new(cfg).unwrap();
        let doc = board_doc(&mut core);
        assert!(
            doc.starts_with("doc_"),
            "an id of its own, not the harness's name: {doc}"
        );
        add_sticky(&mut core);
        doc
    };
    let db = data.path().join("db");
    assert!(db.join(FILE).exists(), "the database is {FILE}");
    assert!(
        !db.join(V1_FILE).exists(),
        "a fresh install never had the v1 file"
    );

    let mut core = Core::new(config(data.path()).unwrap()).unwrap();
    assert_eq!(
        board_doc(&mut core),
        first,
        "the same document after a restart"
    );
    let docs = documents(&mut core);
    let board = docs
        .iter()
        .find(|d| d.id == first)
        .expect("the board is listed");
    assert_eq!(board.title, "Whiteboard");
    assert_eq!(board.kind, proto::DocKind::Crdt);
    assert!(board.head.is_some(), "the sticky's commit is its head");
    assert!(
        board.bytes.is_none(),
        "a harness's document has no size to give"
    );
    assert!(
        matches!(&board.source, proto::DocumentSource::Harness { harness } if harness == WHITEBOARD)
    );
    drop(core);

    let store = Store::open(data.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    let record = store.get_document(&first).unwrap().expect("recorded");
    assert_eq!(record.harness.as_deref(), Some(WHITEBOARD));
    assert_eq!(record.workspace, "ws_tester");
    assert_eq!(record.created_by, "tester");
}

#[test]
fn a_database_from_before_the_version_is_copied_and_migrated_with_its_history() {
    let data = tempfile::tempdir().unwrap();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    let legacy = "io_localspace_whiteboard";
    let db = data.path().join("db");
    // A v1 file: the DAG alone, a board's history under the harness's name,
    // and an export of 6.0 recorded without a workspace.
    {
        let v1 = Store::open_file(&db.join(V1_FILE)).unwrap();
        let dag = localspace_core::dag::Dag::with(v1.db()).unwrap();
        let empty_board = automerge::AutoCommit::new().save();
        dag.commit(
            legacy,
            WHITEBOARD,
            "canvas.add_sticky",
            proto::Json(json!({})),
            &empty_board,
            "added 1 field(s)",
            proto::Author::User,
            None,
        )
        .unwrap();
        let old_export: localspace_core::store::DocumentRecord = serde_json::from_value(json!({
            "title": "board-abc1234.png", "kind": "blob", "mime": "image/png", "bytes": 3, "hash": "h",
            "source": {"export": {"harness": WHITEBOARD, "document": legacy, "commit": "cabc1234"}},
            "created_ms": 5
        }))
        .unwrap();
        v1.put_document("blob:h", &old_export).unwrap();
        assert_eq!(v1.schema_version().unwrap(), 0);
    }

    let mut core = Core::new(cfg).unwrap();
    assert_eq!(
        board_doc(&mut core),
        legacy,
        "the board keeps the history it has under its old name"
    );
    assert!(
        db.join(V1_FILE).exists(),
        "the copy taken before migrating stays"
    );
    assert!(
        db.join(FILE).exists(),
        "the migrated database is the new file"
    );
    let docs = documents(&mut core);
    let board = docs.iter().find(|d| d.id == legacy).expect("listed");
    assert_eq!(board.title, "Whiteboard");
    assert!(board.head.is_some());
    assert!(
        matches!(&board.source, proto::DocumentSource::Harness { harness } if harness == WHITEBOARD)
    );
    assert!(
        docs.iter().any(|d| d.id == "blob:h"),
        "the old export is still listed"
    );
    drop(core);

    let store = Store::open(data.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    let record = store
        .get_document(legacy)
        .unwrap()
        .expect("a record for the old board");
    assert_eq!(record.harness.as_deref(), Some(WHITEBOARD));
    assert_eq!(record.workspace, "ws_tester");
    let export = store.get_document("blob:h").unwrap().unwrap();
    assert_eq!(
        export.workspace, "ws_tester",
        "the export got its workspace"
    );
    assert_eq!(export.created_by, "tester");
    assert!(
        export.harness.is_none(),
        "an export is a file, not a harness's document"
    );
}

#[test]
fn the_oldest_document_is_the_one_shown_when_a_workspace_holds_several() {
    let data = tempfile::tempdir().unwrap();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    let first = {
        let mut core = Core::new(cfg).unwrap();
        board_doc(&mut core)
    };
    {
        // A second, newer board for the same harness in the same workspace.
        let store = Store::open(data.path()).unwrap();
        let mut second = store.get_document(&first).unwrap().unwrap();
        second.created_ms += 1;
        second.title = "Second board".into();
        store.put_document("doc_second", &second).unwrap();
    }
    let mut core = Core::new(config(data.path()).unwrap()).unwrap();
    assert_eq!(board_doc(&mut core), first, "the oldest is shown");
    let docs = documents(&mut core);
    assert!(
        docs.iter()
            .any(|d| d.id == first && d.title == "Whiteboard")
    );
    assert!(
        docs.iter()
            .any(|d| d.id == "doc_second" && d.title == "Second board"),
        "the second is held, under its own name"
    );
}

fn conversations(core: &mut Core) -> (Vec<proto::ConversationSummary>, String) {
    match core.handle(proto::Request::ListConversations) {
        proto::Response::Conversations { list, current } => (list, current),
        other => panic!("ListConversations failed: {other:?}"),
    }
}

#[test]
fn the_legacy_conversations_file_is_imported_once_and_the_ledger_survives_a_restart() {
    let data = tempfile::tempdir().unwrap();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    // A v2 database, as 2026-09-12's first store commit wrote, with the
    // conversations still in the file beside it.
    {
        let core = Core::new(cfg).unwrap();
        drop(core);
        let store = Store::open(data.path()).unwrap();
        store.set_schema_version(2).unwrap();
    }
    let legacy = data.path().join("conversations.json");
    std::fs::write(
        &legacy,
        json!({
            "version": 1,
            "current": "c_old_2",
            "conversations": [
                {"id": "c_old_1", "title": "Risks", "created_ms": 1, "updated_ms": 2,
                 "messages": [{"role": "user", "content": "Put three risks on the board", "tool_calls": []}]},
                {"id": "c_old_2", "title": "Launch", "created_ms": 3, "updated_ms": 4,
                 "messages": [{"role": "user", "content": "Plan the launch", "tool_calls": []}]}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let mut core = Core::new(config(data.path()).unwrap()).unwrap();
    let (list, current) = conversations(&mut core);
    assert_eq!(
        current, "c_old_2",
        "the file's current conversation is current"
    );
    let titles: Vec<&str> = list.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Launch", "Risks"],
        "both came in, and no placeholder beside them"
    );
    match core.handle(proto::Request::GetTranscript) {
        proto::Response::Transcript { messages } => {
            assert_eq!(messages[0].content, "Plan the launch")
        }
        other => panic!("{other:?}"),
    }
    assert!(!legacy.exists(), "the file was renamed");
    assert!(data.path().join("conversations.json.imported").exists());

    // The ledger: an artifact registered now is there after a restart.
    add_sticky(&mut core);
    match core.handle(proto::Request::CallTool {
        tool: "canvas.export_outline".into(),
        params: proto::Json(json!({})),
    }) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { .. }) => {}
        other => panic!("export_outline failed: {other:?}"),
    }
    drop(core);
    let mut core = Core::new(config(data.path()).unwrap()).unwrap();
    match core.handle(proto::Request::GetTask) {
        proto::Response::Task(task) => {
            assert_eq!(task.artifacts.len(), 1, "the ledger survived the restart");
            assert_eq!(task.artifacts[0].kind, "outline.v1");
        }
        other => panic!("{other:?}"),
    }
    let (list, current) = conversations(&mut core);
    assert_eq!(current, "c_old_2");
    assert_eq!(list.len(), 2, "imported once, not again");
    drop(core);
    assert_eq!(
        Store::open(data.path()).unwrap().schema_version().unwrap(),
        SCHEMA_VERSION
    );
}
