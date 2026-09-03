//! Conformance test for the `egui` surface ABI (spec §5.3).
//!
//! Loads the reference whiteboard surface through the Client's own
//! `SurfaceRunner`, runs a frame headlessly, and checks the output paints. This
//! is the test that would catch a schema drift between the pinned epaint in the
//! SDK and the one the Client links.

#![cfg(not(target_arch = "wasm32"))]

use localspace_client::surface::SurfaceRunner;
use std::path::PathBuf;

fn board_wasm() -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("harnesses/whiteboard/ui/board.wasm");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping: {} has not been built", path.display());
            None
        }
    }
}

const DOC: &str = r#"{
    "title": "Board",
    "frames": [{"id": "f1", "name": "Risks", "x": 0.0, "y": 0.0, "w": 600.0, "h": 400.0}],
    "shapes": [
        {"id": "s1", "kind": "sticky", "x": 20.0, "y": 20.0, "w": 130.0, "h": 110.0,
         "fill": "red", "text": "supply chain", "frame": "f1"},
        {"id": "s2", "kind": "sticky", "x": 180.0, "y": 20.0, "w": 130.0, "h": 110.0,
         "fill": "red", "text": "hiring", "frame": "f1"}
    ],
    "selection": ["s1"]
}"#;

#[test]
fn a_surface_imports_only_the_wasm_bindgen_placeholders() {
    let Some(bytes) = board_wasm() else { return };
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::from_binary(&engine, &bytes).expect("not a wasm module");

    // `egui` depends on `web-sys` unconditionally on wasm32, so the linker emits
    // wasm-bindgen placeholders whether or not any of that code is reachable.
    // The Client stubs exactly those with traps and refuses anything else, so a
    // surface still cannot reach the filesystem, the network or a model.
    let stray: Vec<String> = module
        .imports()
        .filter(|i| !i.module().starts_with("__wbindgen"))
        .map(|i| format!("{}::{}", i.module(), i.name()))
        .collect();
    assert!(
        stray.is_empty(),
        "a surface must import nothing but the wasm-bindgen placeholders; found: {}",
        stray.join(", ")
    );
}

#[test]
fn the_reference_surface_paints_a_frame_through_the_runner() {
    let Some(bytes) = board_wasm() else { return };
    let mut runner = SurfaceRunner::load("test/board", &bytes).expect("the runner refused the surface");
    runner.set_doc(DOC.to_string());

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    let mut painted = None;
    let mut error = None;
    let mut host_output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(rect),
            time: Some(0.0),
            ..Default::default()
        },
        |ui| match runner.show(ui, rect, Vec::new()) {
            Ok(paint) => painted = Some(paint),
            Err(e) => error = Some(format!("{e:#}")),
        },
    );
    // The host's own texture deltas: eframe applies these; here nothing paints,
    // so release them or epaint panics on drop.
    host_output.textures_delta.clear();

    if let Some(e) = error {
        panic!("the surface failed to run: {e}");
    }
    let paint = painted.expect("the runner produced nothing");
    assert!(
        !paint.primitives.is_empty(),
        "the surface drew nothing at all"
    );
    assert!(
        paint.primitives.iter().any(|p| !p.mesh.vertices.is_empty()),
        "every mesh was empty"
    );
    assert!(runner.stats.last_ms > 0.0, "no frame time was recorded");
    assert!(
        !runner.stats.throttled,
        "one frame cannot trip the slow-surface throttle"
    );
}

/// Run one host frame with no input and no messages; return whether the runner
/// produced output without error.
fn quiet_frame(runner: &mut SurfaceRunner, ctx: &egui::Context, rect: egui::Rect, time: f64) {
    let mut error = None;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(rect),
            time: Some(time),
            ..Default::default()
        },
        |ui| {
            if let Err(e) = runner.show(ui, rect, Vec::new()) {
                error = Some(format!("{e:#}"));
            }
        },
    );
    out.textures_delta.clear();
    if let Some(e) = error {
        panic!("the surface failed to run: {e}");
    }
}

#[test]
fn a_frame_in_which_nothing_changed_does_not_run_the_guest() {
    // §16.3: pay per change, not per frame. A repaint the host does for its own
    // reasons — the mouse crossed the rail, a notice appeared — must not enter
    // the guest, and the same document coming back from Core after the guest's
    // own edit must not be pushed at it again.
    let Some(bytes) = board_wasm() else { return };
    let mut runner = SurfaceRunner::load("test/board", &bytes).expect("load");
    runner.set_doc(DOC.to_string());

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    quiet_frame(&mut runner, &ctx, rect, 0.0);
    assert_eq!(runner.guest_frames, 1, "the first frame carries the document");

    // Settle: a fresh egui context may ask for a repaint or two while it lays
    // out fonts. Those requests are honoured, and then it goes quiet.
    for i in 1..=5 {
        quiet_frame(&mut runner, &ctx, rect, i as f64 * 0.1);
    }
    let after_settling = runner.guest_frames;

    for i in 6..=25 {
        quiet_frame(&mut runner, &ctx, rect, i as f64 * 0.1);
    }
    assert_eq!(
        runner.guest_frames, after_settling,
        "twenty quiet host frames ran the guest {} more time(s)",
        runner.guest_frames - after_settling
    );
    assert!(runner.skipped_frames >= 20, "skips were not counted");

    // The same document again is not a change.
    runner.set_doc(DOC.to_string());
    quiet_frame(&mut runner, &ctx, rect, 3.0);
    assert_eq!(runner.guest_frames, after_settling, "an identical document re-ran the guest");

    // A different document is.
    runner.set_doc(DOC.replacen('{', "{\"changed\":true,", 1));
    quiet_frame(&mut runner, &ctx, rect, 3.1);
    assert_eq!(runner.guest_frames, after_settling + 1, "a changed document did not reach the guest");
}

#[test]
fn a_module_that_is_not_a_surface_is_refused_with_a_useful_message() {
    // A valid wasm module that exports none of the three ABI functions.
    let wat = b"\0asm\x01\0\0\0";
    let err = match SurfaceRunner::load("test/empty", wat) {
        Ok(_) => panic!("an empty module was accepted as a surface"),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        err.contains("memory") || err.contains("hs_"),
        "unhelpful error: {err}"
    );
}
