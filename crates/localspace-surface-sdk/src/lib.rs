#![deny(unsafe_code)]
//! What harness authors compile a `kind = "egui"` surface against.
//!
//! A surface is a plain `wasm32-unknown-unknown` module — not a WASI component,
//! because browsers cannot run components without a transpile step and a surface
//! needs no IO at all. It has no filesystem, no network and no model access; it
//! exchanges a document and opaque messages with its own harness logic, through
//! Core.
//!
//! # The ABI
//!
//! ```text
//! hs_alloc(len: i32) -> i32                  // scratch buffer for the host to write into
//! hs_init(ptr: i32, len: i32)                // postcard(SurfaceInit)
//! hs_frame(ptr: i32, len: i32) -> i64        // postcard(FrameInput) -> (ptr << 32) | len
//! hs_release()                               // the host has read this frame's textures
//! ```
//!
//! `hs_frame` returns one `i64` rather than two values so the ABI needs no
//! multi-value support and is identical under `wasmtime` and the browser's own
//! `WebAssembly` API.
//!
//! # Why meshes and not shapes
//!
//! `epaint::Shape` is not serializable (a text shape holds an `Arc<Galley>` full
//! of font-atlas state). The surface therefore tessellates with its own `egui`
//! and hands the host `epaint::Mesh` — vertices, indices and a texture id, all of
//! which do serialize — plus its textures, whose pixels stay in the surface's
//! memory and are read from there by the host. The host uploads those textures
//! under namespaced ids and draws the meshes into the panel rect. This is the
//! whole trick that makes sandboxed surfaces possible.

use serde::{Deserialize, Serialize};

/// Bumped whenever anything below changes shape. Must match `proto::SHAPE_SCHEMA`.
pub const SHAPE_SCHEMA: u32 = 2;

pub use egui;
pub use epaint;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceInit {
    pub shape_schema: u32,
    pub pixels_per_point: f32,
    pub dark_mode: bool,
    /// The harness document at open time, as JSON.
    pub doc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInput {
    pub raw_input: egui::RawInput,
    /// Present when the document changed since the last frame.
    pub doc: Option<String>,
    /// Messages from the harness logic (opaque to Core).
    #[serde(default)]
    pub messages: Vec<Vec<u8>>,
}

/// One clipped mesh: what the host actually paints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WirePrimitive {
    pub clip: epaint::Rect,
    pub mesh: epaint::Mesh,
}

/// One texture the surface set or patched this frame. The pixels are not in
/// the frame: they stay in the surface's own memory, at `pixels`, until its
/// next call, and the host reads them from there. A font atlas is the largest
/// thing a surface ever hands over, and this is what keeps it from being
/// copied twice more on the way — once by the serialiser and once by the
/// host's decoder — inside a linear memory that never shrinks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireTexture {
    pub id: epaint::TextureId,
    /// `None` sets the whole texture; `Some` patches a region at that position.
    pub pos: Option<[usize; 2]>,
    /// Width and height of the image at `pixels`.
    pub size: [usize; 2],
    pub options: epaint::textures::TextureOptions,
    /// Offset and length, in bytes, of the premultiplied RGBA pixels in the
    /// surface's memory. Valid until the surface's next export is called.
    pub pixels: (u32, u32),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WireTextures {
    /// In order: a full set of a texture precedes the patches to it.
    pub set: Vec<WireTexture>,
    pub free: Vec<epaint::TextureId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameOutput {
    pub primitives: Vec<WirePrimitive>,
    pub textures: WireTextures,
    /// 0 means "no repaint needed until something happens".
    pub repaint_after_ms: u64,
    /// Present when the surface changed the document.
    pub doc: Option<String>,
    /// Commands for the harness logic.
    #[serde(default)]
    pub messages: Vec<Vec<u8>>,
    pub cursor: CursorIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    ResizeHorizontal,
    ResizeVertical,
    Crosshair,
}

impl From<egui::CursorIcon> for CursorIcon {
    fn from(c: egui::CursorIcon) -> Self {
        match c {
            egui::CursorIcon::PointingHand => CursorIcon::Pointer,
            egui::CursorIcon::Text => CursorIcon::Text,
            egui::CursorIcon::Grab => CursorIcon::Grab,
            egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
            egui::CursorIcon::ResizeHorizontal => CursorIcon::ResizeHorizontal,
            egui::CursorIcon::ResizeVertical => CursorIcon::ResizeVertical,
            egui::CursorIcon::Crosshair => CursorIcon::Crosshair,
            _ => CursorIcon::Default,
        }
    }
}

// ---------------------------------------------------------------------------
// Author-facing surface trait
// ---------------------------------------------------------------------------

/// What a surface sees each frame besides the egui context.
pub struct SurfaceState {
    /// The harness document. Mutate it and set `doc_dirty` to push the change
    /// back through Core, which reconciles it into the CRDT and commits.
    pub doc: serde_json::Value,
    pub doc_dirty: bool,
    /// Messages that arrived from the harness logic this frame.
    pub inbox: Vec<Vec<u8>>,
    outbox: Vec<Vec<u8>>,
}

impl SurfaceState {
    pub fn new() -> SurfaceState {
        SurfaceState {
            doc: serde_json::Value::Object(Default::default()),
            doc_dirty: false,
            inbox: Vec::new(),
            outbox: Vec::new(),
        }
    }

    /// Send a command to the harness logic. Design a harness so the document
    /// carries state and messages carry only commands: that is what keeps it
    /// correct when the logic is on the other side of a network.
    pub fn send(&mut self, message: Vec<u8>) {
        if message.len() <= 64 * 1024 {
            self.outbox.push(message);
        }
    }

    pub fn send_json(&mut self, value: &serde_json::Value) {
        self.send(value.to_string().into_bytes());
    }

    pub fn take_outbox(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbox)
    }

    pub fn mark_dirty(&mut self) {
        self.doc_dirty = true;
    }
}

