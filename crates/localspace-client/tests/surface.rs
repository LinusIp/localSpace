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
    let mut runner =
        SurfaceRunner::load("test/board", &bytes, 16).expect("the runner refused the surface");
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
    assert!(paint.meshes > 0, "the surface drew nothing at all");
    assert!(paint.vertices > 0, "every mesh was empty");
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
    let mut runner = SurfaceRunner::load("test/board", &bytes, 16).expect("load");
    runner.set_doc(DOC.to_string());

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    quiet_frame(&mut runner, &ctx, rect, 0.0);
    assert_eq!(
        runner.guest_frames, 1,
        "the first frame carries the document"
    );

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
        runner.guest_frames,
        after_settling,
        "twenty quiet host frames ran the guest {} more time(s)",
        runner.guest_frames - after_settling
    );
    assert!(runner.skipped_frames >= 20, "skips were not counted");

    // The same document again is not a change.
    runner.set_doc(DOC.to_string());
    quiet_frame(&mut runner, &ctx, rect, 3.0);
    assert_eq!(
        runner.guest_frames, after_settling,
        "an identical document re-ran the guest"
    );

    // A different document is.
    runner.set_doc(DOC.replacen('{', "{\"changed\":true,", 1));
    quiet_frame(&mut runner, &ctx, rect, 3.1);
    assert_eq!(
        runner.guest_frames,
        after_settling + 1,
        "a changed document did not reach the guest"
    );
}

#[test]
fn a_module_that_is_not_a_surface_is_refused_with_a_useful_message() {
    // A valid wasm module that exports none of the three ABI functions.
    let wat = b"\0asm\x01\0\0\0";
    let err = match SurfaceRunner::load("test/empty", wat, 16) {
        Ok(_) => panic!("an empty module was accepted as a surface"),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        err.contains("memory") || err.contains("hs_"),
        "unhelpful error: {err}"
    );
}

/// One host frame; the surface's error, if it produced one, instead of a panic.
fn try_frame(
    runner: &mut SurfaceRunner,
    ctx: &egui::Context,
    rect: egui::Rect,
    time: f64,
    events: Vec<egui::Event>,
    messages: Vec<Vec<u8>>,
) -> Result<(), String> {
    let mut messages = Some(messages);
    let mut error = None;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(rect),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ui| {
            if let Err(e) = runner.show(ui, rect, messages.take().unwrap_or_default()) {
                error = Some(format!("{e:#}"));
            }
        },
    );
    out.textures_delta.clear();
    error.map_or(Ok(()), Err)
}

/// A board with enough text that every zoom level rasterises a good many glyphs.
fn text_heavy_doc() -> String {
    let mut shapes = Vec::new();
    for i in 0..40 {
        let (x, y) = ((i % 8) as f32 * 160.0, (i / 8) as f32 * 130.0);
        shapes.push(format!(
            r#"{{"id":"s{i}","kind":"sticky","x":{x},"y":{y},"w":130.0,"h":110.0,"fill":"yellow","text":"Risk {i}: supply chain lead times, hiring, FX exposure"}}"#
        ));
    }
    for (i, size) in [12.0, 16.0, 20.0, 28.0, 40.0].iter().enumerate() {
        shapes.push(format!(
            r#"{{"id":"t{i}","kind":"text","x":{},"y":700.0,"w":300.0,"h":60.0,"size":{size},"text":"Heading {i} at {size} pt"}}"#,
            i as f32 * 320.0
        ));
    }
    format!(
        r#"{{"title":"Stress","frames":[],"shapes":[{}],"selection":[]}}"#,
        shapes.join(",")
    )
}

/// What the whiteboard declares in `[resources] memory_mb.surface`.
const SURFACE_BUDGET_MB: u32 = 32;

