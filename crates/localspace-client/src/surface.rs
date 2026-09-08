//! The `SurfaceRunner`: runs a harness's `kind = "egui"` surface module and paints
//! its output into a panel rect.
//!
//! One implementation per target, identical behaviour. Native uses `wasmtime`;
//! the browser Client uses the browser's own `WebAssembly` API through a small JS
//! shim. The same `.wasm` file runs on both, because the ABI is bytes in and
//! bytes out (see `localspace-surface-sdk`).

use egui::{Rect, Vec2};
use localspace_surface_sdk as sdk;
use std::collections::HashMap;

/// A surface that exceeds this for three frames running gets a visible badge and
/// is throttled, so one bad plugin cannot stall the Client.
pub const FRAME_BUDGET_MS: f32 = 8.0;

pub struct SurfaceStats {
    pub last_ms: f32,
    pub slow_streak: u32,
    pub throttled: bool,
}

impl Default for SurfaceStats {
    fn default() -> Self {
        SurfaceStats {
            last_ms: 0.0,
            slow_streak: 0,
            throttled: false,
        }
    }
}

/// What a frame produced, after the host has taken ownership of the textures.
pub struct FramePaint {
    pub primitives: Vec<sdk::WirePrimitive>,
    pub repaint_after_ms: u64,
    pub doc: Option<String>,
    pub messages: Vec<Vec<u8>>,
    pub cursor: sdk::CursorIcon,
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use anyhow::{Context, Result};
    use wasmtime::{Engine, Memory, Module, Store, TypedFunc};

    /// Store data for a surface: its `[resources] memory_mb.surface` budget and
    /// whether it ever asked for more. The enforcement point for the surface
    /// half of spec §1.2.
    pub struct Limits {
        budget: usize,
        pub over_budget: Option<usize>,
    }

    impl wasmtime::ResourceLimiter for Limits {
        fn memory_growing(
            &mut self,
            _current: usize,
            desired: usize,
            _maximum: Option<usize>,
        ) -> wasmtime::Result<bool> {
            if desired > self.budget {
                self.over_budget = Some(desired);
                return Ok(false);
            }
            Ok(true)
        }

        fn table_growing(
            &mut self,
            _current: usize,
            desired: usize,
            _maximum: Option<usize>,
        ) -> wasmtime::Result<bool> {
            Ok(desired <= 1_000_000)
        }
    }

    pub struct Runner {
        store: Store<Limits>,
        memory: Memory,
        alloc: TypedFunc<i32, i32>,
        init: TypedFunc<(i32, i32), ()>,
        frame: TypedFunc<(i32, i32), i64>,
    }

    impl Runner {
        pub fn load(bytes: &[u8], memory_mb: u32) -> Result<Runner> {
            // Same on-disk compile cache as Core's harness engine: a surface is
            // compiled once per machine, and the UI thread is not held for a
            // JIT on every launch.
            let mut config = wasmtime::Config::new();
            if let Ok(cache) = wasmtime::Cache::from_file(None) {
                config.cache(Some(cache));
            }
            let engine = Engine::new(&config).map_err(|e| anyhow::anyhow!("{e}"))?;
            let module = Module::from_binary(&engine, bytes)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("loading the surface module")?;
            let mut store = Store::new(
                &engine,
                Limits {
                    budget: memory_mb as usize * 1024 * 1024,
                    over_budget: None,
                },
            );
            // Every memory.grow, the initial allocation included, is checked
            // against the declared surface budget.
            store.limiter(|l| l);

            // A surface reaches nothing: no filesystem, no network, no model.
            //
            // It does arrive with wasm-bindgen's placeholder imports, because
            // `egui` depends on `web-sys` unconditionally on wasm32 and the
            // linker emits them whether or not any of it is reachable. Those are
            // stubbed with traps: a surface that really tries to call into the
            // browser fails loudly instead of quietly getting JS access. Any
            // other import is refused outright.
            let mut linker: wasmtime::Linker<Limits> = wasmtime::Linker::new(&engine);
            for import in module.imports() {
                if !import.module().starts_with("__wbindgen") {
                    anyhow::bail!(
                        "a surface may not import anything, but this one imports `{}::{}`",
                        import.module(),
                        import.name()
                    );
                }
                let wasmtime::ExternType::Func(ty) = import.ty() else {
                    anyhow::bail!(
                        "a surface may only import functions, but `{}::{}` is not one",
                        import.module(),
                        import.name()
                    );
                };
                let what = format!("{}::{}", import.module(), import.name());
                linker
                    .func_new(import.module(), import.name(), ty, move |_, _, _| {
                        Err(wasmtime::Error::msg(format!(
                            "this surface called `{what}`: surfaces have no browser or host access"
                        )))
                    })
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }

            let instance = match linker.instantiate(&mut store, &module) {
                Ok(i) => i,
                Err(e) => {
                    if let Some(asked) = store.data().over_budget {
                        anyhow::bail!(
                            "this surface needs {} MB just to start, over its declared \
                             [resources] memory_mb.surface = {memory_mb} MB",
                            asked.div_ceil(1024 * 1024)
                        );
                    }
                    return Err(anyhow::anyhow!("{e}")).context("instantiating the surface");
                }
            };


            let memory = instance
                .get_memory(&mut store, "memory")
                .context("the surface exports no memory")?;
            let alloc = instance
                .get_typed_func::<i32, i32>(&mut store, "hs_alloc")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("the surface exports no hs_alloc")?;
            let init = instance
                .get_typed_func::<(i32, i32), ()>(&mut store, "hs_init")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("the surface exports no hs_init")?;
            let frame = instance
                .get_typed_func::<(i32, i32), i64>(&mut store, "hs_frame")
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("the surface exports no hs_frame")?;

            Ok(Runner {
                store,
                memory,
                alloc,
                init,
                frame,
            })
        }