impl Default for SurfaceState {
    fn default() -> Self {
        SurfaceState::new()
    }
}

pub trait Surface: Default {
    /// Called once, before the first frame.
    fn init(&mut self, _cfg: &SurfaceInit) {}
    /// Called each frame the panel is visible and something changed.
    ///
    /// `ui` is the surface's own root, sized to the panel the host gave it.
    fn ui(&mut self, ui: &mut egui::Ui, state: &mut SurfaceState);
}

// ---------------------------------------------------------------------------
// The generated entry points
// ---------------------------------------------------------------------------

/// Runtime shared by every surface. The macro below wires the exports to it.
pub struct SurfaceHost<S: Surface> {
    pub surface: S,
    pub ctx: egui::Context,
    pub state: SurfaceState,
    /// Kept alive until the next call, because the host reads it after `hs_frame`
    /// returns.
    pub out: Vec<u8>,
    /// This frame's texture changes, kept alive until the host says it has
    /// read them — or until the next frame, whichever comes first — because
    /// the frame points at their pixels rather than carrying them. A full font
    /// atlas is 4 MB; released promptly, it is not still occupying the heap
    /// when the atlas next grows.
    pub held: Vec<epaint::ImageDelta>,
}

impl<S: Surface> Default for SurfaceHost<S> {
    fn default() -> Self {
        SurfaceHost {
            surface: S::default(),
            ctx: egui::Context::default(),
            state: SurfaceState::new(),
            out: Vec::new(),
            held: Vec::new(),
        }
    }
}

