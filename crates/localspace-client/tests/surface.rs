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
