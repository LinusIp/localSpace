//! Reference harness #1 — the whiteboard's `kind = "egui"` surface.
//!
//! An infinite dot-grid canvas with the editing a board actually needs: a tool
//! palette, marquee and shift multi-select, dragging a whole selection, eight
//! resize handles, snapping with alignment guides, stacking order, locking,
//! duplicate, clipboard, freehand ink, text labels and keyboard shortcuts.
//!
//! Two rules shape the code:
//!
//! * **The document is the state.** Every edit mutates `state.doc`, and Core
//!   reconciles that into the CRDT. There is no second model to keep in step.
//! * **One gesture, one commit.** A drag mutates the document locally for the
//!   live preview but only sets `doc_dirty` when the gesture ends, so dragging a
//!   sticky across the board is one entry in the history rather than sixty.
//!
//! It has no filesystem, no network and no model access.

use localspace_surface_sdk::egui::{self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, Vec2};
use localspace_surface_sdk::{Surface, SurfaceInit, SurfaceState};
use serde_json::{json, Value};

const PAGE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const DOT: Color32 = Color32::from_rgb(0xE3, 0xE6, 0xE6);
const TEXT: Color32 = Color32::from_rgb(0x1F, 0x23, 0x28);
const MUTED: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const FAINT: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);
const LINE: Color32 = Color32::from_rgb(0xB6, 0xBC, 0xC2);
const ACCENT: Color32 = Color32::from_rgb(0x16, 0xA3, 0x4A);
const GUIDE: Color32 = Color32::from_rgb(0xE0, 0x4A, 0x9F);
const SURFACE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const BORDER: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xE8);

/// How close, in screen pixels, two edges must be before they snap.
const SNAP: f32 = 6.0;
const HANDLE: f32 = 7.0;

const COLOURS: [&str; 6] = ["red", "amber", "green", "blue", "yellow", "grey"];

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Sticky,
    Rect,
    Ellipse,
    Text,
    Pen,
    Connect,
}

impl Tool {
    fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Sticky => "Sticky",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Text => "Text",
            Tool::Pen => "Pen",
            Tool::Connect => "Connector",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    NW,
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
}

impl Handle {
    const ALL: [Handle; 8] = [
        Handle::NW,
        Handle::N,
        Handle::NE,
        Handle::E,
        Handle::SE,
        Handle::S,
        Handle::SW,
        Handle::W,
    ];

    /// Position of this handle on a rect, in unit coordinates.
    fn anchor(self) -> (f32, f32) {
        match self {
            Handle::NW => (0.0, 0.0),
            Handle::N => (0.5, 0.0),
            Handle::NE => (1.0, 0.0),
            Handle::E => (1.0, 0.5),
            Handle::SE => (1.0, 1.0),
            Handle::S => (0.5, 1.0),
            Handle::SW => (0.0, 1.0),
            Handle::W => (0.0, 0.5),
        }
    }

    fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
            Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
            _ => egui::CursorIcon::Grab,
        }
    }
}

enum Gesture {
    None,
    Pan,
    Marquee { from: Pos2 },
    /// Board-space offset of each moving shape from the pointer.
    Move { grabs: Vec<(String, Vec2)> },
    Resize { id: String, handle: Handle },
    Ink { points: Vec<[f64; 2]> },
    Connect { from: String },
}

#[derive(Default)]
struct Board {
    pan: Vec2,
    zoom: f32,
    tool: Tool,
    gesture: Gesture,
    clipboard: Vec<Value>,
    editing: Option<String>,
    edit_buffer: String,
    /// Alignment guides to draw this frame, in board space.
    guides: Vec<(bool, f64, f64, f64)>,
    seq: u64,
    ready: bool,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Select
    }
}

impl Default for Gesture {
    fn default() -> Self {
        Gesture::None
    }
}

impl Surface for Board {
    fn init(&mut self, _cfg: &SurfaceInit) {
        self.zoom = 1.0;
        self.ready = true;
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut SurfaceState) {
        if !self.ready {
            self.zoom = 1.0;
            self.ready = true;
        }
        self.take_commands(state);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAGE))
            .show(ui, |ui| self.canvas(ui, state));
    }
}

// ---------------------------------------------------------------------------
// Document access
// ---------------------------------------------------------------------------

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn is_locked(sh: &Value) -> bool {
    sh["locked"].as_bool().unwrap_or(false)
}

fn is_connector(sh: &Value) -> bool {
    sh["kind"].as_str() == Some("arrow")
}

fn is_ink(sh: &Value) -> bool {
    sh["kind"].as_str() == Some("ink")
}

fn shapes_of(doc: &Value) -> Vec<Value> {
    let mut list = doc["shapes"].as_array().cloned().unwrap_or_default();
    // Stacking order. A stable sort keeps insertion order among equal z.
    list.sort_by_key(|s| s["z"].as_i64().unwrap_or(0));
    list
}

fn selection_of(doc: &Value) -> Vec<String> {
    doc["selection"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn board_rect(sh: &Value) -> Rect {
    if is_ink(sh) {
        if let Some(points) = sh["points"].as_array() {
            let xs: Vec<f64> = points.iter().filter_map(|p| p[0].as_f64()).collect();
            let ys: Vec<f64> = points.iter().filter_map(|p| p[1].as_f64()).collect();
            if !xs.is_empty() {
                let (min_x, max_x) = (
                    xs.iter().cloned().fold(f64::MAX, f64::min),
                    xs.iter().cloned().fold(f64::MIN, f64::max),
                );
                let (min_y, max_y) = (
                    ys.iter().cloned().fold(f64::MAX, f64::min),
                    ys.iter().cloned().fold(f64::MIN, f64::max),
                );
                return Rect::from_min_max(
                    egui::pos2(min_x as f32, min_y as f32),
                    egui::pos2(max_x as f32, max_y as f32),
                );
            }
        }
    }
    Rect::from_min_size(
        egui::pos2(num(sh, "x") as f32, num(sh, "y") as f32),
        egui::vec2(
            (num(sh, "w") as f32).max(24.0),
            (num(sh, "h") as f32).max(18.0),
        ),
    )
}

fn find<'a>(shapes: &'a [Value], id: &str) -> Option<&'a Value> {
    shapes.iter().find(|s| s["id"].as_str() == Some(id))
}

/// Edit one shape in the document by id.
fn with_shape(doc: &mut Value, id: &str, mut edit: impl FnMut(&mut Value)) {
    if let Some(list) = doc["shapes"].as_array_mut() {
        if let Some(sh) = list.iter_mut().find(|s| s["id"].as_str() == Some(id)) {
            edit(sh);
        }
    }
}

fn next_z(doc: &Value) -> i64 {
    doc["shapes"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s["z"].as_i64()).max().unwrap_or(0) + 1)
        .unwrap_or(1)
}

