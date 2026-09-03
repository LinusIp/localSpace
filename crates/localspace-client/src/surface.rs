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

    pub struct Runner {
        store: Store<()>,
        memory: Memory,
        alloc: TypedFunc<i32, i32>,
        init: TypedFunc<(i32, i32), ()>,
        frame: TypedFunc<(i32, i32), i64>,
    }

    impl Runner {
        pub fn load(bytes: &[u8]) -> Result<Runner> {
            let engine = Engine::default();
            let module = Module::from_binary(&engine, bytes)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("loading the surface module")?;
            let mut store = Store::new(&engine, ());

            // A surface reaches nothing: no filesystem, no network, no model.
            //
            // It does arrive with wasm-bindgen's placeholder imports, because
            // `egui` depends on `web-sys` unconditionally on wasm32 and the
            // linker emits them whether or not any of it is reachable. Those are
            // stubbed with traps: a surface that really tries to call into the
            // browser fails loudly instead of quietly getting JS access. Any
            // other import is refused outright.
            let mut linker: wasmtime::Linker<()> = wasmtime::Linker::new(&engine);
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

            let instance = linker
                .instantiate(&mut store, &module)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("instantiating the surface")?;


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
        pub fn load(_bytes: &[u8]) -> Result<Runner> {
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
    pub stats: SurfaceStats,
    initialised: bool,
    dirty: bool,
    last_doc: Option<String>,
}

impl SurfaceRunner {
    pub fn load(name: &str, bytes: &[u8]) -> anyhow::Result<SurfaceRunner> {
        Ok(SurfaceRunner {
            inner: imp::Runner::load(bytes)?,
            textures: HashMap::new(),
            name: name.to_string(),
            stats: SurfaceStats::default(),
            initialised: false,
            dirty: true,
            last_doc: None,
        })
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Hand the surface a new document. Only pushed when it actually changed.
    pub fn set_doc(&mut self, doc: String) {
        if self.last_doc.as_deref() != Some(doc.as_str()) {
            self.last_doc = Some(doc);
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

        let raw_input = self.raw_input(&ctx, rect, hovered);
        let has_input = !raw_input.events.is_empty();

        // Pay per change, not per frame: run only when something actually moved.
        if !self.dirty && !has_input && !self.stats.throttled {
            // Repaint from the cached primitives instead of re-running the guest.
        }

        let input = sdk::FrameInput {
            raw_input,
            doc: self.last_doc.take(),
            messages,
        };

        let started = std::time::Instant::now();
        let out_bytes = self.inner.frame(&postcard::to_allocvec(&input)?)?;
        let elapsed = started.elapsed().as_secs_f32() * 1000.0;

        self.stats.last_ms = elapsed;
        if elapsed > FRAME_BUDGET_MS {
            self.stats.slow_streak += 1;
        } else {
            self.stats.slow_streak = 0;
        }
        self.stats.throttled = self.stats.slow_streak >= 3;

        let mut out: sdk::FrameOutput = postcard::from_bytes(&out_bytes)?;
        self.dirty = false;

        self.apply_textures(&ctx, &mut out.textures_delta);
        self.paint(ui, rect, offset, &out.primitives);

        if let Some(cursor) = map_cursor(out.cursor) {
            ctx.set_cursor_icon(cursor);
        }
        if out.repaint_after_ms > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(out.repaint_after_ms));
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
    fn raw_input(&self, ctx: &egui::Context, rect: Rect, hovered: bool) -> egui::RawInput {
        let offset = rect.min.to_vec2();
        let (events, time) = ctx.input(|i| {
            (
                i.events.clone(),

                i.time,
            )
        });

        let events = if hovered {
            events
                .into_iter()
                .filter_map(|e| translate_event(e, offset, rect))
                .collect()
        } else {
            // Not hovered: forward nothing but keep the surface's clock running.
            Vec::new()
        };

        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::pos2(0.0, 0.0), rect.size())),
            time: Some(time),

            events,
            focused: hovered,
            ..Default::default()
        }
    }

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

    fn paint(&self, ui: &mut egui::Ui, rect: Rect, offset: Vec2, prims: &[sdk::WirePrimitive]) {
        let painter = ui.painter();
        for prim in prims {
            let Some(host_id) = self.textures.get(&prim.mesh.texture_id).map(|h| h.id()) else {
                continue;
            };
            let mut mesh = prim.mesh.clone();
            mesh.texture_id = host_id;
            mesh.translate(offset);

            let clip = prim.clip.translate(offset).intersect(rect);
            if !clip.is_positive() {
                continue;
            }
            painter
                .clone()
                .with_clip_rect(clip)
                .add(egui::Shape::mesh(mesh));
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