/// A board a team would actually make: a frame of stickies and one label.
fn ordinary_doc() -> String {
    let mut shapes = Vec::new();
    for i in 0..12 {
        let (x, y) = (40.0 + (i % 4) as f32 * 160.0, 60.0 + (i / 4) as f32 * 130.0);
        shapes.push(format!(
            r#"{{"id":"s{i}","kind":"sticky","x":{x},"y":{y},"w":130.0,"h":110.0,"fill":"yellow","text":"Risk {i}: lead times","frame":"f1"}}"#
        ));
    }
    shapes.push(
        r#"{"id":"t0","kind":"text","x":40.0,"y":10.0,"w":300.0,"h":30.0,"size":18.0,"text":"Q4 risks"}"#
            .to_string(),
    );
    format!(
        r#"{{"title":"Q4","frames":[{{"id":"f1","name":"Risks","x":0.0,"y":0.0,"w":700.0,"h":460.0}}],"shapes":[{}],"selection":[]}}"#,
        shapes.join(",")
    )
}

/// Bytes at rest and at peak, guest frames, and the steps at which memory grew.
struct Sweep {
    at_rest: usize,
    peak: usize,
    frames: u64,
    growth: Vec<String>,
}

#[test]
fn zooming_an_ordinary_board_stays_far_inside_the_surface_budget() {
    let Some(bytes) = board_wasm() else { return };
    let s = zoom_sweep(&bytes, ordinary_doc());
    eprintln!(
        "ordinary board: {:.1} MB at rest, {:.1} MB peak over {} guest frames; grew at: {}",
        s.at_rest as f64 / 1048576.0,
        s.peak as f64 / 1048576.0,
        s.frames,
        s.growth.join("; ")
    );
    assert!(
        s.peak <= 16 * 1024 * 1024,
        "an ordinary board peaked at {:.1} MB while zooming",
        s.peak as f64 / 1048576.0
    );
}

#[test]
fn zooming_through_every_level_stays_inside_the_surface_budget() {
    // The failure this guards: each distinct text size rasterises new glyphs
    // into the surface's font atlas, and every time that atlas grows the whole
    // image travels through the surface's memory. A smooth zoom used to mint a
    // new size per frame, and a board declared at 16 MB died at 17 MB in a
    // user's hands.
    let Some(bytes) = board_wasm() else { return };
    let s = zoom_sweep(&bytes, text_heavy_doc());
    eprintln!(
        "text-heavy board: {:.1} MB at rest, {:.1} MB peak over {} guest frames; grew at: {}",
        s.at_rest as f64 / 1048576.0,
        s.peak as f64 / 1048576.0,
        s.frames,
        s.growth.join("; ")
    );
    let headroom = SURFACE_BUDGET_MB as usize * 1024 * 1024 * 3 / 4;
    assert!(
        s.peak <= headroom,
        "peak {:.1} MB leaves too little headroom under {SURFACE_BUDGET_MB} MB",
        s.peak as f64 / 1048576.0
    );
}

