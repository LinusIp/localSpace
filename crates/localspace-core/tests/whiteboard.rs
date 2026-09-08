//! End-to-end test against the real reference harness.
//!
//! This installs `harnesses/whiteboard` — the actual signed-shape package, with
//! its logic running as a wasm component under wasmtime — and drives it through
//! Core exactly as the agent does. It is the test that proves the pieces fit:
//! manifest, tool lint, Tier A runtime, JSON-into-CRDT reconciliation, DAG
//! commits, undo, the confirmation gate, and the context provider.
//!
//! Skipped with a clear message when the harness has not been built yet.

use anyhow::Result;
use localspace_core::model::{ChatReply, ChatRequest, ModelWorker, ProposedCall};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const WHITEBOARD: &str = "io.localspace.whiteboard";

fn harness_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("harnesses");
    if dir.join("whiteboard").join("logic.wasm").exists() {
        Some(dir)
    } else {
        eprintln!(
            "skipping: {} has not been built. Run `cargo xtask harnesses` or see docs/BUILD.md",
            dir.join("whiteboard").join("logic.wasm").display()
        );
        None
    }
}

fn core() -> Option<Core> {
    let dir = harness_dir()?;
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(dir);
    let core = Core::new(cfg).expect("creating Core");
    assert!(
        core.environment()
            .harnesses
            .iter()
            .any(|h| h.id == WHITEBOARD),
        "the whiteboard failed to install"
    );
    Some(core)
}

fn call(core: &mut Core, tool: &str, params: Value) -> proto::ToolOutcome {
    core.call_tool(tool, &params, proto::Author::User)
}

fn board(core: &mut Core) -> Value {
    match core.handle(proto::Request::CallTool {
        tool: "canvas.list".into(),
        params: proto::Json(json!({})),
    }) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { result, .. }) => result.0,
        other => panic!("canvas.list failed: {other:?}"),
    }
}

#[test]
fn the_package_installs_with_its_declared_shape() {
    let Some(core) = core() else { return };
    let env = core.environment();
    let h = env.harnesses.iter().find(|h| h.id == WHITEBOARD).unwrap();

    assert_eq!(h.tier, proto::Tier::Wasm);
    assert_eq!(h.doc_kind, proto::DocKind::Crdt);
    assert!(h.has_context_provider);
    assert_eq!(h.tool_count, 20, "editing tools plus the outline export");
    assert_eq!(h.front_door.len(), 2, "front doors: {:?}", h.front_door);
    assert!(h.front_door.contains(&"canvas.list".to_string()));

    // Default deny: the package asked for nothing, so it has nothing.
    assert_eq!(h.capabilities.net, "none");
    assert_eq!(h.capabilities.fs, "none");
    assert!(h.capabilities.model.is_empty());

    // Two views, one of each renderable kind.
    let kinds: Vec<proto::SurfaceKind> = h.views.iter().map(|v| v.kind).collect();
    assert!(kinds.contains(&proto::SurfaceKind::Egui));
    assert!(kinds.contains(&proto::SurfaceKind::Widgets));
}

#[test]
fn a_tool_call_mutates_the_document_and_lands_a_commit() {
    let Some(mut core) = core() else { return };

    let outcome = call(
        &mut core,
        "canvas.add_sticky",
        json!({"text": "supply chain", "fill": "red"}),
    );
    let (summary, commit) = match outcome {
        proto::ToolOutcome::Ok {
            diff_summary,
            commit,
            ..
        } => (diff_summary, commit),
        other => panic!("add_sticky failed: {other:?}"),
    };
    assert!(summary.contains("sticky"), "{summary}");
    assert!(commit.is_some(), "a write must produce a commit");

    let listed = board(&mut core);
    assert_eq!(listed["shapes"].as_array().unwrap().len(), 1);
    assert_eq!(listed["shapes"][0]["text"], "supply chain");
    assert_eq!(listed["shapes"][0]["fill"], "red");

    // The commit carries the tool call that caused it.
    match core.handle(proto::Request::GetHistory { limit: 10 }) {
        proto::Response::History { commits } => {
            assert_eq!(commits.len(), 1);
            assert_eq!(commits[0].tool, "canvas.add_sticky");
            assert_eq!(commits[0].harness, WHITEBOARD);
            assert!(!commits[0].doc_hash.is_empty());
        }
        other => panic!("history failed: {other:?}"),
    }
}

#[test]
fn undo_puts_the_board_back() {
    let Some(mut core) = core() else { return };
    call(&mut core, "canvas.add_sticky", json!({"text": "one"}));
    call(&mut core, "canvas.add_sticky", json!({"text": "two"}));
    assert_eq!(board(&mut core)["shapes"].as_array().unwrap().len(), 2);

    core.handle(proto::Request::Undo);
    assert_eq!(board(&mut core)["shapes"].as_array().unwrap().len(), 1);

    core.handle(proto::Request::Redo);
    assert_eq!(board(&mut core)["shapes"].as_array().unwrap().len(), 2);
}

#[test]
fn a_bad_parameter_is_refused_before_the_harness_sees_it() {
    let Some(mut core) = core() else { return };

    // Missing a required parameter.
    match call(&mut core, "canvas.add_sticky", json!({"fill": "red"})) {
        proto::ToolOutcome::Error { message } => assert!(message.contains("required"), "{message}"),
        other => panic!("expected a schema error, got {other:?}"),
    }
    // Outside the declared enum.
    match call(
        &mut core,
        "canvas.add_sticky",
        json!({"text": "x", "fill": "chartreuse"}),
    ) {
        proto::ToolOutcome::Error { message } => assert!(message.contains("one of"), "{message}"),
        other => panic!("expected an enum error, got {other:?}"),
    }
    // Nothing reached the document.
    assert_eq!(board(&mut core)["shapes"].as_array().unwrap().len(), 0);
}

