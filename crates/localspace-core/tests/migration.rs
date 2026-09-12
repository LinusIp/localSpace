//! The v1 → v3 migration against a database the previous binary wrote
//! (`tests/fixtures/v1/README.md`): the one irreversible step of Phase A.
//! It renames the file, so the tests prove that the board, its history,
//! its exports and the conversations all come through, and that the v1
//! file is copied before the first write and never written to.

use localspace_core::store::{FILE, SCHEMA_VERSION, Store, V1_FILE};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::path::{Path, PathBuf};

const WHITEBOARD: &str = "io.localspace.whiteboard";
/// The board's document under its v1 name: the harness id with dots as
/// underscores.
const LEGACY_BOARD: &str = "io_localspace_whiteboard";
const PNG_DOC: &str = "blob:460c4d8bad9fad8b54b6d7f725ecd9bb3671bfc4f8317717369de5a4a8c3059e";
const SVG_DOC: &str = "blob:af505c845345d6bf90d78ca4a87db39e75aa9c0a7ad2dc54fc9942d2a832b529";
/// The exact bytes the previous binary was given to export.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#fdf6d8"/></svg>"##;

fn repo_dir(name: &str, marker: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    dir.join(marker).exists().then_some(dir)
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}

/// A copy of the fixture as a data directory, so the migration runs on
/// the previous binary's bytes and the checked-in file stays as it is.
fn data_from_fixture() -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(data.path().join("db")).unwrap();
    std::fs::copy(
        fixture().join("db").join(V1_FILE),
        data.path().join("db").join(V1_FILE),
    )
    .unwrap();
    std::fs::copy(
        fixture().join("conversations.json"),
        data.path().join("conversations.json"),
    )
    .unwrap();
    data
}

fn config(data: &Path) -> Option<Config> {
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    cfg.data_dir = Some(data.to_path_buf());
    Some(cfg)
}

/// A file's content hash. The file must not be held open by a Core: on
/// Windows the database is locked while it is.
fn hash_of(path: &Path) -> String {
    blake3::hash(&std::fs::read(path).unwrap())
        .to_hex()
        .to_string()
}

fn board(core: &mut Core) -> (String, serde_json::Value) {
    match core.handle(proto::Request::GetDocJson {
        harness: WHITEBOARD.into(),
    }) {
        proto::Response::DocJson { doc, json, .. } => (doc, json.0),
        other => panic!("GetDocJson failed: {other:?}"),
    }
}

fn shapes(core: &mut Core) -> usize {
    board(core).1["shapes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0)
}

fn history(core: &mut Core) -> Vec<proto::Commit> {
    match core.handle(proto::Request::GetHistory { limit: 100 }) {
        proto::Response::History { commits } => commits,
        other => panic!("GetHistory failed: {other:?}"),
    }
}

fn documents(core: &mut Core) -> Vec<proto::DocumentInfo> {
    match core.handle(proto::Request::ListDocuments) {
        proto::Response::Documents { documents } => documents,
        other => panic!("ListDocuments failed: {other:?}"),
    }
}

fn blob(core: &mut Core, doc: &str) -> (String, String, Vec<u8>) {
    match core.handle(proto::Request::GetDocBlob { doc: doc.into() }) {
        proto::Response::DocBlob { name, mime, bytes } => (name, mime, bytes),
        other => panic!("GetDocBlob failed: {other:?}"),
    }
}

fn add_sticky(core: &mut Core, text: &str) {
    match core.handle(proto::Request::CallTool {
        tool: "canvas.add_sticky".into(),
        params: proto::Json(serde_json::json!({"text": text, "fill": "grey"})),
    }) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { .. }) => {}
        other => panic!("add_sticky failed: {other:?}"),
    }
}