impl<S: Surface> SurfaceHost<S> {
    pub fn init(&mut self, bytes: &[u8]) {
        let Ok(cfg) = postcard::from_bytes::<SurfaceInit>(bytes) else {
            return;
        };
        self.ctx.set_pixels_per_point(cfg.pixels_per_point);
        self.ctx.set_visuals(if cfg.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        if let Ok(doc) = serde_json::from_str(&cfg.doc) {
            self.state.doc = doc;
        }
        self.surface.init(&cfg);
    }

    pub fn frame(&mut self, bytes: &[u8]) -> &[u8] {
        let input: FrameInput = match postcard::from_bytes(bytes) {
            Ok(i) => i,
            Err(_) => {
                self.out = Vec::new();
                return &self.out;
            }
        };

        if let Some(doc) = &input.doc {
            if let Ok(parsed) = serde_json::from_str(doc) {
                self.state.doc = parsed;
            }
        }
        self.state.inbox = input.messages;
        self.state.doc_dirty = false;

        let surface = &mut self.surface;
        let state = &mut self.state;
        let mut full = self.ctx.run_ui(input.raw_input, |ui| surface.ui(ui, state));
        let textures = self.take_textures(&mut full.textures_delta);

        let primitives = self
            .ctx
            .tessellate(full.shapes, full.pixels_per_point)
            .into_iter()
            .filter_map(|p| match p.primitive {
                epaint::Primitive::Mesh(mesh) => Some(WirePrimitive {
                    clip: p.clip_rect,
                    mesh,
                }),
                // A paint callback needs the host's own render pass; a sandboxed
                // surface does not get one. Use a `stream` view for that.
                epaint::Primitive::Callback(_) => None,
            })
            .collect();

        let out = FrameOutput {
            primitives,
            textures,
            repaint_after_ms: full
                .viewport_output
                .values()
                .map(|v| v.repaint_delay.as_millis().min(u128::from(u64::MAX)) as u64)
                .min()
                .unwrap_or(0),
            doc: if self.state.doc_dirty {
                Some(self.state.doc.to_string())
            } else {
                None
            },
            messages: self.state.take_outbox(),
            cursor: full.platform_output.cursor_icon.into(),
        };

        // Serialise into last frame's buffer, whose capacity is kept: the steady
        // state then costs no allocation, and in a linear memory that never
        // shrinks a fresh buffer's doubling would set the high-water mark for
        // good.
        let mut buf = std::mem::take(&mut self.out);
        buf.clear();
        self.out = postcard::to_extend(&out, buf).unwrap_or_default();
        &self.out
    }

    /// The host has read this frame's textures out of memory.
    pub fn release(&mut self) {
        self.held.clear();
    }

    /// Move this frame's texture changes out of egui's delta. The images are
    /// held here, alive, until the host has read them, and the wire form points
    /// at their pixels. The delta is left empty, which is what lets it drop.
    fn take_textures(&mut self, delta: &mut epaint::textures::TexturesDelta) -> WireTextures {
        let mut wire = WireTextures {
            set: Vec::new(),
            free: delta.free.drain().collect(),
        };
        self.held.clear();
        for (id, deltas) in delta.set.drain() {
            for d in deltas {
                let image = match &d.image {
                    epaint::ImageData::Color(image) => image,
                };
                let bytes = image.pixels.len() * std::mem::size_of::<epaint::Color32>();
                wire.set.push(WireTexture {
                    id,
                    pos: d.pos,
                    size: image.size,
                    options: d.options,
                    pixels: (image.pixels.as_ptr() as usize as u32, bytes as u32),
                });
                self.held.push(d);
            }
        }
        wire
    }
}

/// Generate the three wasm exports for a `Surface` implementation.
///
/// ```ignore
/// #[derive(Default)]
/// struct Board { /* ... */ }
/// impl localspace_surface_sdk::Surface for Board {
///     fn ui(&mut self, ctx: &egui::Context, state: &mut SurfaceState) { /* ... */ }
/// }
/// localspace_surface_sdk::export_surface!(Board);
/// ```
#[macro_export]
macro_rules! export_surface {
    ($ty:ty) => {
        static mut HS_SCRATCH: ::std::vec::Vec<u8> = ::std::vec::Vec::new();
        static mut HS_HOST: ::std::option::Option<$crate::SurfaceHost<$ty>> = ::std::option::Option::None;

        #[allow(static_mut_refs, unsafe_code)]
        fn hs_host() -> &'static mut $crate::SurfaceHost<$ty> {
            // SAFETY: a wasm surface module is single-threaded and the host
            // calls its exports one at a time, so this static is never
            // aliased; it is initialised on first use and lives forever.
            unsafe {
                if HS_HOST.is_none() {
                    HS_HOST = ::std::option::Option::Some($crate::SurfaceHost::default());
                }
                HS_HOST.as_mut().unwrap()
            }
        }

        /// Scratch buffer the host writes the next input into.
        #[no_mangle]
        #[allow(static_mut_refs, unsafe_code)]
        pub extern "C" fn hs_alloc(len: i32) -> i32 {
            // Reused across frames: the capacity stays, so the host's input
            // does not cost a fresh allocation every frame.
            // SAFETY: single-threaded, one export call at a time (see
            // `hs_host`); the pointer handed out stays valid until the next
            // `hs_alloc`, which is the ABI's contract with the host.
            unsafe {
                HS_SCRATCH.clear();
                HS_SCRATCH.resize(len.max(0) as usize, 0);
                HS_SCRATCH.as_ptr() as i32
            }
        }

        #[no_mangle]
        #[allow(unsafe_code)]
        pub extern "C" fn hs_init(ptr: i32, len: i32) {
            // SAFETY: `ptr` and `len` name the bytes the host wrote into the
            // buffer it got from `hs_alloc`, inside this module's own linear
            // memory; the slice lives only for this call.
            let bytes =
                unsafe { ::std::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize) };
            hs_host().init(bytes);
        }

        /// Returns `(ptr << 32) | len` so the ABI needs no multi-value support.
        #[no_mangle]
        #[allow(unsafe_code)]
        pub extern "C" fn hs_frame(ptr: i32, len: i32) -> i64 {
            // SAFETY: as in `hs_init`: the host's input in this module's own
            // memory, read only for the duration of the call.
            let bytes =
                unsafe { ::std::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize) };
            let out = hs_host().frame(bytes);
            ((out.as_ptr() as i64) << 32) | (out.len() as i64 & 0xffff_ffff)
        }

        /// The host has read the textures the last frame pointed at.
        #[no_mangle]
        pub extern "C" fn hs_release() {
            hs_host().release();
        }
    };
}