#[test]
fn the_harness_reports_its_own_failures_without_corrupting_the_document() {
    let Some(mut core) = core() else { return };
    match call(&mut core, "canvas.move", json!({"id": "nope", "x": 1, "y": 2})) {
        proto::ToolOutcome::Error { message } => assert!(message.contains("no shape"), "{message}"),
        other => panic!("expected the harness's own error, got {other:?}"),
    }
    assert_eq!(board(&mut core)["shapes"].as_array().unwrap().len(), 0);
}

#[test]
fn a_destructive_write_stops_at_the_confirmation_gate() {
    let Some(mut core) = core() else { return };
    call(&mut core, "canvas.add_sticky", json!({"text": "keep me"}));
    let id = board(&mut core)["shapes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // As the agent, `canvas.delete` is `confirm = "destructive"`.
    let outcome = core.call_tool("canvas.delete", &json!({"id": id}), proto::Author::Agent);
    assert!(
        matches!(outcome, proto::ToolOutcome::AwaitingConfirm { .. }),
        "expected a confirmation gate, got {outcome:?}"
    );
    assert_eq!(
        board(&mut core)["shapes"].as_array().unwrap().len(),
        1,
        "nothing may be deleted before the user answers"
    );
}

#[test]
fn the_context_provider_serialises_the_board_within_its_budget() {
    let Some(mut core) = core() else { return };
    for i in 0..40 {
        call(
            &mut core,
            "canvas.add_sticky",
            json!({"text": format!("risk number {i}"), "fill": "amber"}),
        );
    }
    core.handle(proto::Request::SetFocus {
        harness: Some(WHITEBOARD.into()),
    });

    for budget in [300usize, 600, 1500] {
        match core.handle(proto::Request::PreviewContext { budget }) {
            proto::Response::Context { blocks, .. } => {
                let block = blocks
                    .iter()
                    .find(|b| b.harness == WHITEBOARD)
                    .expect("no block from the whiteboard");
                assert!(
                    block.tokens <= budget + 20,
                    "at budget {budget} the provider returned {} tokens",
                    block.tokens
                );
                assert!(block.text.contains("board"), "{}", block.text);
                assert!(block.expandable, "the provider must offer a zoom");
            }
            other => panic!("PreviewContext failed: {other:?}"),
        }
    }
}

#[test]
fn the_provider_is_not_re_run_when_the_document_has_not_changed() {
    let Some(mut core) = core() else { return };
    call(&mut core, "canvas.add_sticky", json!({"text": "one"}));
    core.handle(proto::Request::SetFocus {
        harness: Some(WHITEBOARD.into()),
    });

    let first = core.context_blocks();
    let second = core.context_blocks();
    assert_eq!(
        first[0].text, second[0].text,
        "an unchanged document must produce byte-identical blocks"
    );
    assert!(
        core.provider_cache_hit_rate() > 0.0,
        "the second turn must have hit the provider cache"
    );
}

#[test]
fn the_widgets_view_renders_from_the_document() {
    let Some(mut core) = core() else { return };
    call(&mut core, "canvas.set_title", json!({"title": "Q4 planning"}));

    match core.handle(proto::Request::GetWidgetView {
        harness: WHITEBOARD.into(),
        view: "settings".into(),
    }) {
        proto::Response::WidgetView { root } => {
            let rendered = serde_json::to_string(&root).unwrap();
            assert!(rendered.contains("Q4 planning"), "{rendered}");
        }
        other => panic!("GetWidgetView failed: {other:?}"),
    }
}

#[test]
fn the_egui_surface_module_is_shipped_and_matches_the_shape_schema() {
    let Some(mut core) = core() else { return };
    match core.handle(proto::Request::GetSurfaceModule {
        harness: WHITEBOARD.into(),
        view: "board".into(),
    }) {
        proto::Response::SurfaceModule {
            bytes,
            shape_schema,
            ..
        } => {
            assert_eq!(shape_schema, proto::SHAPE_SCHEMA);
            assert!(bytes.len() > 1024, "surface module looks empty");
            assert_eq!(&bytes[0..4], b"\0asm", "not a wasm module");
        }
        other => panic!("GetSurfaceModule failed: {other:?}"),
    }
}

#[test]
fn find_capability_reaches_the_whiteboard_when_it_is_not_focused() {
    let Some(mut core) = core() else { return };
    core.handle(proto::Request::SetFocus { harness: None });

    let out = core.call_tool(
        "find_capability",
        &json!({"need": "draw a sticky note on a board"}),
        proto::Author::Agent,
    );
    match out {
        proto::ToolOutcome::Ok { result, .. } => {
            let hits = result.0;
            assert_eq!(hits[0]["harness"], WHITEBOARD, "{hits}");
        }
        other => panic!("find_capability failed: {other:?}"),
    }
    // And it is focused for the next turn.
    assert_eq!(core.environment().focus.as_deref(), Some(WHITEBOARD));
}

// ---------------------------------------------------------------------------
// Resources (spec §1.2): declared budgets are enforced, idle logic is dropped
// ---------------------------------------------------------------------------

/// A private copy of the whiteboard package with extra manifest lines, so a
/// test can change `[resources]` without touching the real package.
fn whiteboard_with(extra_manifest: &str) -> Option<tempfile::TempDir> {
    let src = harness_dir()?.join("whiteboard");
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join("whiteboard");
    std::fs::create_dir_all(dst.join("ui")).unwrap();
    for name in ["tools.json", "evals.json", "logic.wasm"] {
        std::fs::copy(src.join(name), dst.join(name)).unwrap();
    }
    std::fs::copy(src.join("ui/board.wasm"), dst.join("ui/board.wasm")).unwrap();
    let manifest = std::fs::read_to_string(src.join("harness.toml")).unwrap();
    std::fs::write(
        dst.join("harness.toml"),
        format!("{manifest}\n{extra_manifest}\n"),
    )
    .unwrap();
    Some(dir)
}

#[test]
fn a_harness_over_its_declared_memory_budget_is_refused_by_name() {
    use localspace_core::registry::{Policy, Registry};
    use localspace_core::runtime::NoServices;

    let Some(dir) = whiteboard_with("[resources]\nmemory_mb = { logic = 1, surface = 16 }") else {
        return;
    };
    let mut staged = Registry::stage(&dir.path().join("whiteboard"), &Policy::default())
        .expect("the manifest is valid; the budget is just too small");
    let err = Registry::instantiate(&mut staged, Arc::new(NoServices))
        .expect_err("a 1 MB budget cannot hold the whiteboard's logic")
        .to_string();
    assert!(
        err.contains("memory_mb.logic = 1 MB"),
        "the refusal must name the budget, got: {err}"
    );
    assert!(staged.runtime.is_none(), "nothing may be left running");
}

#[test]
fn idle_logic_is_unloaded_and_comes_back_with_its_document_intact() {
    let Some(dir) = whiteboard_with("[resources]\nidle_unload = \"1s\"") else {
        return;
    };
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(dir.path().to_path_buf());
    let mut core = Core::new(cfg).expect("creating Core");

    let loaded = |core: &Core| -> bool {
        core.environment()
            .harnesses
            .iter()
            .find(|h| h.id == WHITEBOARD)
            .map(|h| h.loaded)
            .unwrap_or(false)
    };

    assert!(matches!(
        call(&mut core, "canvas.add_sticky", json!({"text": "survives"})),
        proto::ToolOutcome::Ok { .. }
    ));
    assert!(loaded(&core), "just used, so resident");

    std::thread::sleep(std::time::Duration::from_millis(1300));
    core.tick();
    assert!(!loaded(&core), "idle past idle_unload, so dropped");

    // The next call brings it back, and the document was never touched.
    let listed = board(&mut core);
    assert_eq!(listed["shapes"].as_array().unwrap().len(), 1);
    assert_eq!(listed["shapes"][0]["text"], "survives");
    assert!(loaded(&core), "re-instantiated on first call");
}

#[test]
fn the_default_budgets_are_the_spec_s_and_are_reported() {
    let Some(core) = core() else { return };
    let env = core.environment();
    let h = env.harnesses.iter().find(|h| h.id == WHITEBOARD).unwrap();
    assert_eq!(h.resources.logic_mb, 32);
    assert_eq!(h.resources.surface_mb, 16);
    assert_eq!(h.resources.idle_unload_secs, 300);
}

// ---------------------------------------------------------------------------
// Inter-harness handoff (spec §18): whiteboard -> outline.v1 -> planning board
// ---------------------------------------------------------------------------

/// A Core with the whiteboard installed and the planner installed from the
/// bundle, so a task can cross from one to the other.
fn core_with_both() -> Option<Core> {
    let mut core = core_with_catalog()?;
    let path = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .expect("planner missing from the catalog")
        .path;
    assert!(matches!(
        core.handle(proto::Request::InstallHarness { path }),
        proto::Response::Ok
    ));
    Some(core)
}

fn task_of(core: &mut Core) -> proto::Task {
    match core.handle(proto::Request::GetTask) {
        proto::Response::Task(t) => t,
        other => panic!("GetTask failed: {other:?}"),
    }
}

#[test]
fn the_packages_declare_what_they_produce_and_accept() {
    let Some(core) = core_with_both() else { return };
    let env = core.environment();
    let board = env.harnesses.iter().find(|h| h.id == WHITEBOARD).unwrap();
    let planner = env.harnesses.iter().find(|h| h.id == "io.localspace.planner").unwrap();
    assert_eq!(board.produces, vec!["outline.v1".to_string()]);
    assert_eq!(planner.accepts, vec!["outline.v1".to_string()]);
}

#[test]
fn exporting_registers_an_artifact_pinned_to_a_commit() {
    let Some(mut core) = core_with_both() else { return };
    stickies(&mut core, 3);

    let outcome = call(&mut core, "canvas.export_outline", json!({}));
    let (summary, commit) = match outcome {
        proto::ToolOutcome::Ok {
            diff_summary,
            commit,
            ..
        } => (diff_summary, commit),
        other => panic!("export failed: {other:?}"),
    };
    // The one line the model sees names the artifact and its type.
    assert!(summary.contains("art_1 (outline.v1)"), "{summary}");

    let task = task_of(&mut core);
    assert_eq!(task.artifacts.len(), 1);
    let art = &task.artifacts[0];
    assert_eq!(art.id, "art_1");
    assert_eq!(art.kind, "outline.v1");
    assert_eq!(art.produced_by, WHITEBOARD);
    assert_eq!(Some(art.commit.clone()), commit, "pinned to the commit the export made");
    assert!(art.summary.contains("3 item(s)"), "{}", art.summary);

    // The artifact is a reference into the DAG, not a copy: the pinned version
    // contains the outline the export wrote.
    match core.handle(proto::Request::GetHistory { limit: 5 }) {
        proto::Response::History { commits } => {
            assert_eq!(commits[0].id, art.commit);
            assert_eq!(commits[0].tool, "canvas.export_outline");
        }
        other => panic!("history failed: {other:?}"),
    }
}

#[test]
fn a_handoff_lands_the_outline_as_cards_on_the_planning_board() {
    let Some(mut core) = core_with_both() else { return };
    call(&mut core, "canvas.add_sticky", json!({"text": "Supply chain", "fill": "red"}));
    call(&mut core, "canvas.add_sticky", json!({"text": "Hiring", "fill": "red"}));
    call(&mut core, "canvas.export_outline", json!({}));

    // The board moves on after the export; the artifact must not.
    call(&mut core, "canvas.add_sticky", json!({"text": "Added later", "fill": "grey"}));

    let outcome = call(
        &mut core,
        "board.import_outline",
        json!({"artifact": "art_1", "column": "In progress"}),
    );
    match outcome {
        proto::ToolOutcome::Ok { diff_summary, .. } => {
            assert!(diff_summary.contains("imported 2 card(s)"), "{diff_summary}");
            assert!(diff_summary.contains("art_1"), "{diff_summary}");
        }
        other => panic!("import failed: {other:?}"),
    }

    // Two cards, from the pinned version — the later sticky is not among them.
    let cards = match call(&mut core, "board.zoom", json!({})) {
        proto::ToolOutcome::Ok { result, .. } => result.0["cards"].as_array().cloned().unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(cards.len(), 2, "{cards:?}");
    let texts: Vec<&str> = cards.iter().map(|c| c["text"].as_str().unwrap()).collect();
    assert!(texts.contains(&"Supply chain"));
    assert!(texts.contains(&"Hiring"));
    assert!(!texts.contains(&"Added later"), "the handoff must use the pinned version");
    assert!(cards.iter().all(|c| c["column"] == "In progress"));
    assert_eq!(cards[0]["source"]["artifact"], "art_1", "each card cites where it came from");

    // Both legs are audited.
    let events: Vec<String> = core.audit_log().records().iter().map(|r| r.event.clone()).collect();
    assert!(events.contains(&"artifact.produced".to_string()), "{events:?}");
    assert!(events.contains(&"artifact.handoff".to_string()), "{events:?}");
}

#[test]
fn a_handoff_to_a_harness_that_does_not_accept_the_type_is_refused_with_a_suggestion() {
    let Some(mut core) = core_with_both() else { return };
    stickies(&mut core, 1);
    call(&mut core, "canvas.export_outline", json!({}));

    // The whiteboard produces outline.v1 but does not accept it. Handing the
    // artifact to one of its own tools must be refused before the harness runs,
    // and the refusal must say who would take it.
    match call(&mut core, "canvas.set_title", json!({"title": "x", "artifact": "art_1"})) {
        proto::ToolOutcome::Denied { reason } => {
            assert!(reason.contains("does not accept outline.v1"), "{reason}");
            assert!(reason.contains("io.localspace.planner"), "{reason}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }

    // An artifact that does not exist is refused by name, listing what does.
    match call(&mut core, "board.import_outline", json!({"artifact": "art_9"})) {
        proto::ToolOutcome::Denied { reason } => {
            assert!(reason.contains("art_9"), "{reason}");
            assert!(reason.contains("art_1"), "{reason}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_ledger_is_in_the_prompt_whichever_harness_is_focused() {
    let Some(mut core) = core_with_both() else { return };
    stickies(&mut core, 1);
    call(&mut core, "canvas.export_outline", json!({}));
    call(
        &mut core,
        "task.plan",
        json!({"steps": [
            {"harness": WHITEBOARD, "intent": "export the risks"},
            {"harness": "io.localspace.planner", "intent": "make them cards"}
        ]}),
    );

    for focus in [WHITEBOARD, "io.localspace.planner"] {
        core.handle(proto::Request::SetFocus {
            harness: Some(focus.into()),
        });
        match core.handle(proto::Request::PreviewContext { budget: 600 }) {
            proto::Response::Context { prompt_preview, .. } => {
                assert!(prompt_preview.contains("[task "), "focused {focus}: no ledger");
                assert!(prompt_preview.contains("art_1 outline.v1 from io.localspace.whiteboard"));
                assert!(prompt_preview.contains("make them cards"));
                // The ledger sits after the stable prefix, before the conversation.
                let ledger_at = prompt_preview.find("[task ").unwrap();
                let state_at = prompt_preview.find("[state]").unwrap();
                let convo_at = prompt_preview.find("[conversation]").unwrap();
                assert!(state_at < ledger_at && ledger_at < convo_at);
            }
            other => panic!("PreviewContext failed: {other:?}"),
        }
    }
}

#[test]
fn a_plan_may_only_name_installed_harnesses() {
    let Some(mut core) = core_with_both() else { return };
    match call(
        &mut core,
        "task.plan",
        json!({"steps": [{"harness": "io.example.cad", "intent": "sketch"}]}),
    ) {
        proto::ToolOutcome::Error { message } => {
            assert!(message.contains("io.example.cad"), "{message}");
            assert!(message.contains("not installed"), "{message}");
        }
        other => panic!("expected an error, got {other:?}"),
    }
    assert!(task_of(&mut core).plan.is_empty(), "a refused plan writes nothing");
}

#[test]
fn an_artifact_of_an_undeclared_kind_is_not_registered() {
    // A harness may only register kinds it declares it produces. The planner
    // declares none, so even a well-formed `artifact` in a result is refused —
    // in the trace, not silently.
    let Some(mut core) = core_with_both() else { return };
    let planner_produces = core
        .environment()
        .harnesses
        .iter()
        .find(|h| h.id == "io.localspace.planner")
        .unwrap()
        .produces
        .clone();
    assert!(planner_produces.is_empty());
    // Nothing the planner ships returns an artifact today, so the ledger stays
    // empty after using it — the declaration, not the code, is the gate.
    call(&mut core, "board.add_card", json!({"text": "x"}));
    assert!(task_of(&mut core).artifacts.is_empty());
}

// ---------------------------------------------------------------------------
// Editing: the operations a board needs beyond "add one shape"
// ---------------------------------------------------------------------------

/// Add `n` stickies and return their ids in creation order.
fn stickies(core: &mut Core, n: usize) -> Vec<String> {
    for i in 0..n {
        call(
            core,
            "canvas.add_sticky",
            json!({"text": format!("note {i}"), "fill": "yellow"}),
        );
    }
    board(core)["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["id"].as_str().map(|s| s.to_string()))
        .collect()
}

/// The full shape records, which `canvas.list` deliberately does not return.
fn full(core: &mut Core) -> Vec<Value> {
    match call(core, "canvas.zoom", json!({})) {
        proto::ToolOutcome::Ok { result, .. } => {
            result.0["shapes"].as_array().cloned().unwrap_or_default()
        }
        other => panic!("zoom failed: {other:?}"),
    }
}

fn shape<'a>(shapes: &'a [Value], id: &str) -> &'a Value {
    shapes
        .iter()
        .find(|s| s["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("no shape {id}"))
}

#[test]
fn aligning_puts_every_shape_on_one_edge() {
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 3);
    // Spread them out first, so aligning has something to do.
    for (i, id) in ids.iter().enumerate() {
        call(
            &mut core,
            "canvas.move",
            json!({"id": id, "x": 100 + i * 37, "y": 50 + i * 60}),
        );
    }

    match call(&mut core, "canvas.align", json!({"ids": ids, "edge": "left"})) {
        proto::ToolOutcome::Ok { diff_summary, .. } => {
            assert!(diff_summary.contains("aligned 3"), "{diff_summary}");
        }
        other => panic!("align failed: {other:?}"),
    }

    let shapes = full(&mut core);
    let xs: Vec<f64> = ids.iter().map(|id| shape(&shapes, id)["x"].as_f64().unwrap()).collect();
    assert!(
        xs.windows(2).all(|w| (w[0] - w[1]).abs() < 0.001),
        "left edges did not line up: {xs:?}"
    );
    assert_eq!(xs[0], 100.0, "they align to the leftmost, not to zero");
}

#[test]
fn distributing_spaces_shapes_evenly_between_the_outermost_two() {
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 4);
    for (i, id) in ids.iter().enumerate() {
        let x = [0.0, 30.0, 40.0, 600.0][i];
        call(&mut core, "canvas.move", json!({"id": id, "x": x, "y": 0}));
    }

    call(&mut core, "canvas.distribute", json!({"ids": ids, "axis": "x"}));

    let shapes = full(&mut core);
    let mut xs: Vec<f64> = ids.iter().map(|id| shape(&shapes, id)["x"].as_f64().unwrap()).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // The two outermost stay put; the gaps between all four become equal.
    assert_eq!(xs[0], 0.0);
    assert!((xs[3] - 600.0).abs() < 1.5, "{xs:?}");
    let gaps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        gaps.windows(2).all(|g| (g[0] - g[1]).abs() < 1.5),
        "uneven gaps: {gaps:?}"
    );
}

#[test]
fn duplicate_copies_offset_and_selects_the_copies() {
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 2);

    match call(&mut core, "canvas.duplicate", json!({"ids": ids.clone()})) {
        proto::ToolOutcome::Ok { diff_summary, .. } => {
            assert!(diff_summary.contains("duplicated 2"), "{diff_summary}")
        }
        other => panic!("duplicate failed: {other:?}"),
    }

    let shapes = full(&mut core);
    assert_eq!(shapes.len(), 4);
    let original = shape(&shapes, &ids[0]).clone();
    let copy = shapes
        .iter()
        .find(|s| s["id"].as_str().unwrap().starts_with(&format!("{}_", ids[0])))
        .expect("no copy of the first sticky");
    assert_eq!(copy["text"], original["text"]);
    assert_eq!(
        copy["x"].as_f64().unwrap() - original["x"].as_f64().unwrap(),
        24.0,
        "a copy sits beside its original, not on top of it"
    );

    // The copies are what you now have hold of, which is what you want next.
    let selection = board(&mut core);
    let _ = selection;
    let doc_selection = match call(&mut core, "canvas.zoom", json!({})) {
        proto::ToolOutcome::Ok { .. } => (),
        other => panic!("{other:?}"),
    };
    let _ = doc_selection;
}

#[test]
fn ordering_changes_which_shape_is_on_top() {
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 3);

    let z_of = |core: &mut Core, id: &str| -> i64 {
        shape(&full(core), id)["z"].as_i64().unwrap_or(0)
    };
    assert!(z_of(&mut core, &ids[0]) < z_of(&mut core, &ids[2]));

    call(&mut core, "canvas.order", json!({"ids": [ids[0].clone()], "to": "front"}));
    assert!(
        z_of(&mut core, &ids[0]) > z_of(&mut core, &ids[2]),
        "front did not put it on top"
    );

    call(&mut core, "canvas.order", json!({"ids": [ids[0].clone()], "to": "back"}));
    assert!(
        z_of(&mut core, &ids[0]) < z_of(&mut core, &ids[1]),
        "back did not put it underneath"
    );
}

#[test]
fn a_locked_shape_refuses_every_edit_except_unlocking() {
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 1);
    let id = ids[0].clone();

    call(&mut core, "canvas.lock", json!({"ids": [id.clone()], "locked": true}));

    for (tool, params) in [
        ("canvas.move", json!({"id": id, "x": 500, "y": 500})),
        ("canvas.resize", json!({"id": id, "w": 400, "h": 400})),
        ("canvas.delete", json!({"id": id})),
    ] {
        match call(&mut core, tool, params) {
            proto::ToolOutcome::Error { message } => {
                assert!(message.contains("locked"), "{tool}: {message}")
            }
            other => panic!("{tool} should have been refused, got {other:?}"),
        }
    }

    // The shape is untouched, and unlocking still works.
    let shapes = full(&mut core);
    assert_eq!(shapes.len(), 1);
    assert_eq!(shape(&shapes, &id)["locked"], true);

    call(&mut core, "canvas.lock", json!({"ids": [id.clone()], "locked": false}));
    assert!(matches!(
        call(&mut core, "canvas.move", json!({"id": id, "x": 500, "y": 500})),
        proto::ToolOutcome::Ok { .. }
    ));
}

#[test]
fn a_text_label_has_no_box_and_carries_its_size() {
    let Some(mut core) = core() else { return };
    match call(
        &mut core,
        "canvas.add_text",
        json!({"text": "Q4 risks", "x": 40, "y": 12, "size": 24}),
    ) {
        proto::ToolOutcome::Ok { diff_summary, .. } => {
            assert!(diff_summary.contains("text label"), "{diff_summary}")
        }
        other => panic!("add_text failed: {other:?}"),
    }
    let shapes = full(&mut core);
    assert_eq!(shapes[0]["kind"], "text");
    assert_eq!(shapes[0]["size"], 24.0);
    assert_eq!(shapes[0]["fill"], "none");
}

#[test]
fn the_declared_schema_is_the_only_contract() {
    // A model that has just read one shape may reach for `id`. The schema says
    // `ids`, so Core refuses the call before the harness runs and names the
    // parameter it wanted — rather than the harness quietly accepting a shape
    // the grammar would never have produced.
    let Some(mut core) = core() else { return };
    let ids = stickies(&mut core, 1);
    match call(&mut core, "canvas.order", json!({"id": ids[0], "to": "front"})) {
        proto::ToolOutcome::Error { message } => {
            assert!(message.contains("required"), "{message}");
            assert!(message.contains("ids"), "the message must name it: {message}");
        }
        other => panic!("expected a schema refusal, got {other:?}"),
    }
    match call(&mut core, "canvas.order", json!({"ids": [ids[0]], "to": "front"})) {
        proto::ToolOutcome::Ok { .. } => {}
        other => panic!("the declared form must work: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The catalog behind the marketplace
// ---------------------------------------------------------------------------

fn registry_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("registry");
    if dir.join("planner").join("logic.wasm").exists() {
        Some(dir)
    } else {
        eprintln!("skipping: {} has not been built", dir.display());
        None
    }
}

fn core_with_catalog() -> Option<Core> {
    let harnesses = harness_dir()?;
    let registry = registry_dir()?;
    let mut cfg = Config::personal("tester");
    cfg.harness_dir = Some(harnesses);
    cfg.catalog_dirs = vec![registry];
    Some(Core::new(cfg).expect("creating Core"))
}

fn catalog(core: &mut Core) -> Vec<proto::CatalogEntry> {
    match core.handle(proto::Request::ListCatalog) {
        proto::Response::Catalog { entries } => entries,
        other => panic!("ListCatalog failed: {other:?}"),
    }
}

#[test]
fn the_catalog_lists_the_bundle_and_marks_what_is_installed() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let entries = catalog(&mut core);

    let board = entries
        .iter()
        .find(|e| e.id == WHITEBOARD)
        .expect("the installed whiteboard is missing from the catalog");
    assert!(board.installed, "it is installed, so it must say so");
    assert_eq!(board.installed_version.as_deref(), Some("1.0.0"));

    let planner = entries
        .iter()
        .find(|e| e.id == "io.localspace.planner")
        .expect("the planner is missing from the catalog");
    assert!(!planner.installed, "it is only in the bundle, not installed");
    assert!(planner.blocked.is_none());
    assert_eq!(planner.tier, proto::Tier::Wasm);
    assert!(planner.tool_count >= 8);
    assert_eq!(planner.front_door.len(), 2);
    assert!(planner.has_context_provider);
    assert!(planner.eval_cases >= 4, "a store entry needs an eval suite");
    assert!(
        planner.description.to_lowercase().contains("wip")
            || planner.description.to_lowercase().contains("column"),
        "{}",
        planner.description
    );
}

#[test]
fn capabilities_are_shown_in_words_a_non_engineer_can_read() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let entries = catalog(&mut core);
    for entry in &entries {
        assert!(
            !entry.capability_lines.is_empty(),
            "{} showed an empty list, which a reader has to interpret",
            entry.id
        );
    }

    // The whiteboard declares clipboard = "on-user-action" and nothing else, so
    // that is the one line — no jargon, and no mention of what it did not ask for.
    let board = entries.iter().find(|e| e.id == WHITEBOARD).unwrap();
    assert_eq!(
        board.capability_lines,
        vec!["Reads the clipboard when you paste".to_string()]
    );

    // The planner declares nothing at all, and the catalog says so outright
    // rather than leaving a blank the reader has to trust.
    let planner = entries
        .iter()
        .find(|e| e.id == "io.localspace.planner")
        .unwrap();
    assert_eq!(planner.capability_lines.len(), 1);
    assert!(
        planner.capability_lines[0].contains("Nothing outside its own document"),
        "{:?}",
        planner.capability_lines
    );
}

#[test]
fn installing_from_the_catalog_makes_the_harness_usable() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let path = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .expect("planner missing")
        .path;

    // Before: its tools do not exist at all.
    match core.call_tool("board.add_card", &json!({"text": "x"}), proto::Author::User) {
        proto::ToolOutcome::Error { message } => assert!(message.contains("no tool named")),
        other => panic!("expected the tool to be unknown, got {other:?}"),
    }

    assert!(matches!(
        core.handle(proto::Request::InstallHarness { path }),
        proto::Response::Ok
    ));

    // After: it installs, exposes its tools, and writes to its own document.
    let env = core.environment();
    assert!(env.harnesses.iter().any(|h| h.id == "io.localspace.planner"));

    let outcome = core.call_tool(
        "board.add_card",
        &json!({"text": "write the migration plan", "column": "In progress"}),
        proto::Author::User,
    );
    match outcome {
        proto::ToolOutcome::Ok {
            diff_summary,
            commit,
            ..
        } => {
            assert!(diff_summary.contains("In progress"), "{diff_summary}");
            assert!(commit.is_some());
        }
        other => panic!("add_card failed: {other:?}"),
    }

    // And the catalog now reports it as installed.
    let entries = catalog(&mut core);
    assert!(
        entries
            .iter()
            .find(|e| e.id == "io.localspace.planner")
            .unwrap()
            .installed
    );

    // Installing a package is an audited event, not a silent one.
    let records = core.audit_log().records();
    assert!(
        records
            .iter()
            .any(|r| r.event == "harness.install"
                && r.detail["harness"] == "io.localspace.planner"),
        "install was not audited: {:?}",
        records.iter().map(|r| &r.event).collect::<Vec<_>>()
    );
    assert!(core.audit_log().verify().is_ok());
}

#[test]
fn an_update_that_widens_capabilities_waits_for_the_user() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let source = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .unwrap()
        .path;
    core.handle(proto::Request::InstallHarness {
        path: source.clone(),
    });

    // Same package, one version later, now asking for the clipboard and the model.
    let dir = tempfile::tempdir().unwrap();
    let staged = dir.path().join("planner");
    std::fs::create_dir_all(&staged).unwrap();
    for name in ["tools.json", "evals.json", "logic.wasm"] {
        std::fs::copy(PathBuf::from(&source).join(name), staged.join(name)).unwrap();
    }
    let manifest = std::fs::read_to_string(PathBuf::from(&source).join("harness.toml"))
        .unwrap()
        .replace("version = \"1.0.0\"", "version = \"1.1.0\"")
        .replace("model = []", "model = [\"complete\"]")
        .replace("docs = \"none\"", "docs = \"acl\"");
    std::fs::write(staged.join("harness.toml"), manifest).unwrap();

    let token = match core.handle(proto::Request::InstallHarness {
        path: staged.display().to_string(),
    }) {
        proto::Response::InstallPrompt { diff, token, .. } => {
            assert!(diff.iter().any(|l| l.contains("model.complete")), "{diff:?}");
            assert!(diff.iter().any(|l| l.contains("documents")), "{diff:?}");
            token
        }
        other => panic!("a widened install must prompt, got {other:?}"),
    };

    // Nothing changed until the user answered.
    let installed_version = |core: &Core| {
        core.environment()
            .harnesses
            .iter()
            .find(|h| h.id == "io.localspace.planner")
            .map(|h| h.version.clone())
    };
    assert_eq!(installed_version(&core).as_deref(), Some("1.0.0"));

    // An answer with the wrong token is refused rather than assumed.
    assert!(matches!(
        core.handle(proto::Request::ApproveInstall {
            harness: "io.localspace.planner".into(),
            token: "not-the-token".into(),
        }),
        proto::Response::Error { .. }
    ));
    assert_eq!(installed_version(&core).as_deref(), Some("1.0.0"));

    core.handle(proto::Request::ApproveInstall {
        harness: "io.localspace.planner".into(),
        token,
    });
    assert_eq!(installed_version(&core).as_deref(), Some("1.1.0"));

    let records = core.audit_log().records();
    assert!(records
        .iter()
        .any(|r| r.event == "harness.capabilities_approved"));
}

#[test]
fn the_planner_provider_reports_what_is_over_its_wip_limit() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let path = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .unwrap()
        .path;
    core.handle(proto::Request::InstallHarness { path });
    core.handle(proto::Request::SetFocus {
        harness: Some("io.localspace.planner".into()),
    });

    // The default "In progress" column has a WIP limit of 3.
    for i in 0..4 {
        core.call_tool(
            "board.add_card",
            &json!({"text": format!("task {i}"), "column": "In progress"}),
            proto::Author::User,
        );
    }

    match core.handle(proto::Request::PreviewContext { budget: 600 }) {
        proto::Response::Context { blocks, .. } => {
            let block = blocks
                .iter()
                .find(|b| b.harness == "io.localspace.planner")
                .expect("no block from the planner");
            assert!(
                block.text.contains("over WIP limit"),
                "the one thing a planning board exists to say: {}",
                block.text
            );
            assert!(block.text.contains("In progress 4/3"), "{}", block.text);
        }
        other => panic!("PreviewContext failed: {other:?}"),
    }
}