fn decode_base64(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| *c != b'=') {
        let value = ALPHABET.iter().position(|a| *a == c).expect("base64") as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

#[test]
fn a_real_v1_database_migrates_with_its_board_its_history_and_its_exports_intact() {
    let data = data_from_fixture();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    let mut core = Core::new(cfg).unwrap();

    // The board: the same document, under its v1 name, with what was on it.
    let (doc, json) = board(&mut core);
    assert_eq!(
        doc, LEGACY_BOARD,
        "the board keeps the name its history is under"
    );
    let texts: Vec<&str> = json["shapes"]
        .as_array()
        .expect("shapes")
        .iter()
        .filter_map(|s| s["text"].as_str())
        .collect();
    assert_eq!(texts, vec!["Design", "Build", "Launch"]);

    // Its history: three commits on the board, two exports, and the lock —
    // the fixture's two commits and the one every start writes.
    let commits = history(&mut core);
    let on_board: Vec<&proto::Commit> = commits.iter().filter(|c| c.doc == LEGACY_BOARD).collect();
    assert_eq!(on_board.len(), 3, "{commits:#?}");
    assert!(on_board.iter().all(|c| c.tool == "canvas.add_sticky"));
    assert_eq!(
        commits
            .iter()
            .filter(|c| c.tool == "surface:export")
            .count(),
        2
    );
    assert!(
        commits
            .iter()
            .filter(|c| c.doc != LEGACY_BOARD && c.tool != "surface:export")
            .all(|c| c.doc == "environment_lock"),
        "{commits:#?}"
    );

    // The history is live, not just listed. Undo acts on the document last
    // touched, and a start touches the lock; one new sticky makes the board
    // that document, then two undos walk back through the new commit and
    // into the migrated history, and the redos come forward again.
    add_sticky(&mut core, "after the migration");
    assert_eq!(shapes(&mut core), 4);
    for expected in [3, 2] {
        assert!(matches!(
            core.handle(proto::Request::Undo),
            proto::Response::Ok
        ));
        assert_eq!(
            shapes(&mut core),
            expected,
            "undo walked the migrated history"
        );
    }
    for expected in [3, 4] {
        assert!(matches!(
            core.handle(proto::Request::Redo),
            proto::Response::Ok
        ));
        assert_eq!(shapes(&mut core), expected);
    }
    assert!(matches!(
        core.handle(proto::Request::Undo),
        proto::Response::Ok
    ));
    assert_eq!(shapes(&mut core), 3, "back at the fixture's board");

    // The exports: listed with their source, and their bytes exactly as given.
    let docs = documents(&mut core);
    for (id, name, mime, expected) in [
        (
            PNG_DOC,
            "board-fixture-c7b43d8.png",
            "image/png",
            decode_base64(PNG_BASE64),
        ),
        (
            SVG_DOC,
            "board-fixture-c7b43d8.svg",
            "image/svg+xml",
            SVG.as_bytes().to_vec(),
        ),
    ] {
        let listed = docs
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("{name} is listed"));
        assert_eq!(listed.title, name);
        assert_eq!(listed.mime, mime);
        assert_eq!(listed.bytes, Some(expected.len() as u64));
        assert_eq!(
            &listed.hash,
            id.trim_start_matches("blob:"),
            "the head's content is the export's"
        );
        assert!(
            matches!(&listed.source, proto::DocumentSource::Export { harness, document, .. }
                if harness == WHITEBOARD && document == LEGACY_BOARD),
            "{:?}",
            listed.source
        );
        let (got_name, got_mime, bytes) = blob(&mut core, id);
        assert_eq!(got_name, name);
        assert_eq!(got_mime, mime);
        assert_eq!(bytes, expected, "{name}'s bytes came through unchanged");
        assert_eq!(blake3::hash(&bytes).to_hex().to_string(), listed.hash);
    }
    let board_listed = docs
        .iter()
        .find(|d| d.id == LEGACY_BOARD)
        .expect("the board is listed");
    assert_eq!(board_listed.title, "Whiteboard");
    assert!(
        matches!(&board_listed.source, proto::DocumentSource::Harness { harness } if harness == WHITEBOARD)
    );

    // The conversation and its two messages, with the current one kept.
    match core.handle(proto::Request::ListConversations) {
        proto::Response::Conversations { list, current } => {
            assert_eq!(list.len(), 1);
            assert_eq!(current, "c_1789223120830_1");
            assert_eq!(list[0].messages, 2);
        }
        other => panic!("{other:?}"),
    }
    match core.handle(proto::Request::GetTranscript) {
        proto::Response::Transcript { messages } => {
            assert_eq!(messages[0].content, "What is on the board?");
            assert_eq!(messages.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    assert!(data.path().join("conversations.json.imported").exists());
    drop(core);

    // The files: the v1 copy beside the migrated database, stamped with
    // this code's schema.
    let db = data.path().join("db");
    assert!(db.join(V1_FILE).exists());
    assert!(db.join(FILE).exists());
    {
        let store = Store::open(data.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let record = store
            .get_document(LEGACY_BOARD)
            .unwrap()
            .expect("the board got its record");
        assert_eq!(record.harness.as_deref(), Some(WHITEBOARD));
        assert_eq!(record.workspace, "ws_tester");
        for id in [PNG_DOC, SVG_DOC] {
            let export = store.get_document(id).unwrap().unwrap();
            assert_eq!(
                export.workspace, "ws_tester",
                "the export got its workspace"
            );
            assert!(export.harness.is_none());
        }
    }

    // And a second start finds everything where the first left it.
    let mut core = Core::new(config(data.path()).unwrap()).unwrap();
    assert_eq!(board(&mut core).0, LEGACY_BOARD);
    assert_eq!(shapes(&mut core), 3);
    assert_eq!(documents(&mut core).len(), 3);
}

#[test]
fn the_v1_file_is_copied_before_the_first_write_and_never_written_to() {
    let data = data_from_fixture();
    let Some(cfg) = config(data.path()) else {
        return;
    };
    let db = data.path().join("db");
    let v1 = db.join(V1_FILE);
    let checked_in = hash_of(&fixture().join("db").join(V1_FILE));
    assert_eq!(
        hash_of(&v1),
        checked_in,
        "the copy under test is the checked-in file"
    );

    // Read-only: opening it for writing would fail, so only a copy can pass.
    let mut permissions = std::fs::metadata(&v1).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&v1, permissions).unwrap();

    // Opening the store is the first thing that touches the directory: the
    // copy exists before any table is written, and the v1 file is as it was.
    {
        let store = Store::open(data.path()).unwrap();
        assert!(db.join(FILE).exists(), "the copy was taken");
        assert_eq!(store.schema_version().unwrap(), 0, "nothing migrated yet");
    }
    assert_eq!(
        hash_of(&v1),
        checked_in,
        "opening wrote nothing to the v1 file"
    );
    let copied = hash_of(&db.join(FILE));

    // The migration, then a further write: the v1 file never changes; the
    // migrated database is what was written.
    let mut core = Core::new(cfg).unwrap();
    assert_eq!(
        hash_of(&v1),
        checked_in,
        "the migration wrote nothing to the v1 file"
    );
    assert_eq!(shapes(&mut core), 3);
    add_sticky(&mut core, "after");
    drop(core);
    assert_eq!(
        hash_of(&v1),
        checked_in,
        "the migration and a write after it wrote nothing to the v1 file"
    );
    assert_ne!(
        hash_of(&db.join(FILE)),
        copied,
        "the migrated database is the file that was written"
    );

    // The temp directory can only be removed once the file is writable again.
    let mut permissions = std::fs::metadata(&v1).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&v1, permissions).unwrap();
}