/// Zoom a board through every level twice, with the buttons and with the wheel,
/// under the whiteboard's declared budget. Panics if the surface fails.
fn zoom_sweep(bytes: &[u8], doc: String) -> Sweep {
    let budget_mb = SURFACE_BUDGET_MB;
    let mut runner = SurfaceRunner::load("test/board", bytes, budget_mb).expect("load");
    let at_load = runner.memory_bytes();
    runner.set_doc(doc);

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1400.0, 900.0));
    let centre = egui::pos2(700.0, 450.0);
    let mut time = 0.0;

    quiet_frame(&mut runner, &ctx, rect, time);
    let at_rest = runner.memory_bytes();
    eprintln!(
        "surface memory: {:.1} MB as instantiated (data and stack), {:.1} MB after the first frame",
        at_load as f64 / 1048576.0,
        at_rest as f64 / 1048576.0
    );
    let mut peak = at_rest;
    // The memory after every step, so a failure says where it grew.
    let mut curve: Vec<(String, usize)> = Vec::new();
    let mut failure = None;

    // The top bar's buttons: 25 % to 400 % in steps, back down, and again —
    // the second pass is what shows whether the memory keeps ratcheting up
    // with every rebuild of the atlas or has found its level.
    let sweep: Vec<i32> = (25..=400)
        .step_by(5)
        .chain((25..=400).rev().step_by(5))
        .collect();
    let levels: Vec<i32> = sweep.iter().chain(sweep.iter()).copied().collect();
    for percent in levels {
        time += 0.016;
        let cmd = serde_json::json!({"kind": "zoom", "value": percent as f32 / 100.0})
            .to_string()
            .into_bytes();
        let result = try_frame(
            &mut runner,
            &ctx,
            rect,
            time,
            vec![egui::Event::PointerMoved(centre)],
            vec![cmd],
        );
        curve.push((format!("button {percent}%"), runner.memory_bytes()));
        peak = peak.max(runner.memory_bytes());
        if let Err(e) = result {
            failure = Some(e);
            break;
        }
    }

    // The wheel: a smooth zoom in and back out, one notch per frame.
    for i in 0..400 {
        if failure.is_some() {
            break;
        }
        time += 0.016;
        let delta = if i < 200 { 6.0 } else { -6.0 };
        let events = vec![
            egui::Event::PointerMoved(centre),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            },
        ];
        let result = try_frame(&mut runner, &ctx, rect, time, events, Vec::new());
        curve.push((format!("wheel {i}"), runner.memory_bytes()));
        peak = peak.max(runner.memory_bytes());
        if let Err(e) = result {
            failure = Some(e);
            break;
        }
    }

    // Every step at which the surface's memory grew.
    let mut last = at_rest;
    let mut growth = Vec::new();
    for (step, bytes) in &curve {
        if *bytes > last {
            growth.push(format!("{step}: {:.1} MB", *bytes as f64 / 1048576.0));
            last = *bytes;
        }
    }
    if let Some(e) = failure {
        panic!(
            "the surface failed while zooming: {e}\n  grew at: {}",
            growth.join("; ")
        );
    }
    assert!(
        runner.over_budget().is_none(),
        "the surface broke its {budget_mb} MB budget while zooming"
    );
    Sweep {
        at_rest,
        peak,
        frames: runner.guest_frames,
        growth,
    }
}

/// A thousand stickies on a 40 by 25 grid: at 100 % in a 1400 by 900 panel
/// about sixty are on screen.
fn thousand_sticky_doc() -> String {
    let mut shapes = Vec::with_capacity(1000);
    for i in 0..1000 {
        let (x, y) = ((i % 40) as f32 * 160.0, (i / 40) as f32 * 130.0);
        shapes.push(format!(
            r#"{{"id":"s{i}","kind":"sticky","x":{x},"y":{y},"w":130.0,"h":110.0,"fill":"yellow","text":"Item {i}: a line of text that wraps inside the note"}}"#
        ));
    }
    format!(
        r#"{{"title":"Big","frames":[],"shapes":[{}],"selection":[]}}"#,
        shapes.join(",")
    )
}