        /// Bytes the surface asked for beyond its budget, if it ever did.
        pub fn over_budget(&self) -> Option<usize> {
            self.store.data().over_budget
        }

        fn write(&mut self, bytes: &[u8]) -> Result<(i32, i32)> {
            let ptr = self
                .alloc
                .call(&mut self.store, bytes.len() as i32)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            self.memory
                .write(&mut self.store, ptr as usize, bytes)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok((ptr, bytes.len() as i32))
        }

        pub fn init(&mut self, bytes: &[u8]) -> Result<()> {
            let (ptr, len) = self.write(bytes)?;
            self.init
                .call(&mut self.store, (ptr, len))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(())
        }

        pub fn frame(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
            let (ptr, len) = self.write(bytes)?;
            let ret = self
                .frame
                .call(&mut self.store, (ptr, len))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let (out_ptr, out_len) = localspace_surface_sdk::unpack(ret);
            let mut out = vec![0u8; out_len as usize];
            self.memory
                .read(&self.store, out_ptr as usize, &mut out)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(out)
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use anyhow::{bail, Result};

    /// The browser runner drives the same three exports through the browser's own
    /// `WebAssembly` API. It is not built in this configuration — see
    /// `docs/STATUS.md`; the desktop path is the one that runs today.
    pub struct Runner;

    impl Runner {
        pub fn over_budget(&self) -> Option<usize> {
            None
        }
        pub fn load(_bytes: &[u8], _memory_mb: u32) -> Result<Runner> {
            bail!("the browser SurfaceRunner is not built in this configuration")
        }
        pub fn init(&mut self, _bytes: &[u8]) -> Result<()> {
            bail!("the browser SurfaceRunner is not built in this configuration")
        }
        pub fn frame(&mut self, _bytes: &[u8]) -> Result<Vec<u8>> {
            bail!("the browser SurfaceRunner is not built in this configuration")
        }
    }
}

pub struct SurfaceRunner {
    inner: imp::Runner,
    /// Guest texture id -> the host's own handle. The Client namespaces these so
    /// a surface's font atlas cannot collide with the host's.
    textures: HashMap<epaint::TextureId, egui::TextureHandle>,
    name: String,
    /// `[resources] memory_mb.surface`, for the message when it is exceeded.
    memory_mb: u32,
    pub stats: SurfaceStats,
    initialised: bool,
    /// Something the guest has not seen yet: a document, a message, a command.
    dirty: bool,
    /// The document the guest currently holds — whichever side wrote it last.
    last_doc: Option<String>,
    /// A document to hand over on the next frame the guest runs.
    pending_doc: Option<String>,
    /// Last frame's meshes, already translated for `cached_offset` and remapped
    /// to host texture ids. A frame in which nothing changed paints these and
    /// never enters the guest (§16.3: pay per change, not per frame).
    cached: Vec<(Rect, std::sync::Arc<epaint::Mesh>)>,
    cached_offset: Vec2,
    cached_cursor: sdk::CursorIcon,
    /// How often the guest actually ran, and how often it was skipped.
    pub guest_frames: u64,
    pub skipped_frames: u64,
    /// False until Core has handed over a real document. Until then the guest
    /// is looking at a placeholder, and nothing it writes may reach the store.
    received_doc: bool,
}

impl SurfaceRunner {
    pub fn load(name: &str, bytes: &[u8], memory_mb: u32) -> anyhow::Result<SurfaceRunner> {
        Ok(SurfaceRunner {
            inner: imp::Runner::load(bytes, memory_mb)?,
            textures: HashMap::new(),
            name: name.to_string(),
            memory_mb,
            stats: SurfaceStats::default(),
            initialised: false,
            dirty: true,
            last_doc: None,
            pending_doc: None,
            cached: Vec::new(),
            cached_offset: Vec2::ZERO,
            cached_cursor: sdk::CursorIcon::Default,
            guest_frames: 0,
            skipped_frames: 0,
            received_doc: false,
        })
    }