/// Unpack what `hs_frame` returned.
pub fn unpack(ret: i64) -> (u32, u32) {
    (((ret >> 32) & 0xffff_ffff) as u32, (ret & 0xffff_ffff) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_return_value_packs_and_unpacks() {
        let ptr = 0x0012_3456u32;
        let len = 0x0000_9abcu32;
        let packed = ((ptr as i64) << 32) | (len as i64 & 0xffff_ffff);
        assert_eq!(unpack(packed), (ptr, len));
    }

    #[test]
    fn a_frame_output_survives_postcard() {
        // The whole ABI rests on this: epaint's mesh output is serializable even
        // though its shape output is not.
        let mut mesh = epaint::Mesh::default();
        mesh.add_colored_rect(
            epaint::Rect::from_min_size(epaint::pos2(0.0, 0.0), epaint::vec2(10.0, 4.0)),
            epaint::Color32::RED,
        );
        let out = FrameOutput {
            primitives: vec![WirePrimitive {
                clip: epaint::Rect::EVERYTHING,
                mesh,
            }],
            textures: WireTextures::default(),
            repaint_after_ms: 0,
            doc: Some("{\"a\":1}".into()),
            messages: vec![b"hello".to_vec()],
            cursor: CursorIcon::Grab,
        };
        let bytes = postcard::to_allocvec(&out).unwrap();
        let back: FrameOutput = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.primitives.len(), 1);
        assert_eq!(back.primitives[0].mesh.vertices.len(), 4);
        assert_eq!(back.primitives[0].mesh.indices.len(), 6);
        assert_eq!(back.doc.as_deref(), Some("{\"a\":1}"));
        assert_eq!(back.cursor, CursorIcon::Grab);
    }

    #[derive(Default)]
    struct Blank;
    impl Surface for Blank {
        fn ui(&mut self, _ui: &mut egui::Ui, _state: &mut SurfaceState) {}
    }

    #[test]
    fn textures_are_pointed_at_not_carried_and_held_until_released() {
        // A full font atlas is 4 MB. The frame carries where its pixels are;
        // the image itself stays alive in the surface until the host says it
        // has read it. (The pointer is a wasm32 address: on a 64-bit test host
        // it is truncated, so only its length is checked here.)
        let mut host = SurfaceHost::<Blank>::default();
        let mut delta = epaint::textures::TexturesDelta::default();
        let image = epaint::ColorImage::filled([2, 2], epaint::Color32::WHITE);
        delta.set.insert(
            epaint::TextureId::Managed(0),
            std::iter::once(epaint::ImageDelta::full(
                image,
                epaint::textures::TextureOptions::LINEAR,
            ))
            .collect(),
        );
        delta.free.insert(epaint::TextureId::Managed(7));

        let wire = host.take_textures(&mut delta);
        assert!(delta.is_empty(), "the delta must be left empty so it can drop");
        assert_eq!(wire.set.len(), 1);
        assert_eq!(wire.set[0].id, epaint::TextureId::Managed(0));
        assert_eq!(wire.set[0].size, [2, 2]);
        assert_eq!(wire.set[0].pixels.1, 2 * 2 * 4, "four bytes a pixel");
        assert_eq!(wire.free, vec![epaint::TextureId::Managed(7)]);
        assert_eq!(host.held.len(), 1, "the image is held for the host to read");

        let bytes = postcard::to_allocvec(&wire).unwrap();
        let back: WireTextures = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.set[0].pixels, wire.set[0].pixels);

        host.release();
        assert!(host.held.is_empty(), "released images are dropped at once");
    }

    #[test]
    fn frame_input_carries_real_egui_input() {
        let input = FrameInput {
            raw_input: egui::RawInput {
                screen_rect: Some(epaint::Rect::from_min_size(
                    epaint::pos2(0.0, 0.0),
                    epaint::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            doc: None,
            messages: Vec::new(),
        };
        let bytes = postcard::to_allocvec(&input).unwrap();
        let back: FrameInput = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(
            back.raw_input.screen_rect.unwrap().width(),
            800.0
        );
    }

    #[test]
    fn messages_over_the_size_limit_are_refused() {
        let mut s = SurfaceState::new();
        s.send(vec![0u8; 64 * 1024]);
        s.send(vec![0u8; 64 * 1024 + 1]);
        assert_eq!(s.take_outbox().len(), 1);
    }
}