#[test]
fn a_thousand_shape_board_costs_only_its_visible_part() {
    // Immediate mode rebuilds the shape list every frame, so a board must draw
    // what is on screen and skip the rest. Here sixty of a thousand stickies
    // are visible; a frame must cost what sixty cost, not what a thousand do.
    let Some(bytes) = board_wasm() else { return };
    let mut runner = SurfaceRunner::load("test/board", &bytes, SURFACE_BUDGET_MB).expect("load");
    runner.set_doc(thousand_sticky_doc());

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1400.0, 900.0));
    let mut time = 0.0;
    // Warm up: fonts, galleys and the first document hand-over.
    for _ in 0..5 {
        time += 0.016;
        quiet_frame(&mut runner, &ctx, rect, time);
        let events = vec![egui::Event::PointerMoved(egui::pos2(
            700.0 + time as f32,
            450.0,
        ))];
        try_frame(&mut runner, &ctx, rect, time + 0.008, events, Vec::new()).expect("frame");
    }

    // Thirty frames with the pointer moving, so the guest runs every time.
    let mut total_ms = 0.0;
    let mut max_ms: f32 = 0.0;
    let before = runner.guest_frames;
    for i in 0..30 {
        time += 0.016;
        let events = vec![egui::Event::PointerMoved(egui::pos2(
            600.0 + i as f32 * 3.0,
            400.0,
        ))];
        try_frame(&mut runner, &ctx, rect, time, events, Vec::new()).expect("frame");
        total_ms += runner.stats.last_ms;
        max_ms = max_ms.max(runner.stats.last_ms);
    }
    let ran = (runner.guest_frames - before) as f32;
    let avg_ms = total_ms / ran.max(1.0);
    eprintln!(
        "thousand-sticky board: guest frame avg {avg_ms:.2} ms, max {max_ms:.2} ms over {ran} frames; memory {:.1} MB",
        runner.memory_bytes() as f64 / 1048576.0
    );
    assert!(
        ran >= 30.0,
        "the guest must run on every frame with pointer input"
    );
    assert!(
        avg_ms < 8.0,
        "a frame of a thousand-sticky board costs {avg_ms:.2} ms; the surface budget is 8 ms"
    );

    // The same thousand stickies, all off screen: what a frame costs before
    // anything is drawn at all — the per-shape work that visibility cannot save.
    let mut shapes = Vec::with_capacity(1000);
    for i in 0..1000 {
        let (x, y) = (
            50000.0 + (i % 40) as f32 * 160.0,
            50000.0 + (i / 40) as f32 * 130.0,
        );
        shapes.push(format!(
            r#"{{"id":"s{i}","kind":"sticky","x":{x},"y":{y},"w":130.0,"h":110.0,"fill":"yellow","text":"Item {i}: a line of text that wraps inside the note"}}"#
        ));
    }
    runner.set_doc(format!(
        r#"{{"title":"Far","frames":[],"shapes":[{}],"selection":[]}}"#,
        shapes.join(",")
    ));
    for _ in 0..3 {
        time += 0.016;
        quiet_frame(&mut runner, &ctx, rect, time);
    }
    let mut far_total = 0.0;
    let before = runner.guest_frames;
    for i in 0..30 {
        time += 0.016;
        let events = vec![egui::Event::PointerMoved(egui::pos2(
            600.0 + i as f32 * 3.0,
            400.0,
        ))];
        try_frame(&mut runner, &ctx, rect, time, events, Vec::new()).expect("frame");
        far_total += runner.stats.last_ms;
    }
    let far_ran = (runner.guest_frames - before) as f32;
    eprintln!(
        "thousand stickies all off screen: guest frame avg {:.2} ms over {far_ran} frames",
        far_total / far_ran.max(1.0)
    );
}

#[test]
fn a_throttled_surface_is_entered_less_not_more() {
    // Being slow is a reason to enter a surface less often. The bug this
    // guards: the runner used to run a throttled guest on every host frame, so
    // a slow surface was made slower by its own penalty.
    let Some(bytes) = board_wasm() else { return };
    let mut runner = SurfaceRunner::load("test/board", &bytes, 16).expect("load");
    runner.set_doc(DOC.to_string());

    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    for i in 0..=5 {
        quiet_frame(&mut runner, &ctx, rect, i as f64 * 0.1);
    }
    let settled = runner.guest_frames;

    runner.stats.throttled = true;
    for i in 6..=25 {
        quiet_frame(&mut runner, &ctx, rect, i as f64 * 0.1);
    }
    assert_eq!(
        runner.guest_frames,
        settled,
        "a throttled surface was entered on {} quiet host frames",
        runner.guest_frames - settled
    );
}