    /// Set once the surface has asked for more than its budget. The Client
    /// drops and reloads a runner that reports this, and says so.
    pub fn over_budget(&self) -> Option<usize> {
        self.inner.over_budget()
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Hand the surface a document. Only a document the guest does not already
    /// hold is pushed — including the round trip after the guest's own edit,
    /// which comes back from Core as the same JSON the guest sent.
    pub fn set_doc(&mut self, doc: String) {
        self.received_doc = true;
        if self.last_doc.as_deref() != Some(doc.as_str()) {
            self.last_doc = Some(doc.clone());
            self.pending_doc = Some(doc);
            self.dirty = true;
        }
    }

    fn ensure_init(&mut self, ctx: &egui::Context) -> anyhow::Result<()> {
        if self.initialised {
            return Ok(());
        }
        let cfg = sdk::SurfaceInit {
            shape_schema: sdk::SHAPE_SCHEMA,
            pixels_per_point: ctx.pixels_per_point(),
            dark_mode: ctx.theme() == egui::Theme::Dark,
            doc: self.last_doc.clone().unwrap_or_else(|| "{}".into()),
        };
        self.inner.init(&postcard::to_allocvec(&cfg)?)?;
        self.initialised = true;
        Ok(())
    }

    /// Run one frame and paint it into `rect`. Returns what the surface wants
    /// sent back to its logic.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        messages: Vec<Vec<u8>>,
    ) -> anyhow::Result<FramePaint> {
        let ctx = ui.ctx().clone();
        self.ensure_init(&ctx)?;

        let offset = rect.min.to_vec2();
        let response = ui.interact(
            rect,
            egui::Id::new(("surface", &self.name)),
            egui::Sense::click_and_drag(),
        );
        let hovered = response.hovered() || response.dragged();

        // Keyboard reaches the surface only once it has been clicked. A hover,
        // a focus change, or a window being maximised is not a reason to let a
        // Delete through to the board — a stray event during exactly that was
        // enough to wipe every shape once.
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let focused = response.has_focus();

        let raw_input = self.raw_input(&ctx, rect, hovered, focused);
        let has_input = !raw_input.events.is_empty();

        // Pay per change, not per frame (§16.3). The guest runs only when input
        // arrived, a document or message is waiting, it asked for a repaint, or
        // it is being throttled. Otherwise last frame's meshes are painted again
        // and the guest is never entered.
        if !self.dirty && !has_input && messages.is_empty() && !self.stats.throttled {
            self.skipped_frames += 1;
            self.paint_cached(ui, rect, offset);
            if let Some(cursor) = map_cursor(self.cached_cursor) {
                ctx.set_cursor_icon(cursor);
            }
            return Ok(FramePaint {
                primitives: Vec::new(),
                repaint_after_ms: u64::MAX,
                doc: None,
                messages: Vec::new(),
                cursor: self.cached_cursor,
            });
        }

        let input = sdk::FrameInput {
            raw_input,
            doc: self.pending_doc.take(),
            messages,
        };

        let started = std::time::Instant::now();
        let out_bytes = match self.inner.frame(&postcard::to_allocvec(&input)?) {
            Ok(bytes) => bytes,
            Err(e) => {
                // A trap after a refused grow means the budget, not a bug in
                // the frame. Name it, so the Client can restart and explain.
                if let Some(asked) = self.inner.over_budget() {
                    anyhow::bail!(
                        "surface exceeded its declared [resources] memory_mb.surface = {} MB \
                         (asked for {} MB)",
                        self.memory_mb,
                        asked.div_ceil(1024 * 1024)
                    );
                }
                return Err(e);
            }
        };
        let elapsed = started.elapsed().as_secs_f32() * 1000.0;
        self.guest_frames += 1;

        self.stats.last_ms = elapsed;
        if elapsed > FRAME_BUDGET_MS {
            self.stats.slow_streak += 1;
        } else {
            self.stats.slow_streak = 0;
        }
        self.stats.throttled = self.stats.slow_streak >= 3;

        let mut out: sdk::FrameOutput = postcard::from_bytes(&out_bytes)?;
        self.dirty = false;

        // Until Core has handed over the real document, the guest is drawing a
        // placeholder. A write from that state would replace a loaded board
        // with an empty one, so it is dropped here rather than trusted.
        if !self.received_doc {
            out.doc = None;
        }

        self.apply_textures(&ctx, &mut out.textures_delta);
        self.rebuild_cache(offset, &out.primitives);
        self.paint_cached(ui, rect, offset);
        self.cached_cursor = out.cursor;

        // If the guest wrote the document, it now holds that version, and the
        // same JSON coming back from Core must not be pushed at it again.
        if let Some(doc) = &out.doc {
            self.last_doc = Some(doc.clone());
        }

        if let Some(cursor) = map_cursor(out.cursor) {
            ctx.set_cursor_icon(cursor);
        }

        // egui's own convention: zero means "again, now"; `Duration::MAX` means
        // "nothing pending". A requested repaint is one of the three reasons the
        // guest runs, so it marks the runner dirty for that frame.
        match out.repaint_after_ms {
            0 => {
                ctx.request_repaint();
                self.dirty = true;
            }
            ms if ms < 60_000 => {
                ctx.request_repaint_after(std::time::Duration::from_millis(ms));
                self.dirty = true;
            }
            _ => {}
        }

        Ok(FramePaint {
            primitives: out.primitives,
            repaint_after_ms: out.repaint_after_ms,
            doc: out.doc,
            messages: out.messages,
            cursor: out.cursor,
        })
    }

