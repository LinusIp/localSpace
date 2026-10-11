//! Packages from a catalog (architecture v2 §1 principle 3, §13 step 5;
//! marketplace spec §1): only chat ships in the box; what the user installs
//! is copied under the data directory, comes back after a restart, and goes
//! when uninstalled.

use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::path::PathBuf;

fn harnesses() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harnesses");
    dir.join("whiteboard/logic.wasm").exists().then_some(dir)
}

/// The repository's `registry/`, which holds the types package the whiteboard
/// depends on (plugin spec §18.3).
fn registry() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry");
    dir.join("types/types.toml").exists().then_some(dir)
}

fn environment(core: &mut Core) -> proto::EnvironmentState {
    match core.handle(proto::Request::GetEnvironment) {
        proto::Response::Environment(state) => state,
        other => panic!("expected the environment, got {other:?}"),
    }
}

#[test]
fn an_install_from_the_catalog_is_copied_and_survives_a_restart() {
    let Some(harnesses) = harnesses() else { return };
    let Some(registry) = registry() else { return };
    let data = tempfile::tempdir().unwrap();
    let copy = data.path().join("installed/io.localspace.whiteboard");
    let types_copy = data.path().join("installed/io.localspace.types");

    {
        let mut cfg = Config::personal("tester");
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.catalog_dirs = vec![harnesses.clone(), registry.clone()];
        let mut core = Core::new(cfg).expect("creating Core");
        assert!(
            environment(&mut core).harnesses.is_empty(),
            "only chat ships in the box"
        );

        let entries = match core.handle(proto::Request::ListCatalog) {
            proto::Response::Catalog { entries } => entries,
            other => panic!("expected the catalog, got {other:?}"),
        };
        let entry = entries
            .iter()
            .find(|e| e.id == "io.localspace.whiteboard")
            .expect("the whiteboard is offered");
        assert!(!entry.installed);

        match core.handle(proto::Request::InstallHarness {
            path: entry.path.clone(),
        }) {
            proto::Response::Ok => {}
            other => panic!("install: {other:?}"),
        }
        assert!(copy.join("harness.toml").exists(), "the package was copied");
        assert!(copy.join("logic.wasm").exists());
        assert!(copy.join("tools.json").exists());
        // The types package the whiteboard depends on came with it, copied
        // the same way, so the install stands on its own after a restart.
        assert!(
            types_copy.join("types.toml").exists(),
            "the dependency was copied"
        );
        let ids: Vec<String> = environment(&mut core)
            .harnesses
            .iter()
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                "io.localspace.types".to_string(),
                "io.localspace.whiteboard".to_string()
            ]
        );
    }

    {
        // A new Core on the same data directory, with no catalog and no
        // pre-installed directory: the install is still there.
        let mut cfg = Config::personal("tester");
        cfg.data_dir = Some(data.path().to_path_buf());
        let mut core = Core::new(cfg).expect("creating Core again");
        let ids: Vec<String> = environment(&mut core)
            .harnesses
            .iter()
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                "io.localspace.types".to_string(),
                "io.localspace.whiteboard".to_string()
            ],
            "the install came back, with its dependency"
        );

        match core.handle(proto::Request::UninstallHarness {
            harness: "io.localspace.whiteboard".into(),
        }) {
            proto::Response::Ok => {}
            other => panic!("uninstall: {other:?}"),
        }
        let ids: Vec<String> = environment(&mut core)
            .harnesses
            .iter()
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec!["io.localspace.types".to_string()],
            "the dependency stays until it is uninstalled itself"
        );
        assert!(!copy.exists(), "uninstall removes the environment's copy");
        assert!(types_copy.exists());
    }
    // The catalog's own package was never touched.
    assert!(harnesses.join("whiteboard/harness.toml").exists());
}

fn doc_json(core: &mut Core) -> serde_json::Value {
    match core.handle(proto::Request::GetDocJson {
        harness: "io.localspace.whiteboard".into(),
    }) {
        proto::Response::DocJson { json, .. } => json.0,
        other => panic!("expected the document, got {other:?}"),
    }
}

fn history_len(core: &mut Core) -> usize {
    match core.handle(proto::Request::GetHistory { limit: 1000 }) {
        proto::Response::History { commits } => commits.len(),
        other => panic!("expected the history, got {other:?}"),
    }
}