fn palette(name: &str) -> (Color32, Color32) {
    match name {
        "red" => (
            Color32::from_rgb(0xE0, 0x6C, 0x6C),
            Color32::from_rgb(0xFE, 0xF2, 0xF2),
        ),
        "amber" => (
            Color32::from_rgb(0xE0, 0xA0, 0x30),
            Color32::from_rgb(0xFF, 0xF8, 0xEC),
        ),
        "green" => (
            Color32::from_rgb(0x3F, 0xB1, 0x70),
            Color32::from_rgb(0xEC, 0xFD, 0xF3),
        ),
        "blue" => (
            Color32::from_rgb(0x5B, 0x8F, 0xEE),
            Color32::from_rgb(0xEF, 0xF6, 0xFF),
        ),
        "yellow" => (
            Color32::from_rgb(0xD9, 0xBE, 0x3A),
            Color32::from_rgb(0xFE, 0xFB, 0xE9),
        ),
        _ => (
            Color32::from_rgb(0xC4, 0xC9, 0xCE),
            Color32::from_rgb(0xF5, 0xF6, 0xF7),
        ),
    }
}

impl Board {
    fn new_id(&mut self, time: f64) -> String {
        self.seq += 1;
        format!("u{}_{}", self.seq, (time * 1000.0) as u64 % 1_000_000)
    }

    /// Commands the Client sends straight into the surface: the zoom controls and
    /// Fit in the top bar act on this viewport, not on the document.
    fn take_commands(&mut self, state: &mut SurfaceState) {
        for message in std::mem::take(&mut state.inbox) {
            let Ok(text) = String::from_utf8(message) else {
                continue;
            };
            let Ok(msg) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            match msg.get("kind").and_then(|k| k.as_str()) {
                Some("zoom") => {
                    if let Some(v) = msg.get("value").and_then(|v| v.as_f64()) {
                        self.zoom = (v as f32).clamp(0.25, 4.0);
                    }
                }
                Some("fit") => {
                    self.zoom = 1.0;
                    self.fit(state);
                }
                _ => {}
            }
        }
    }

    fn fit(&mut self, state: &SurfaceState) {
        let shapes = shapes_of(&state.doc);
        if shapes.is_empty() {
            self.pan = Vec2::ZERO;
            return;
        }
        let mut bounds = board_rect(&shapes[0]);
        for sh in &shapes[1..] {
            bounds = bounds.union(board_rect(sh));
        }
        self.pan = Vec2::new(40.0 - bounds.min.x, 40.0 - bounds.min.y);
    }
}

/// A text size under zoom, snapped to one of twelve sizes per doubling: a step
/// of about six per cent, which the eye does not catch. Every distinct size is
/// a fresh set of glyphs rasterised into the font atlas, and a smooth zoom
/// would otherwise mint a new size every frame until the atlas — and the
/// surface's memory with it — was rebuilt over and over.
fn pt(size: f32) -> f32 {
    let steps = (size.max(1.0).log2() * 12.0).round();
    (steps / 12.0).exp2()
}

// ---------------------------------------------------------------------------
// The canvas
// ---------------------------------------------------------------------------