    /// Translate the host's input into the surface's own coordinate space.
    fn raw_input(
        &self,
        ctx: &egui::Context,
        rect: Rect,
        hovered: bool,
        focused: bool,
    ) -> egui::RawInput {
        let offset = rect.min.to_vec2();
        let (events, time) = ctx.input(|i| (i.events.clone(), i.time));

        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::pos2(0.0, 0.0), rect.size())),
            time: Some(time),
            events: filter_events(events, offset, rect, hovered, focused),
            focused,
            ..Default::default()
        }
    }
}

/// Decide which host events the surface may see.
///
/// Pointer events go through while the pointer is over the panel. Keyboard and
/// text go through only while the panel holds keyboard focus — which it gets by
/// being clicked, never by being hovered or by the window changing state.
pub fn filter_events(
    events: Vec<egui::Event>,
    offset: Vec2,
    rect: Rect,
    hovered: bool,
    focused: bool,
) -> Vec<egui::Event> {
    use egui::Event;
    events
        .into_iter()
        .filter(|e| match e {
            Event::Key { .. } | Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut => {
                focused
            }
            _ => hovered,
        })
        .filter_map(|e| translate_event(e, offset, rect))
        .collect()
}

impl SurfaceRunner {

    /// Upload the surface's textures under host-owned ids.
    fn apply_textures(
        &mut self,
        ctx: &egui::Context,
        delta: &mut epaint::textures::TexturesDelta,
    ) {
        for (guest_id, deltas) in delta.set.iter() {
            for d in deltas.iter() {
                let image = to_color_image(&d.image);
                match (d.pos, self.textures.get_mut(guest_id)) {
                    (Some(pos), Some(handle)) => handle.set_partial(pos, image, d.options),
                    _ => {
                        let name = format!("{}::{guest_id:?}", self.name);
                        let handle = ctx.load_texture(name, image, d.options);
                        self.textures.insert(*guest_id, handle);
                    }
                }
            }
        }
        for id in &delta.free {
            self.textures.remove(id);
        }
        // epaint panics on drop while deltas are unapplied. They are applied now.
        delta.clear();
    }

    /// Translate the guest's meshes into the panel and remap their texture ids,
    /// once per guest frame rather than once per host frame.
    fn rebuild_cache(&mut self, offset: Vec2, prims: &[sdk::WirePrimitive]) {
        self.cached.clear();
        self.cached_offset = offset;
        for prim in prims {
            let Some(host_id) = self.textures.get(&prim.mesh.texture_id).map(|h| h.id()) else {
                continue;
            };
            let mut mesh = prim.mesh.clone();
            mesh.texture_id = host_id;
            mesh.translate(offset);
            self.cached
                .push((prim.clip.translate(offset), std::sync::Arc::new(mesh)));
        }
    }