/// Architecture v2 §6.3: a surface's write without a commit moves the
/// document for every client but leaves the history alone; an edit is a
/// commit named after the view, by the user; the same document again is
/// no change.
#[test]
fn a_write_without_a_commit_moves_the_document_but_not_the_history() {
    let Some(harnesses) = harnesses() else { return };
    let Some(registry) = registry() else { return };
    let data = tempfile::tempdir().unwrap();
    let mut cfg = Config::personal("tester");
    cfg.data_dir = Some(data.path().to_path_buf());
    cfg.harness_dir = Some(harnesses);
    // The whiteboard depends on the types package, resolved from the catalog.
    cfg.catalog_dirs = vec![registry];
    let mut core = Core::new(cfg).expect("creating Core");

    let mut doc = doc_json(&mut core);
    // A fresh document is whatever the logic last wrote, which may be nothing yet.
    for key in ["shapes", "frames", "selection"] {
        if !doc[key].is_array() {
            doc[key] = serde_json::json!([]);
        }
    }
    let before = history_len(&mut core);

    doc["selection"] = serde_json::json!(["nothing-yet"]);
    match core.handle(proto::Request::WriteDoc {
        harness: "io.localspace.whiteboard".into(),
        view: "web".into(),
        doc: proto::Json(doc.clone()),
        commit: false,
    }) {
        proto::Response::Ok => {}
        other => panic!("write: {other:?}"),
    }
    assert_eq!(
        doc_json(&mut core)["selection"],
        serde_json::json!(["nothing-yet"])
    );
    assert_eq!(history_len(&mut core), before, "no commit for a selection");

    doc["shapes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "s_test", "kind": "sticky", "x": 10.0, "y": 10.0, "w": 130.0, "h": 110.0,
            "fill": "yellow", "text": "from the surface", "frame": null, "z": 1, "locked": false
        }));
    match core.handle(proto::Request::WriteDoc {
        harness: "io.localspace.whiteboard".into(),
        view: "web".into(),
        doc: proto::Json(doc.clone()),
        commit: true,
    }) {
        proto::Response::Ok => {}
        other => panic!("write: {other:?}"),
    }
    assert_eq!(history_len(&mut core), before + 1);
    let commits = match core.handle(proto::Request::GetHistory { limit: 1 }) {
        proto::Response::History { commits } => commits,
        other => panic!("{other:?}"),
    };
    assert_eq!(commits[0].tool, "surface:web");
    assert_eq!(commits[0].author, proto::Author::User);

    core.handle(proto::Request::WriteDoc {
        harness: "io.localspace.whiteboard".into(),
        view: "web".into(),
        doc: proto::Json(doc),
        commit: true,
    });
    assert_eq!(
        history_len(&mut core),
        before + 1,
        "the same document again is no change"
    );
}

/// A whiteboard installed by an older version, in a release that sets it
/// aside (docs/DECISIONS.md, 2026-10-10): its files stay where they are and
/// nothing of it is shown or offered. It is not loaded, the catalog does not
/// offer it, it cannot be installed again, its board is not listed among the
/// documents, and the model is told about no tool and no board.
#[test]
fn a_set_aside_harness_left_installed_is_kept_on_disk_and_shown_nowhere() {
    let Some(harnesses) = harnesses() else { return };
    let Some(registry) = registry() else { return };
    let data = tempfile::tempdir().unwrap();
    let copy = data.path().join("installed/io.localspace.whiteboard");
    let config = |set_aside: bool| {
        let mut cfg = Config::personal("tester");
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.catalog_dirs = vec![harnesses.clone(), registry.clone()];
        if set_aside {
            cfg.set_aside = localspace_core::SET_ASIDE
                .iter()
                .map(|id| id.to_string())
                .collect();
        }
        cfg
    };
    let documents = |core: &mut Core| match core.handle(proto::Request::ListDocuments) {
        proto::Response::Documents { documents } => documents,
        other => panic!("expected the documents, got {other:?}"),
    };

    // An older version: the whiteboard installed, its board made.
    {
        let mut core = Core::new(config(false)).expect("creating Core");
        let path = harnesses.join("whiteboard").display().to_string();
        match core.handle(proto::Request::InstallHarness { path }) {
            proto::Response::Ok => {}
            other => panic!("install: {other:?}"),
        }
        assert!(
            !documents(&mut core).is_empty(),
            "the board is a document of the workspace"
        );
    }
    assert!(copy.join("harness.toml").exists());

    // This version: the same data, the whiteboard set aside.
    let mut core = Core::new(config(true)).expect("creating Core again");
    assert!(
        copy.join("harness.toml").exists() && copy.join("logic.wasm").exists(),
        "its files stay where they were"
    );
    let env = environment(&mut core);
    assert!(
        env.harnesses.iter().all(|h| h.tool_count == 0),
        "no tool is loaded: {:?}",
        env.harnesses.iter().map(|h| &h.id).collect::<Vec<_>>()
    );
    match core.handle(proto::Request::ListCatalog) {
        proto::Response::Catalog { entries } => assert!(
            entries
                .iter()
                .all(|e| !localspace_core::SET_ASIDE.contains(&e.id.as_str())),
            "the catalog offers neither: {:?}",
            entries.iter().map(|e| &e.id).collect::<Vec<_>>()
        ),
        other => panic!("expected the catalog, got {other:?}"),
    }
    let path = harnesses.join("whiteboard").display().to_string();
    match core.handle(proto::Request::InstallHarness { path }) {
        proto::Response::Error { message } => {
            assert!(message.contains("not part of this version"), "{message}")
        }
        other => panic!("a set-aside harness was installed: {other:?}"),
    }
    assert!(
        documents(&mut core).is_empty(),
        "its board is kept and not listed"
    );
    match core.handle(proto::Request::PreviewContext { budget: 0 }) {
        proto::Response::Context { prompt_preview, .. } => {
            assert!(!prompt_preview.contains("[tools]"), "{prompt_preview}");
            assert!(!prompt_preview.contains("[state]"), "{prompt_preview}");
            assert!(!prompt_preview.contains("canvas"), "{prompt_preview}");
        }
        other => panic!("expected the context, got {other:?}"),
    }
}