impl Board {
    fn canvas(&mut self, ui: &mut egui::Ui, state: &mut SurfaceState) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        self.dots(&painter, rect);

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.0015)).clamp(0.25, 4.0);
            }
        }

        let z = self.zoom;
        let pan = self.pan;
        let to_screen = move |p: Pos2| -> Pos2 { rect.min + (p.to_vec2() + pan) * z };
        let to_board = move |p: Pos2| -> Pos2 { ((p - rect.min) / z - pan).to_pos2() };

        let shapes = shapes_of(&state.doc);
        let selection = selection_of(&state.doc);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let modifiers = ui.input(|i| i.modifiers);
        let time = ui.input(|i| i.time);

        self.draw_frames(&painter, &state.doc, &to_screen);
        self.draw_connectors(&painter, &shapes, &to_screen);
        self.draw_shapes(&painter, &shapes, &selection, &to_screen);

        self.handle_keyboard(ui, state, &shapes, &selection, time);
        self.handle_pointer(
            ui,
            state,
            &response,
            &shapes,
            &selection,
            pointer,
            modifiers,
            time,
            &to_screen,
            &to_board,
        );

        self.draw_guides(&painter, &to_screen, rect);
        self.draw_selection_chrome(ui, &painter, state, &to_screen);
        self.draw_ink_preview(&painter, &to_screen);
        self.draw_marquee(&painter, pointer);

        self.tool_palette(ui, rect);
        self.text_editor(ui, state);
        self.hint(&painter, rect, &shapes, &selection);
    }

    fn dots(&self, painter: &egui::Painter, rect: Rect) {
        let step = 22.0 * self.zoom;
        if step < 10.0 {
            return;
        }
        let offset = Vec2::new(
            (self.pan.x * self.zoom) % step,
            (self.pan.y * self.zoom) % step,
        );
        // One mesh of tiny quads, not a tessellated circle per dot. At 100 %
        // a laptop screen holds about 2,600 dots, and a feathered circle each
        // was the single largest thing a frame produced.
        let mut mesh = egui::Mesh::default();
        let mut y = rect.top() + offset.y;
        while y < rect.bottom() {
            let mut x = rect.left() + offset.x;
            while x < rect.right() {
                mesh.add_colored_rect(
                    Rect::from_center_size(egui::pos2(x, y), Vec2::splat(2.0)),
                    DOT,
                );
                x += step;
            }
            y += step;
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

impl Board {
    fn draw_frames(&self, painter: &egui::Painter, doc: &Value, to_screen: &impl Fn(Pos2) -> Pos2) {
        for f in doc["frames"].as_array().cloned().unwrap_or_default() {
            let r = Rect::from_min_size(
                to_screen(egui::pos2(num(&f, "x") as f32, num(&f, "y") as f32)),
                egui::vec2(num(&f, "w") as f32, num(&f, "h") as f32) * self.zoom,
            );
            // The title sits above the frame; a frame wholly off screen costs nothing.
            if !painter.clip_rect().intersects(r.expand(24.0)) {
                continue;
            }
            painter.rect_stroke(
                r,
                CornerRadius::same(10),
                Stroke::new(1.0, DOT),
                egui::StrokeKind::Outside,
            );
            painter.text(
                r.min + egui::vec2(10.0, -16.0),
                egui::Align2::LEFT_TOP,
                f["name"].as_str().unwrap_or("frame"),
                egui::FontId::proportional(pt(11.0 * self.zoom.clamp(0.7, 1.4))),
                FAINT,
            );
        }
    }

    fn draw_connectors(
        &self,
        painter: &egui::Painter,
        shapes: &[Value],
        to_screen: &impl Fn(Pos2) -> Pos2,
    ) {
        let z = self.zoom;
        for sh in shapes.iter().filter(|s| is_connector(s)) {
            let (Some(from), Some(to)) = (sh["from"].as_str(), sh["to"].as_str()) else {
                continue;
            };
            let (Some(a), Some(b)) = (find(shapes, from), find(shapes, to)) else {
                continue;
            };
            let ar = screen_rect(a, z, to_screen);
            let br = screen_rect(b, z, to_screen);
            if !painter.clip_rect().intersects(ar.union(br).expand(16.0)) {
                continue;
            }
            let start = edge_point(ar, br.center());
            let end = edge_point(br, ar.center());

            painter.circle_filled(start, 2.5 * z.clamp(0.7, 1.3), LINE);
            painter.line_segment([start, end], Stroke::new(1.4, LINE));
            let dir = (end - start).normalized();
            let head = end - dir * 8.0 * z.clamp(0.7, 1.3);
            let side = egui::vec2(-dir.y, dir.x) * 4.0 * z.clamp(0.7, 1.3);
            painter.add(egui::Shape::convex_polygon(
                vec![end, head + side, head - side],
                LINE,
                Stroke::NONE,
            ));

            let label = sh["text"].as_str().unwrap_or("");
            if !label.is_empty() {
                let mid = start + (end - start) * 0.5;
                let galley = painter.layout_no_wrap(
                    label.to_string(),
                    egui::FontId::proportional(pt(10.5 * z.clamp(0.7, 1.3))),
                    MUTED,
                );
                let bg = Rect::from_center_size(mid, galley.size() + egui::vec2(10.0, 5.0));
                painter.rect_filled(bg, CornerRadius::same(9), PAGE);
                painter.rect_stroke(
                    bg,
                    CornerRadius::same(9),
                    Stroke::new(1.0, DOT),
                    egui::StrokeKind::Inside,
                );
                painter.galley(bg.center() - galley.size() * 0.5, galley, MUTED);
            }
        }
    }

    fn draw_shapes(
        &self,
        painter: &egui::Painter,
        shapes: &[Value],
        selection: &[String],
        to_screen: &impl Fn(Pos2) -> Pos2,
    ) {
        let z = self.zoom;
        for sh in shapes {
            if is_connector(sh) {
                continue;
            }
            let id = sh["id"].as_str().unwrap_or("");
            let selected = selection.iter().any(|s| s == id);

            // Only what is on screen is drawn. Immediate mode rebuilds the
            // shape list every frame, so a thousand-note board must cost what
            // its visible part costs, not what the whole document does.
            let bounds = screen_rect(sh, z, to_screen);
            let overflow = if sh["kind"].as_str() == Some("text") {
                // A text shape draws unwrapped and may run past its own width.
                sh["text"].as_str().map_or(0, str::len) as f32
                    * num(sh, "size").max(12.0) as f32
                    * z
            } else {
                0.0
            };
            if !painter
                .clip_rect()
                .intersects(bounds.expand2(egui::vec2(48.0 + overflow, 48.0)))
            {
                continue;
            }

            if is_ink(sh) {
                let (accent, _) = palette(sh["fill"].as_str().unwrap_or("grey"));
                let points: Vec<Pos2> = sh["points"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| {
                                Some(to_screen(egui::pos2(
                                    p.get(0)?.as_f64()? as f32,
                                    p.get(1)?.as_f64()? as f32,
                                )))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if points.len() > 1 {
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new(2.0 * z.clamp(0.5, 2.0), accent),
                    ));
                }
                if selected {
                    painter.rect_stroke(
                        screen_rect(sh, z, to_screen).expand(4.0),
                        CornerRadius::same(6),
                        Stroke::new(1.0, ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
                continue;
            }

            let r = screen_rect(sh, z, to_screen);
            let kind = sh["kind"].as_str().unwrap_or("rect");
            let (accent, tint) = palette(sh["fill"].as_str().unwrap_or("grey"));
            let text = sh["text"].as_str().unwrap_or("");

            if kind == "text" {
                let size = num(sh, "size").max(12.0) as f32;
                painter.text(
                    r.left_top(),
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::proportional(pt(size * z.clamp(0.4, 2.0))),
                    TEXT,
                );
            } else if kind == "ellipse" {
                painter.add(egui::Shape::ellipse_filled(r.center(), r.size() * 0.5, PAGE));
                painter.add(egui::Shape::ellipse_stroke(
                    r.center(),
                    r.size() * 0.5,
                    Stroke::new(1.4, accent),
                ));
            } else {
                painter.rect_filled(r, CornerRadius::same(10), PAGE);
                painter.rect_stroke(
                    r,
                    CornerRadius::same(10),
                    Stroke::new(1.4, accent),
                    egui::StrokeKind::Inside,
                );
            }

            if kind != "text" {
                let pad = 9.0 * z.clamp(0.6, 1.4);
                let chip = Rect::from_min_size(
                    r.min + Vec2::splat(pad),
                    Vec2::splat(15.0 * z.clamp(0.6, 1.4)),
                );
                painter.rect_filled(chip, CornerRadius::same(4), tint);
                painter.rect_filled(
                    chip.shrink(chip.width() * 0.3),
                    CornerRadius::same(1),
                    accent,
                );
                painter.text(
                    egui::pos2(chip.right() + 6.0 * z.clamp(0.6, 1.4), chip.center().y),
                    egui::Align2::LEFT_CENTER,
                    kind.to_uppercase(),
                    egui::FontId::proportional(pt(8.5 * z.clamp(0.7, 1.4))),
                    accent,
                );
                if !text.is_empty() {
                    // The wrap width is snapped to 8 px: egui caches galleys by
                    // their exact layout parameters, and a smooth zoom would
                    // otherwise re-lay out every note's text on every frame.
                    let wrap = ((r.width() - pad * 2.0).max(20.0) / 8.0).round() * 8.0;
                    let galley = painter.layout(
                        text.to_string(),
                        egui::FontId::proportional(pt(12.5 * z.clamp(0.6, 1.5))),
                        TEXT,
                        wrap,
                    );
                    painter.galley(
                        egui::pos2(r.left() + pad, chip.bottom() + 7.0 * z.clamp(0.6, 1.4)),
                        galley,
                        TEXT,
                    );
                }
            }

            if is_locked(sh) {
                // A small closed padlock in the corner, so "why will this not move"
                // is answerable by looking.
                let c = r.right_top() + egui::vec2(-10.0, 10.0);
                painter.rect_filled(
                    Rect::from_center_size(c + egui::vec2(0.0, 2.0), egui::vec2(9.0, 7.0)),
                    CornerRadius::same(2),
                    FAINT,
                );
                painter.add(egui::Shape::Path(egui::epaint::PathShape {
                    points: (0..=8)
                        .map(|i| {
                            let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
                            c + egui::vec2(a.cos(), a.sin()) * 3.2 + egui::vec2(0.0, -2.0)
                        })
                        .collect(),
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::new(1.4, FAINT).into(),
                }));
            }

            if selected {
                painter.rect_stroke(
                    r.expand(2.0),
                    CornerRadius::same(10),
                    Stroke::new(1.5, ACCENT),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }

    fn draw_guides(
        &self,
        painter: &egui::Painter,
        to_screen: &impl Fn(Pos2) -> Pos2,
        clip: Rect,
    ) {
        for (vertical, at, from, to) in &self.guides {
            let stroke = Stroke::new(1.0, GUIDE);
            if *vertical {
                let a = to_screen(egui::pos2(*at as f32, *from as f32));
                let b = to_screen(egui::pos2(*at as f32, *to as f32));
                painter.line_segment(
                    [
                        egui::pos2(a.x, a.y.max(clip.top())),
                        egui::pos2(b.x, b.y.min(clip.bottom())),
                    ],
                    stroke,
                );
            } else {
                let a = to_screen(egui::pos2(*from as f32, *at as f32));
                let b = to_screen(egui::pos2(*to as f32, *at as f32));
                painter.line_segment(
                    [
                        egui::pos2(a.x.max(clip.left()), a.y),
                        egui::pos2(b.x.min(clip.right()), b.y),
                    ],
                    stroke,
                );
            }
        }
    }

    fn draw_marquee(&self, painter: &egui::Painter, pointer: Option<Pos2>) {
        if let (Gesture::Marquee { from }, Some(to)) = (&self.gesture, pointer) {
            let r = Rect::from_two_pos(*from, to);
            painter.rect_filled(r, CornerRadius::same(2), ACCENT.gamma_multiply(0.08));
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                Stroke::new(1.0, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn draw_ink_preview(&self, painter: &egui::Painter, to_screen: &impl Fn(Pos2) -> Pos2) {
        if let Gesture::Ink { points } = &self.gesture {
            let screen: Vec<Pos2> = points
                .iter()
                .map(|p| to_screen(egui::pos2(p[0] as f32, p[1] as f32)))
                .collect();
            if screen.len() > 1 {
                painter.add(egui::Shape::line(screen, Stroke::new(2.0, TEXT)));
            }
        }
    }
}

fn screen_rect(sh: &Value, zoom: f32, to_screen: &impl Fn(Pos2) -> Pos2) -> Rect {
    let b = board_rect(sh);
    Rect::from_min_size(to_screen(b.min), b.size() * zoom)
}

/// Where a line into `rect` from `towards` should stop.
fn edge_point(rect: Rect, towards: Pos2) -> Pos2 {
    let c = rect.center();
    let d = towards - c;
    if d == Vec2::ZERO {
        return c;
    }
    let half = rect.size() * 0.5;
    let scale = (half.x / d.x.abs()).min(half.y / d.y.abs());
    c + d * scale
}

// ---------------------------------------------------------------------------
// Pointer
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
impl Board {
    fn handle_pointer(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut SurfaceState,
        response: &egui::Response,
        shapes: &[Value],
        selection: &[String],
        pointer: Option<Pos2>,
        modifiers: egui::Modifiers,
        time: f64,
        to_screen: &impl Fn(Pos2) -> Pos2,
        to_board: &impl Fn(Pos2) -> Pos2,
    ) {
        self.guides.clear();
        let Some(p) = pointer else { return };
        let board = to_board(p);

        // --- what is under the pointer, topmost first -----------------------
        let hit = shapes
            .iter()
            .rev()
            .find(|sh| {
                !is_connector(sh) && screen_rect(sh, self.zoom, to_screen).contains(p)
            })
            .and_then(|sh| sh["id"].as_str().map(|s| s.to_string()));

        let handle_hit = if selection.len() == 1 {
            find(shapes, &selection[0])
                .filter(|sh| !is_locked(sh) && !is_ink(sh))
                .and_then(|sh| {
                    let r = screen_rect(sh, self.zoom, to_screen);
                    Handle::ALL.into_iter().find(|h| {
                        let (ax, ay) = h.anchor();
                        let c = r.lerp_inside(egui::vec2(ax, ay));
                        Rect::from_center_size(c, Vec2::splat(HANDLE * 2.0)).contains(p)
                    })
                })
        } else {
            None
        };
        if let Some(h) = handle_hit {
            ui.ctx().set_cursor_icon(h.cursor());
        }

        // --- begin a gesture -------------------------------------------------
        if response.drag_started() || (response.clicked() && matches!(self.gesture, Gesture::None)) {
            let starting_drag = response.drag_started();

            match self.tool {
                Tool::Select => {
                    if let Some(h) = handle_hit {
                        if starting_drag {
                            self.gesture = Gesture::Resize {
                                id: selection[0].clone(),
                                handle: h,
                            };
                            return;
                        }
                    }
                    match &hit {
                        Some(id) => {
                            let mut next = selection.to_vec();
                            if modifiers.shift || modifiers.command {
                                if let Some(i) = next.iter().position(|s| s == id) {
                                    next.remove(i);
                                } else {
                                    next.push(id.clone());
                                }
                            } else if !next.contains(id) {
                                next = vec![id.clone()];
                            }
                            state.doc["selection"] = json!(next.clone());
                            state.doc_dirty = true;

                            if starting_drag {
                                let grabs = next
                                    .iter()
                                    .filter_map(|sid| {
                                        let sh = find(shapes, sid)?;
                                        if is_locked(sh) || is_connector(sh) {
                                            return None;
                                        }
                                        Some((sid.clone(), board_rect(sh).min - board))
                                    })
                                    .collect::<Vec<_>>();
                                if !grabs.is_empty() {
                                    // The move itself is committed on release.
                                    state.doc_dirty = false;
                                    self.gesture = Gesture::Move { grabs };
                                }
                            }
                        }
                        None => {
                            if starting_drag {
                                self.gesture = if modifiers.alt {
                                    Gesture::Pan
                                } else {
                                    Gesture::Marquee { from: p }
                                };
                            } else if !selection.is_empty() {
                                state.doc["selection"] = json!([]);
                                state.doc_dirty = true;
                            }
                        }
                    }
                }
                Tool::Pen => {
                    if starting_drag {
                        self.gesture = Gesture::Ink {
                            points: vec![[board.x as f64, board.y as f64]],
                        };
                    }
                }
                Tool::Connect => {
                    if let Some(id) = &hit {
                        match &self.gesture {
                            Gesture::Connect { from } if from != id => {
                                let from = from.clone();
                                self.add_connector(state, &from, id, time);
                                self.gesture = Gesture::None;
                                self.tool = Tool::Select;
                            }
                            _ => self.gesture = Gesture::Connect { from: id.clone() },
                        }
                    }
                }
                creating => {
                    if !starting_drag {
                        self.create(state, creating, board, time);
                        self.tool = Tool::Select;
                    }
                }
            }
        }

        // --- continue a gesture ----------------------------------------------
        if response.dragged() {
            match &mut self.gesture {
                Gesture::Pan => self.pan += response.drag_delta() / self.zoom,
                Gesture::Ink { points } => {
                    points.push([board.x as f64, board.y as f64]);
                }
                Gesture::Move { grabs } => {
                    let grabs = grabs.clone();
                    self.move_selection(state, shapes, &grabs, board, modifiers.shift);
                }
                Gesture::Resize { id, handle } => {
                    let (id, handle) = (id.clone(), *handle);
                    self.resize(state, &id, handle, board, modifiers.shift);
                }
                Gesture::Marquee { .. } | Gesture::Connect { .. } | Gesture::None => {}
            }
        }

        // --- end a gesture ---------------------------------------------------
        if response.drag_stopped() {
            match std::mem::take(&mut self.gesture) {
                Gesture::Marquee { from } => {
                    let area = Rect::from_two_pos(from, p);
                    let picked: Vec<String> = shapes
                        .iter()
                        .filter(|sh| {
                            !is_connector(sh)
                                && area.intersects(screen_rect(sh, self.zoom, to_screen))
                        })
                        .filter_map(|sh| sh["id"].as_str().map(|s| s.to_string()))
                        .collect();
                    state.doc["selection"] = json!(picked);
                    state.doc_dirty = true;
                }
                Gesture::Ink { points } => {
                    if points.len() > 2 {
                        self.add_ink(state, points, time);
                    }
                }
                // A move or a resize mutated the document live; this is the one
                // point at which it becomes a commit.
                Gesture::Move { .. } | Gesture::Resize { .. } => state.doc_dirty = true,
                Gesture::Pan | Gesture::Connect { .. } | Gesture::None => {}
            }
        }

        if response.double_clicked() {
            if let Some(id) = &hit {
                if let Some(sh) = find(shapes, id) {
                    if !is_locked(sh) {
                        self.edit_buffer = sh["text"].as_str().unwrap_or("").to_string();
                        self.editing = Some(id.clone());
                    }
                }
            }
        }
    }

    /// Move every grabbed shape, snapping the first one to its neighbours and
    /// carrying the rest by the same delta so the selection keeps its shape.
    fn move_selection(
        &mut self,
        state: &mut SurfaceState,
        shapes: &[Value],
        grabs: &[(String, Vec2)],
        board: Pos2,
        no_snap: bool,
    ) {
        let Some((lead_id, lead_offset)) = grabs.first() else {
            return;
        };
        let Some(lead) = find(shapes, lead_id) else {
            return;
        };
        let size = board_rect(lead).size();
        let wanted = board + *lead_offset;

        let moving: Vec<&str> = grabs.iter().map(|(id, _)| id.as_str()).collect();
        let (snapped, guides) = if no_snap {
            (wanted, Vec::new())
        } else {
            self.snap(shapes, &moving, wanted, size)
        };
        self.guides = guides;
        let delta = snapped - wanted;

        for (id, offset) in grabs {
            let target = board + *offset + delta;
            with_shape(&mut state.doc, id, |sh| {
                sh["x"] = json!(target.x.round());
                sh["y"] = json!(target.y.round());
            });
        }
    }

    /// Nudge a position so its edges or centre line up with a neighbour's.
    /// Returns the adjusted position and the guides to draw.
    fn snap(
        &self,
        shapes: &[Value],
        moving: &[&str],
        wanted: Pos2,
        size: Vec2,
    ) -> (Pos2, Vec<(bool, f64, f64, f64)>) {
        let threshold = SNAP / self.zoom;
        let mut out = wanted;
        let mut guides = Vec::new();

        let candidates: Vec<Rect> = shapes
            .iter()
            .filter(|sh| {
                !is_connector(sh)
                    && !moving.contains(&sh["id"].as_str().unwrap_or(""))
            })
            .map(board_rect)
            .collect();

        let mine_x = [wanted.x, wanted.x + size.x * 0.5, wanted.x + size.x];
        let mine_y = [wanted.y, wanted.y + size.y * 0.5, wanted.y + size.y];

        let mut best_x: Option<(f32, f32)> = None;
        let mut best_y: Option<(f32, f32)> = None;

        for other in &candidates {
            for (i, mx) in mine_x.iter().enumerate() {
                for ox in [other.left(), other.center().x, other.right()] {
                    let d = ox - mx;
                    if d.abs() <= threshold && best_x.map(|(bd, _)| d.abs() < bd.abs()).unwrap_or(true)
                    {
                        best_x = Some((d, ox));
                        let _ = i;
                    }
                }
            }
            for my in mine_y.iter() {
                for oy in [other.top(), other.center().y, other.bottom()] {
                    let d = oy - my;
                    if d.abs() <= threshold && best_y.map(|(bd, _)| d.abs() < bd.abs()).unwrap_or(true)
                    {
                        best_y = Some((d, oy));
                    }
                }
            }
        }

        if let Some((d, at)) = best_x {
            out.x += d;
            let span = candidates
                .iter()
                .filter(|r| (r.left() - at).abs() < 0.5 || (r.center().x - at).abs() < 0.5 || (r.right() - at).abs() < 0.5)
                .fold((out.y, out.y + size.y), |(lo, hi), r| {
                    (lo.min(r.top()), hi.max(r.bottom()))
                });
            guides.push((true, at as f64, span.0 as f64 - 20.0, span.1 as f64 + 20.0));
        }
        if let Some((d, at)) = best_y {
            out.y += d;
            let span = candidates
                .iter()
                .filter(|r| (r.top() - at).abs() < 0.5 || (r.center().y - at).abs() < 0.5 || (r.bottom() - at).abs() < 0.5)
                .fold((out.x, out.x + size.x), |(lo, hi), r| {
                    (lo.min(r.left()), hi.max(r.right()))
                });
            guides.push((false, at as f64, span.0 as f64 - 20.0, span.1 as f64 + 20.0));
        }

        (out, guides)
    }

    fn resize(
        &mut self,
        state: &mut SurfaceState,
        id: &str,
        handle: Handle,
        board: Pos2,
        keep_ratio: bool,
    ) {
        with_shape(&mut state.doc, id, |sh| {
            let x = num(sh, "x");
            let y = num(sh, "y");
            let w = num(sh, "w");
            let h = num(sh, "h");
            let (mut left, mut top, mut right, mut bottom) = (x, y, x + w, y + h);
            let (px, py) = (board.x as f64, board.y as f64);

            match handle {
                Handle::NW => {
                    left = px;
                    top = py;
                }
                Handle::N => top = py,
                Handle::NE => {
                    right = px;
                    top = py;
                }
                Handle::E => right = px,
                Handle::SE => {
                    right = px;
                    bottom = py;
                }
                Handle::S => bottom = py,
                Handle::SW => {
                    left = px;
                    bottom = py;
                }
                Handle::W => left = px,
            }

            let mut new_w = (right - left).max(24.0);
            let mut new_h = (bottom - top).max(18.0);
            if keep_ratio && w > 0.0 && h > 0.0 {
                let ratio = w / h;
                if new_w / new_h > ratio {
                    new_w = new_h * ratio;
                } else {
                    new_h = new_w / ratio;
                }
            }
            // Dragging a top or left handle moves the origin as well as the size.
            let new_x = if matches!(handle, Handle::NW | Handle::W | Handle::SW) {
                right - new_w
            } else {
                left
            };
            let new_y = if matches!(handle, Handle::NW | Handle::N | Handle::NE) {
                bottom - new_h
            } else {
                top
            };

            sh["x"] = json!(new_x.round());
            sh["y"] = json!(new_y.round());
            sh["w"] = json!(new_w.round());
            sh["h"] = json!(new_h.round());
        });
    }
}

// ---------------------------------------------------------------------------
// Creating things
// ---------------------------------------------------------------------------

impl Board {
    fn create(&mut self, state: &mut SurfaceState, tool: Tool, at: Pos2, time: f64) {
        let id = self.new_id(time);
        let z = next_z(&state.doc);
        let shape = match tool {
            Tool::Sticky => json!({
                "id": id, "kind": "sticky",
                "x": at.x.round(), "y": at.y.round(), "w": 130.0, "h": 110.0,
                "fill": "yellow", "text": "", "frame": Value::Null, "z": z, "locked": false
            }),
            Tool::Rect => json!({
                "id": id, "kind": "rect",
                "x": at.x.round(), "y": at.y.round(), "w": 160.0, "h": 90.0,
                "fill": "grey", "text": "", "frame": Value::Null, "z": z, "locked": false
            }),
            Tool::Ellipse => json!({
                "id": id, "kind": "ellipse",
                "x": at.x.round(), "y": at.y.round(), "w": 140.0, "h": 100.0,
                "fill": "blue", "text": "", "frame": Value::Null, "z": z, "locked": false
            }),
            Tool::Text => json!({
                "id": id, "kind": "text",
                "x": at.x.round(), "y": at.y.round(), "w": 140.0, "h": 26.0,
                "size": 16.0, "fill": "none", "text": "Text", "frame": Value::Null,
                "z": z, "locked": false
            }),
            _ => return,
        };
        if let Some(list) = state.doc["shapes"].as_array_mut() {
            list.push(shape);
        }
        state.doc["selection"] = json!([id.clone()]);
        state.doc_dirty = true;
        // A new sticky or label is almost always about to be typed into.
        if matches!(tool, Tool::Sticky | Tool::Text) {
            self.edit_buffer = if tool == Tool::Text { "Text".into() } else { String::new() };
            self.editing = Some(id);
        }
    }

    fn add_ink(&mut self, state: &mut SurfaceState, points: Vec<[f64; 2]>, time: f64) {
        let id = self.new_id(time);
        let z = next_z(&state.doc);
        let stroke = json!({
            "id": id, "kind": "ink", "points": points,
            "fill": "grey", "text": "", "frame": Value::Null, "z": z, "locked": false
        });
        if let Some(list) = state.doc["shapes"].as_array_mut() {
            list.push(stroke);
        }
        state.doc_dirty = true;
    }

    fn add_connector(&mut self, state: &mut SurfaceState, from: &str, to: &str, time: f64) {
        let id = self.new_id(time);
        let z = next_z(&state.doc);
        let arrow = json!({
            "id": id, "kind": "arrow", "from": from, "to": to,
            "text": "", "fill": "grey",
            "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0,
            "frame": Value::Null, "z": z, "locked": false
        });
        if let Some(list) = state.doc["shapes"].as_array_mut() {
            list.push(arrow);
        }
        state.doc_dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

impl Board {
    fn handle_keyboard(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut SurfaceState,
        shapes: &[Value],
        selection: &[String],
        time: f64,
    ) {
        if self.editing.is_some() {
            return; // The text editor owns the keyboard.
        }
        let (keys, modifiers) = ui.input(|i| {
            (
                [
                    Key::Delete,
                    Key::Backspace,
                    Key::Escape,
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::A,
                    Key::C,
                    Key::V,
                    Key::D,
                    Key::L,
                    Key::V,
                ]
                .into_iter()
                .filter(|k| i.key_pressed(*k))
                .collect::<Vec<_>>(),
                i.modifiers,
            )
        });
        if keys.is_empty() {
            return;
        }
        let cmd = modifiers.command || modifiers.ctrl;

        for key in keys {
            match key {
                Key::Escape => {
                    state.doc["selection"] = json!([]);
                    state.doc_dirty = true;
                    self.tool = Tool::Select;
                }
                Key::Delete | Key::Backspace if !selection.is_empty() => {
                    let keep: Vec<Value> = shapes
                        .iter()
                        .filter(|sh| {
                            let id = sh["id"].as_str().unwrap_or("");
                            !(selection.iter().any(|s| s == id) && !is_locked(sh))
                        })
                        .cloned()
                        .collect();
                    state.doc["shapes"] = json!(keep);
                    state.doc["selection"] = json!([]);
                    state.doc_dirty = true;
                }
                Key::A if cmd => {
                    let all: Vec<String> = shapes
                        .iter()
                        .filter(|sh| !is_connector(sh))
                        .filter_map(|sh| sh["id"].as_str().map(|s| s.to_string()))
                        .collect();
                    state.doc["selection"] = json!(all);
                    state.doc_dirty = true;
                }
                Key::C if cmd => {
                    self.clipboard = selection
                        .iter()
                        .filter_map(|id| find(shapes, id).cloned())
                        .filter(|sh| !is_connector(sh))
                        .collect();
                }
                Key::V if cmd && !self.clipboard.is_empty() => {
                    let pasted = self.paste(state, time);
                    state.doc["selection"] = json!(pasted);
                    state.doc_dirty = true;
                }
                Key::D if cmd && !selection.is_empty() => {
                    self.clipboard = selection
                        .iter()
                        .filter_map(|id| find(shapes, id).cloned())
                        .filter(|sh| !is_connector(sh))
                        .collect();
                    let pasted = self.paste(state, time);
                    state.doc["selection"] = json!(pasted);
                    state.doc_dirty = true;
                }
                Key::L if cmd && !selection.is_empty() => {
                    let any_unlocked = selection
                        .iter()
                        .filter_map(|id| find(shapes, id))
                        .any(|sh| !is_locked(sh));
                    for id in selection {
                        with_shape(&mut state.doc, id, |sh| sh["locked"] = json!(any_unlocked));
                    }
                    state.doc_dirty = true;
                }
                Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                    let step = if modifiers.shift { 10.0 } else { 1.0 };
                    let (dx, dy) = match key {
                        Key::ArrowLeft => (-step, 0.0),
                        Key::ArrowRight => (step, 0.0),
                        Key::ArrowUp => (0.0, -step),
                        _ => (0.0, step),
                    };
                    for id in selection {
                        let movable = find(shapes, id)
                            .map(|sh| !is_locked(sh) && !is_connector(sh))
                            .unwrap_or(false);
                        if movable {
                            with_shape(&mut state.doc, id, |sh| {
                                sh["x"] = json!(num(sh, "x") + dx);
                                sh["y"] = json!(num(sh, "y") + dy);
                            });
                        }
                    }
                    state.doc_dirty = true;
                }
                _ => {}
            }
        }
    }

    fn paste(&mut self, state: &mut SurfaceState, time: f64) -> Vec<String> {
        let mut made = Vec::new();
        let mut z = next_z(&state.doc);
        for original in self.clipboard.clone() {
            let mut copy = original;
            let id = self.new_id(time);
            copy["id"] = json!(id);
            copy["x"] = json!(num(&copy, "x") + 24.0);
            copy["y"] = json!(num(&copy, "y") + 24.0);
            copy["z"] = json!(z);
            copy["locked"] = json!(false);
            z += 1;
            if let Some(list) = state.doc["shapes"].as_array_mut() {
                list.push(copy);
            }
            made.push(id);
        }
        made
    }
}

// ---------------------------------------------------------------------------
// Chrome: tool palette, selection toolbar, text editor, hint
// ---------------------------------------------------------------------------

impl Board {
    fn tool_palette(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let tools = [
            Tool::Select,
            Tool::Sticky,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Text,
            Tool::Pen,
            Tool::Connect,
        ];
        let size = Vec2::new(38.0, 38.0 * tools.len() as f32 + 8.0);
        let origin = rect.left_center() - Vec2::new(0.0, size.y / 2.0) + Vec2::new(12.0, 0.0);
        let panel = Rect::from_min_size(origin, size);

        let painter = ui.painter_at(rect);
        painter.rect_filled(panel, CornerRadius::same(10), SURFACE);
        painter.rect_stroke(
            panel,
            CornerRadius::same(10),
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );

        for (i, tool) in tools.into_iter().enumerate() {
            let slot = Rect::from_min_size(
                panel.min + Vec2::new(3.0, 4.0 + i as f32 * 38.0),
                Vec2::splat(32.0),
            );
            let id = egui::Id::new(("tool", i));
            let r = ui.interact(slot, id, Sense::click());
            let selected = self.tool == tool;
            if selected {
                painter.rect_filled(slot, CornerRadius::same(7), ACCENT.gamma_multiply(0.12));
            } else if r.hovered() {
                painter.rect_filled(slot, CornerRadius::same(7), Color32::from_gray(0xF3));
            }
            draw_tool_icon(&painter, slot.shrink(9.0), tool, if selected { ACCENT } else { MUTED });
            if r.clicked() {
                self.tool = tool;
            }
            r.on_hover_text(tool.label());
        }
    }

    /// The toolbar that appears above a selection: colour, order, lock, copy, delete.
    fn draw_selection_chrome(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        state: &mut SurfaceState,
        to_screen: &impl Fn(Pos2) -> Pos2,
    ) {
        let shapes = shapes_of(&state.doc);
        let selection = selection_of(&state.doc);
        if selection.is_empty() || !matches!(self.gesture, Gesture::None) {
            return;
        }

        let rects: Vec<Rect> = selection
            .iter()
            .filter_map(|id| find(&shapes, id))
            .filter(|sh| !is_connector(sh))
            .map(|sh| screen_rect(sh, self.zoom, to_screen))
            .collect();
        let Some(first) = rects.first() else { return };
        let bounds = rects.iter().fold(*first, |a, b| a.union(*b));

        // Resize handles, for a single unlocked shape.
        if selection.len() == 1 {
            if let Some(sh) = find(&shapes, &selection[0]) {
                if !is_locked(sh) && !is_ink(sh) && sh["kind"].as_str() != Some("text") {
                    for h in Handle::ALL {
                        let (ax, ay) = h.anchor();
                        let c = bounds.lerp_inside(egui::vec2(ax, ay));
                        painter.rect_filled(
                            Rect::from_center_size(c, Vec2::splat(HANDLE)),
                            CornerRadius::same(2),
                            PAGE,
                        );
                        painter.rect_stroke(
                            Rect::from_center_size(c, Vec2::splat(HANDLE)),
                            CornerRadius::same(2),
                            Stroke::new(1.2, ACCENT),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        } else {
            painter.rect_stroke(
                bounds.expand(4.0),
                CornerRadius::same(8),
                Stroke::new(1.0, ACCENT.gamma_multiply(0.6)),
                egui::StrokeKind::Outside,
            );
        }

        // --- the toolbar ----------------------------------------------------
        let buttons = COLOURS.len() + 5;
        let width = buttons as f32 * 30.0;
        let clip = painter.clip_rect();
        // Above the selection, unless there is no room — then below it, so the
        // toolbar never covers the thing it is acting on.
        let above = bounds.top() - 46.0;
        let top = if above < clip.top() + 6.0 {
            bounds.bottom() + 12.0
        } else {
            above
        };
        let bar = Rect::from_min_size(
            egui::pos2(
                (bounds.center().x - width / 2.0)
                    .clamp(clip.left() + 6.0, (clip.right() - width - 6.0).max(clip.left() + 6.0)),
                top,
            ),
            Vec2::new(width, 34.0),
        );
        painter.rect_filled(bar, CornerRadius::same(9), SURFACE);
        painter.rect_stroke(
            bar,
            CornerRadius::same(9),
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );

        let slot = |i: usize| {
            Rect::from_min_size(bar.min + Vec2::new(3.0 + i as f32 * 30.0, 3.0), Vec2::splat(28.0))
        };
        let mut action: Option<&'static str> = None;
        let mut recolour: Option<&'static str> = None;

        for (i, name) in COLOURS.iter().enumerate() {
            let r = ui.interact(slot(i), egui::Id::new(("swatch", i)), Sense::click());
            let (accent, tint) = palette(name);
            painter.circle_filled(slot(i).center(), 9.0, tint);
            painter.circle_stroke(slot(i).center(), 9.0, Stroke::new(1.6, accent));
            if r.hovered() {
                painter.circle_stroke(slot(i).center(), 12.0, Stroke::new(1.0, accent));
            }
            if r.clicked() {
                recolour = Some(name);
            }
            r.on_hover_text(*name);
        }

        let locked_now = selection
            .iter()
            .filter_map(|id| find(&shapes, id))
            .all(is_locked);
        for (n, (key, tip)) in [
            ("front", "Bring to front"),
            ("back", "Send to back"),
            ("dup", "Duplicate"),
            ("lock", if locked_now { "Unlock" } else { "Lock" }),
            ("del", "Delete"),
        ]
        .into_iter()
        .enumerate()
        {
            let i = COLOURS.len() + n;
            let r = ui.interact(slot(i), egui::Id::new(("act", i)), Sense::click());
            if r.hovered() {
                painter.rect_filled(slot(i), CornerRadius::same(6), Color32::from_gray(0xF3));
            }
            draw_action_icon(painter, slot(i).shrink(8.0), key, if key == "del" { Color32::from_rgb(0xDC, 0x26, 0x26) } else { MUTED }, locked_now);
            if r.clicked() {
                action = Some(key);
            }
            r.on_hover_text(tip);
        }

        // --- apply ----------------------------------------------------------
        if let Some(name) = recolour {
            for id in &selection {
                with_shape(&mut state.doc, id, |sh| sh["fill"] = json!(name));
            }
            state.doc_dirty = true;
        }
        match action {
            Some("front") | Some("back") => {
                let to_front = action == Some("front");
                let top = next_z(&state.doc);
                let bottom = shapes
                    .iter()
                    .filter_map(|s| s["z"].as_i64())
                    .min()
                    .unwrap_or(0);
                for (n, id) in selection.iter().enumerate() {
                    with_shape(&mut state.doc, id, |sh| {
                        sh["z"] = json!(if to_front {
                            top + n as i64
                        } else {
                            bottom - 1 - n as i64
                        });
                    });
                }
                state.doc_dirty = true;
            }
            Some("dup") => {
                self.clipboard = selection
                    .iter()
                    .filter_map(|id| find(&shapes, id).cloned())
                    .filter(|sh| !is_connector(sh))
                    .collect();
                let made = self.paste(state, ui.input(|i| i.time));
                state.doc["selection"] = json!(made);
                state.doc_dirty = true;
            }
            Some("lock") => {
                for id in &selection {
                    with_shape(&mut state.doc, id, |sh| sh["locked"] = json!(!locked_now));
                }
                state.doc_dirty = true;
            }
            Some("del") => {
                let keep: Vec<Value> = shapes
                    .iter()
                    .filter(|sh| {
                        let id = sh["id"].as_str().unwrap_or("");
                        !(selection.iter().any(|s| s == id) && !is_locked(sh))
                    })
                    .cloned()
                    .collect();
                state.doc["shapes"] = json!(keep);
                state.doc["selection"] = json!([]);
                state.doc_dirty = true;
            }
            _ => {}
        }
    }

    fn text_editor(&mut self, ui: &mut egui::Ui, state: &mut SurfaceState) {
        let Some(id) = self.editing.clone() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Edit text")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.edit_buffer)
                        .desired_width(260.0)
                        .desired_rows(2),
                );
                response.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.editing = None;
                    }
                });
            });

        if apply {
            let text = self.edit_buffer.clone();
            with_shape(&mut state.doc, &id, |sh| sh["text"] = json!(text));
            state.doc_dirty = true;
            self.editing = None;
        }
        if !open {
            self.editing = None;
        }
    }

    fn hint(&self, painter: &egui::Painter, rect: Rect, shapes: &[Value], selection: &[String]) {
        if shapes.is_empty() {
            painter.text(
                rect.center() - egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "This board is empty",
                egui::FontId::proportional(13.5),
                MUTED,
            );
            painter.text(
                rect.center() + egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                "Pick a tool on the left, or ask the agent.",
                egui::FontId::proportional(11.5),
                FAINT,
            );
            return;
        }
        let line = if selection.is_empty() {
            format!(
                "{} shapes · {} · drag to select · Ctrl+A all",
                shapes.len(),
                self.tool.label()
            )
        } else {
            format!(
                "{} of {} selected · arrows nudge · Ctrl+D duplicate · Ctrl+L lock · Delete removes",
                selection.len(),
                shapes.len()
            )
        };
        painter.text(
            rect.left_bottom() + egui::vec2(12.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            line,
            egui::FontId::proportional(10.5),
            FAINT,
        );
    }
}

// ---------------------------------------------------------------------------
// Icons, drawn rather than typed
// ---------------------------------------------------------------------------

fn draw_tool_icon(painter: &egui::Painter, rect: Rect, tool: Tool, colour: Color32) {
    let stroke = Stroke::new(1.5, colour);
    let c = rect.center();
    match tool {
        Tool::Select => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    rect.left_top(),
                    rect.left_top() + Vec2::new(0.0, rect.height()),
                    rect.left_top() + Vec2::new(rect.width() * 0.42, rect.height() * 0.72),
                    rect.left_top() + Vec2::new(rect.width() * 0.72, rect.height() * 0.62),
                ],
                colour,
                Stroke::NONE,
            ));
        }
        Tool::Sticky => {
            painter.rect_filled(rect, CornerRadius::same(2), colour.gamma_multiply(0.25));
            painter.rect_stroke(rect, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
        }
        Tool::Rect => {
            painter.rect_stroke(
                Rect::from_center_size(c, rect.size() * egui::vec2(1.0, 0.72)),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Tool::Ellipse => {
            painter.add(egui::Shape::ellipse_stroke(
                c,
                rect.size() * 0.5 * egui::vec2(1.0, 0.8),
                stroke,
            ));
        }
        Tool::Text => {
            painter.line_segment([rect.left_top(), rect.right_top()], stroke);
            painter.line_segment([c.to_owned() - Vec2::new(0.0, rect.height() * 0.5), c + Vec2::new(0.0, rect.height() * 0.5)], stroke);
        }
        Tool::Pen => {
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: (0..=10)
                    .map(|i| {
                        let t = i as f32 / 10.0;
                        rect.left_bottom()
                            + Vec2::new(rect.width() * t, -rect.height() * (t * t).min(1.0))
                    })
                    .collect(),
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: stroke.into(),
            }));
        }
        Tool::Connect => {
            painter.circle_stroke(rect.left_bottom() + Vec2::new(2.0, -2.0), 2.5, stroke);
            painter.circle_stroke(rect.right_top() + Vec2::new(-2.0, 2.0), 2.5, stroke);
            painter.line_segment(
                [
                    rect.left_bottom() + Vec2::new(4.0, -4.0),
                    rect.right_top() + Vec2::new(-4.0, 4.0),
                ],
                stroke,
            );
        }
    }
}

fn draw_action_icon(
    painter: &egui::Painter,
    rect: Rect,
    key: &str,
    colour: Color32,
    locked: bool,
) {
    let stroke = Stroke::new(1.4, colour);
    let c = rect.center();
    match key {
        "front" | "back" => {
            let up = key == "front";
            let back = Rect::from_center_size(c + Vec2::new(2.5, 2.5), rect.size() * 0.66);
            let front = Rect::from_center_size(c - Vec2::new(2.5, 2.5), rect.size() * 0.66);
            let (dim, solid) = if up { (back, front) } else { (front, back) };
            painter.rect_stroke(dim, CornerRadius::same(2), Stroke::new(1.0, colour.gamma_multiply(0.4)), egui::StrokeKind::Inside);
            painter.rect_filled(solid, CornerRadius::same(2), colour.gamma_multiply(0.35));
            painter.rect_stroke(solid, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
        }
        "dup" => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::splat(2.0), rect.size() * 0.7),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_center_size(c - Vec2::splat(2.0), rect.size() * 0.7),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        "lock" => {
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, 2.5), egui::vec2(rect.width() * 0.8, rect.height() * 0.5)),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            let shackle: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
                    c + Vec2::new(a.cos(), a.sin()) * rect.width() * 0.26 - Vec2::new(0.0, 2.0)
                })
                .collect();
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: if locked {
                    shackle
                } else {
                    shackle.into_iter().map(|p| p + Vec2::new(3.0, 0.0)).collect()
                },
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: stroke.into(),
            }));
        }
        _ => {
            // A bin: lid, body, one rib.
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.top() + 2.0),
                    egui::pos2(rect.right(), rect.top() + 2.0),
                ],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_min_max(
                    egui::pos2(rect.left() + 2.0, rect.top() + 4.0),
                    rect.right_bottom(),
                ),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([egui::pos2(c.x, rect.top() + 7.0), egui::pos2(c.x, rect.bottom() - 3.0)], stroke);
        }
    }
}

localspace_surface_sdk::export_surface!(Board);