    /// Paint the cached meshes. If the panel moved since they were built — a
    /// resize, a dock change — they are shifted once and cached again.
    fn paint_cached(&mut self, ui: &mut egui::Ui, rect: Rect, offset: Vec2) {
        if offset != self.cached_offset {
            let shift = offset - self.cached_offset;
            for (clip, mesh) in &mut self.cached {
                *clip = clip.translate(shift);
                let mut moved = (**mesh).clone();
                moved.translate(shift);
                *mesh = std::sync::Arc::new(moved);
            }
            self.cached_offset = offset;
        }
        let painter = ui.painter();
        for (clip, mesh) in &self.cached {
            let clip = clip.intersect(rect);
            if !clip.is_positive() {
                continue;
            }
            painter
                .clone()
                .with_clip_rect(clip)
                .add(egui::Shape::Mesh(mesh.clone()));
        }
    }
}

fn translate_event(e: egui::Event, offset: Vec2, rect: Rect) -> Option<egui::Event> {
    use egui::Event;
    Some(match e {
        Event::PointerMoved(p) => {
            if !rect.contains(p) {
                return None;
            }
            Event::PointerMoved(p - offset)
        }
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        } => {
            if !rect.contains(pos) {
                return None;
            }
            Event::PointerButton {
                pos: pos - offset,
                button,
                pressed,
                modifiers,
            }
        }
        Event::MouseMoved(d) => Event::MouseMoved(d),
        other => other,
    })
}

fn to_color_image(data: &epaint::ImageData) -> epaint::ColorImage {
    match data {
        // Including the surface's own font atlas: epaint hands it over already
        // expanded to RGBA, so the host uploads it like any other texture.
        epaint::ImageData::Color(image) => (**image).clone(),
    }
}

fn map_cursor(c: sdk::CursorIcon) -> Option<egui::CursorIcon> {
    Some(match c {
        sdk::CursorIcon::Default => return None,
        sdk::CursorIcon::Pointer => egui::CursorIcon::PointingHand,
        sdk::CursorIcon::Text => egui::CursorIcon::Text,
        sdk::CursorIcon::Grab => egui::CursorIcon::Grab,
        sdk::CursorIcon::Grabbing => egui::CursorIcon::Grabbing,
        sdk::CursorIcon::ResizeHorizontal => egui::CursorIcon::ResizeHorizontal,
        sdk::CursorIcon::ResizeVertical => egui::CursorIcon::ResizeVertical,
        sdk::CursorIcon::Crosshair => egui::CursorIcon::Crosshair,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn pointer_at(x: f32, y: f32) -> egui::Event {
        egui::Event::PointerMoved(egui::pos2(x, y))
    }

    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(400.0, 300.0))
    }

    #[test]
    fn keyboard_needs_focus_not_hover() {
        // The bug this guards: a window being focused or maximised must not be
        // able to deliver a Delete to the board. Hover is not consent.
        let events = vec![key(egui::Key::Delete), key(egui::Key::A), egui::Event::Text("x".into())];
        let hovered_only = filter_events(events.clone(), Vec2::ZERO, rect(), true, false);
        assert!(hovered_only.is_empty(), "keyboard leaked on hover: {hovered_only:?}");

        let focused = filter_events(events, Vec2::ZERO, rect(), true, true);
        assert_eq!(focused.len(), 3, "keyboard must flow once the panel is focused");
    }

    #[test]
    fn pointer_needs_hover_and_is_translated_into_the_panel() {
        let offset = rect().min.to_vec2();
        let inside = filter_events(vec![pointer_at(150.0, 150.0)], offset, rect(), true, false);
        match inside.as_slice() {
            [egui::Event::PointerMoved(p)] => assert_eq!(*p, egui::pos2(50.0, 50.0)),
            other => panic!("expected one translated move, got {other:?}"),
        }

        // Outside the panel, or not hovered at all: nothing reaches the guest.
        assert!(filter_events(vec![pointer_at(10.0, 10.0)], offset, rect(), true, false).is_empty());
        assert!(filter_events(vec![pointer_at(150.0, 150.0)], offset, rect(), false, false).is_empty());
    }
}