#[test]
fn two_installed_harnesses_share_the_tool_budget_by_rank() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let path = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .unwrap()
        .path;
    core.handle(proto::Request::InstallHarness { path });

    // Focus the whiteboard, pin the planner: all of one, front doors of the other.
    core.handle(proto::Request::SetFocus {
        harness: Some(WHITEBOARD.into()),
    });
    core.handle(proto::Request::SetPinned {
        harness: "io.localspace.planner".into(),
        pinned: true,
    });

    let set = core.active_set();
    let names: Vec<&str> = set.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"canvas.move"), "focused harness in full");
    assert!(names.contains(&"board.columns"), "pinned front door");
    assert!(
        !names.contains(&"board.move_card"),
        "a pinned harness contributes front doors only: {names:?}"
    );
}

#[test]
fn find_capability_picks_the_right_harness_out_of_two() {
    let Some(mut core) = core_with_catalog() else {
        return;
    };
    let path = catalog(&mut core)
        .into_iter()
        .find(|e| e.id == "io.localspace.planner")
        .unwrap()
        .path;
    core.handle(proto::Request::InstallHarness { path });
    core.handle(proto::Request::SetFocus { harness: None });

    for (need, expected) in [
        ("move a card to another column on the board", "io.localspace.planner"),
        ("draw a red sticky note on a canvas", WHITEBOARD),
    ] {
        match core.handle(proto::Request::FindCapability { need: need.into() }) {
            proto::Response::Capabilities { hits } => {
                assert_eq!(hits[0].harness, expected, "`{need}` -> {hits:#?}");
            }
            other => panic!("FindCapability failed: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The agent loop, driven by a scripted worker
// ---------------------------------------------------------------------------

/// A worker that answers with a fixed sequence of tool calls.
///
/// This is not a stand-in for a model's judgement — the point is to exercise the
/// loop, the tools, the document and the assertions together, deterministically.
/// Whether a real model picks the right tools is what `localspace evals` measures
/// against the model the environment actually has loaded.
struct Script(Mutex<Vec<ChatReply>>);

fn reply(tool: &str, params: Value) -> ChatReply {
    ChatReply {
        text: String::new(),
        calls: vec![ProposedCall {
            id: "c0".into(),
            tool: tool.to_string(),
            params,
        }],
        prompt_tokens: 200,
        completion_tokens: 20,
    }
}

fn done() -> ChatReply {
    ChatReply {
        text: "Done.".into(),
        ..Default::default()
    }
}

impl Script {
    fn calls(calls: Vec<(&str, Value)>) -> Arc<Script> {
        let mut replies: Vec<ChatReply> =
            calls.into_iter().map(|(t, p)| reply(t, p)).collect();
        replies.push(done());
        Arc::new(Script(Mutex::new(replies)))
    }
}

impl ModelWorker for Script {
    fn info(&self) -> proto::ModelInfo {
        proto::ModelInfo {
            id: "scripted".into(),
            backend: "test".into(),
            context_len: 8192,
            supports_tools: true,
            supports_vision: false,
            loaded: true,
        }
    }
    fn chat(&self, _req: &ChatRequest) -> Result<ChatReply> {
        let mut r = self.0.lock().unwrap();
        Ok(if r.is_empty() { done() } else { r.remove(0) })
    }
    fn embed(&self, _t: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

/// A worker that decides from the prompt rather than from a queue, so it can
/// answer eval cases that arrive in any order.
struct PromptDriven;

impl ModelWorker for PromptDriven {
    fn info(&self) -> proto::ModelInfo {
        proto::ModelInfo {
            id: "prompt-driven".into(),
            backend: "test".into(),
            context_len: 8192,
            supports_tools: true,
            supports_vision: false,
            loaded: true,
        }
    }
    fn chat(&self, req: &ChatRequest) -> Result<ChatReply> {
        // Only the newest user turn matters; earlier ones are already handled.
        let last = req
            .prompt
            .rsplit("user: ")
            .next()
            .unwrap_or("")
            .to_lowercase();
        // Once a tool result for this turn is in the prompt, stop.
        if req.prompt.contains("<- ok:") {
            return Ok(done());
        }
        Ok(if last.contains("call this board") {
            reply("canvas.set_title", json!({"title": "Q4 planning"}))
        } else if last.contains("frame called") {
            reply("canvas.add_frame", json!({"name": "Backlog"}))
        } else {
            done()
        })
    }
    fn embed(&self, _t: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

#[test]
fn an_agent_turn_puts_three_red_stickies_on_the_board() {
    let Some(mut core) = core() else { return };
    core.router().write().unwrap().chat = Some(Script::calls(vec![
        (
            "canvas.add_sticky",
            json!({"text": "supply chain", "fill": "red"}),
        ),
        ("canvas.add_sticky", json!({"text": "hiring", "fill": "red"})),
        (
            "canvas.add_sticky",
            json!({"text": "FX exposure", "fill": "red"}),
        ),
    ]));

    core.handle(proto::Request::SendMessage {
        text: "Put the three risks on the board as red stickies.".into(),
    });

    let listed = board(&mut core);
    let shapes = listed["shapes"].as_array().unwrap();
    assert_eq!(shapes.len(), 3, "{listed}");
    assert!(shapes.iter().all(|s| s["fill"] == "red"));
    assert!(shapes.iter().any(|s| s["text"] == "FX exposure"));

    // Every write in the turn belongs to one run, so rejecting it is one action.
    let commits = match core.handle(proto::Request::GetHistory { limit: 10 }) {
        proto::Response::History { commits } => commits,
        other => panic!("history failed: {other:?}"),
    };
    assert_eq!(commits.len(), 3);
    let run = commits[0].run.clone().expect("agent commits carry a run id");
    assert!(commits.iter().all(|c| c.run.as_deref() == Some(run.as_str())));
    assert!(commits.iter().all(|c| c.author == proto::Author::Agent));

    core.handle(proto::Request::DropRun { run });
    assert_eq!(
        board(&mut core)["shapes"].as_array().unwrap().len(),
        0,
        "dropping the run must undo the whole turn"
    );
}

#[test]
fn a_tool_outside_the_active_set_is_refused_to_the_agent() {
    let Some(mut core) = core() else { return };
    // The whiteboard is not focused and not pinned, so its tools are out of context.
    core.handle(proto::Request::SetFocus { harness: None });
    core.router().write().unwrap().chat = Some(Script::calls(vec![(
        "canvas.add_sticky",
        json!({"text": "sneaky"}),
    )]));

    core.handle(proto::Request::SendMessage {
        text: "add a sticky".into(),
    });

    assert_eq!(
        board(&mut core)["shapes"].as_array().unwrap().len(),
        0,
        "a tool the model was never shown must not be callable"
    );
}

#[test]
fn the_eval_suite_runs_against_the_installed_package() {
    let Some(mut core) = core() else { return };
    // Driven deterministically, to prove the runner's plumbing: a fresh document
    // per case, real tools, real assertions, honest reporting of what failed.
    core.router().write().unwrap().chat = Some(Arc::new(PromptDriven));

    match core.handle(proto::Request::RunEvals {
        harness: WHITEBOARD.into(),
    }) {
        proto::Response::Evals(report) => {
            assert_eq!(report.total, 6, "evals.json ships six cases");

            for name in ["name the board", "a frame to group work"] {
                let case = report
                    .cases
                    .iter()
                    .find(|c| c.name == name)
                    .unwrap_or_else(|| panic!("case `{name}` is missing"));
                assert!(case.passed, "{name}: {}", case.detail);
            }
            // Cases start from a clean document, so a later case cannot inherit
            // an earlier one's shapes.
            let read_case = report
                .cases
                .iter()
                .find(|c| c.name == "read before writing")
                .unwrap();
            assert!(read_case.passed, "{}", read_case.detail);

            // And it reports honestly on the ones this worker does not drive.
            assert!(report.passed < report.total);
            assert!(report
                .cases
                .iter()
                .any(|c| !c.passed && !c.detail.is_empty()));
        }
        other => panic!("RunEvals failed: {other:?}"),
    }
}
