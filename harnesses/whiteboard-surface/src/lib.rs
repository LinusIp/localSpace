//! Reference harness #1 — the whiteboard's `kind = "egui"` surface.
//!
//! An infinite dot-grid canvas of node cards: pan, zoom, select, drag, and edit a
//! card's text. It has no filesystem, no network and no model access; it reads and
//! writes the shared document and nothing else.
//!
//! It follows the Client's visual language on purpose — white cards, hairline
//! borders, a colour per type — so a harness surface does not look like a foreign
//! object dropped into the app.

use localspace_surface_sdk::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use localspace_surface_sdk::{Surface, SurfaceInit, SurfaceState};
use serde_json::{json, Value};

const PAGE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const DOT: Color32 = Color32::from_rgb(0xE3, 0xE6, 0xE6);
const TEXT: Color32 = Color32::from_rgb(0x1F, 0x23, 0x28);
const MUTED: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const FAINT: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);
const LINE: Color32 = Color32::from_rgb(0xB6, 0xBC, 0xC2);

#[derive(Default)]
struct Board {
    pan: Vec2,
    zoom: f32,
    dragging: Option<(String, Vec2)>,
    editing: Option<String>,
    edit_buffer: String,
    ready: bool,
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

impl Board {
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
        let shapes = state.doc["shapes"].as_array().cloned().unwrap_or_default();
        let boxes: Vec<&Value> = shapes
            .iter()
            .filter(|s| s["kind"].as_str() != Some("arrow"))
            .collect();
        if boxes.is_empty() {
            self.pan = Vec2::ZERO;
            return;
        }
        let min_x = boxes.iter().map(|s| num(s, "x")).fold(f32::MAX, f32::min);
        let min_y = boxes.iter().map(|s| num(s, "y")).fold(f32::MAX, f32::min);
        self.pan = Vec2::new(40.0 - min_x, 40.0 - min_y);
    }

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
        let to_screen = |p: Pos2| -> Pos2 { rect.min + (p.to_vec2() + self.pan) * z };
        let to_board = |p: Pos2| -> Pos2 { ((p - rect.min) / z - self.pan).to_pos2() };

        // ---- frames -------------------------------------------------------
        for f in state.doc["frames"].as_array().cloned().unwrap_or_default() {
            let r = Rect::from_min_size(
                to_screen(egui::pos2(num(&f, "x"), num(&f, "y"))),
                egui::vec2(num(&f, "w"), num(&f, "h")) * z,
            );
            painter.rect_stroke(
                r,
                CornerRadius::same(10),
                Stroke::new(1.0, DOT),
                egui::StrokeKind::Outside,
            );
            painter.text(
                r.min + egui::vec2(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                f["name"].as_str().unwrap_or("frame"),
                egui::FontId::proportional(11.0 * z.clamp(0.7, 1.4)),
                FAINT,
            );
        }

        let shapes = state.doc["shapes"].as_array().cloned().unwrap_or_default();
        let selection: Vec<String> = state.doc["selection"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // ---- arrows first, so cards sit on top ----------------------------
        for sh in &shapes {
            if sh["kind"].as_str() != Some("arrow") {
                continue;
            }
            let (Some(from), Some(to)) = (sh["from"].as_str(), sh["to"].as_str()) else {
                continue;
            };
            let (Some(a), Some(b)) = (find(&shapes, from), find(&shapes, to)) else {
                continue;
            };
            let ar = card_rect(&a, z, &to_screen);
            let br = card_rect(&b, z, &to_screen);
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
                    egui::FontId::proportional(10.5 * z.clamp(0.7, 1.3)),
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

        // ---- cards --------------------------------------------------------
        let mut clicked: Option<String> = None;
        let mut double_clicked: Option<String> = None;
        let pointer = ui.input(|i| i.pointer.interact_pos());

        for sh in &shapes {
            let kind = sh["kind"].as_str().unwrap_or("rect");
            if kind == "arrow" {
                continue;
            }
            let id = sh["id"].as_str().unwrap_or("").to_string();
            let r = card_rect(sh, z, &to_screen);
            let (accent, tint) = palette(sh["fill"].as_str().unwrap_or("grey"));
            let selected = selection.contains(&id);

            if kind == "ellipse" {
                painter.add(egui::Shape::ellipse_filled(r.center(), r.size() * 0.5, PAGE));
                painter.add(egui::Shape::ellipse_stroke(
                    r.center(),
                    r.size() * 0.5,
                    Stroke::new(if selected { 2.0 } else { 1.4 }, accent),
                ));
            } else {
                painter.rect_filled(r, CornerRadius::same(10), PAGE);
                painter.rect_stroke(
                    r,
                    CornerRadius::same(10),
                    Stroke::new(if selected { 2.0 } else { 1.4 }, accent),
                    egui::StrokeKind::Inside,
                );
            }

            // Icon chip and type label, as on every card in the app.
            let pad = 9.0 * z.clamp(0.6, 1.4);
            let chip = Rect::from_min_size(r.min + Vec2::splat(pad), Vec2::splat(15.0 * z.clamp(0.6, 1.4)));
            painter.rect_filled(chip, CornerRadius::same(4), tint);
            painter.rect_filled(chip.shrink(chip.width() * 0.3), CornerRadius::same(1), accent);
            painter.text(
                egui::pos2(chip.right() + 6.0 * z.clamp(0.6, 1.4), chip.center().y),
                egui::Align2::LEFT_CENTER,
                kind.to_uppercase(),
                egui::FontId::proportional(8.5 * z.clamp(0.7, 1.4)),
                accent,
            );

            let text = sh["text"].as_str().unwrap_or("");
            if !text.is_empty() {
                let font = egui::FontId::proportional(12.5 * z.clamp(0.6, 1.5));
                let galley = painter.layout(
                    text.to_string(),
                    font,
                    TEXT,
                    (r.width() - pad * 2.0).max(20.0),
                );
                painter.galley(
                    egui::pos2(r.left() + pad, chip.bottom() + 7.0 * z.clamp(0.6, 1.4)),
                    galley,
                    TEXT,
                );
            }

            if let Some(p) = pointer {
                if r.contains(p) {
                    if response.drag_started() {
                        clicked = Some(id.clone());
                        self.dragging = Some((id.clone(), p - r.min));
                    }
                    if response.clicked() {
                        clicked = Some(id.clone());
                    }
                    if response.double_clicked() {
                        double_clicked = Some(id.clone());
                    }
                }
            }
        }

        // ---- interaction --------------------------------------------------
        if response.drag_started() && clicked.is_none() {
            self.dragging = None;
        }
        if response.dragged() {
            match self.dragging.clone() {
                Some((id, grab)) => {
                    if let Some(p) = pointer {
                        let target = to_board(p - grab);
                        if let Some(list) = state.doc["shapes"].as_array_mut() {
                            if let Some(sh) =
                                list.iter_mut().find(|s| s["id"].as_str() == Some(&id))
                            {
                                sh["x"] = json!(target.x.round());
                                sh["y"] = json!(target.y.round());
                                state.doc_dirty = true;
                            }
                        }
                    }
                }
                None => self.pan += response.drag_delta() / z,
            }
        }
        if response.drag_stopped() {
            self.dragging = None;
        }

        if let Some(id) = &clicked {
            state.doc["selection"] = json!([id]);
            state.doc_dirty = true;
        } else if response.clicked() {
            state.doc["selection"] = json!([]);
            state.doc_dirty = true;
        }

        if let Some(id) = double_clicked {
            self.edit_buffer = shapes
                .iter()
                .find(|s| s["id"].as_str() == Some(&id))
                .and_then(|s| s["text"].as_str())
                .unwrap_or("")
                .to_string();
            self.editing = Some(id);
        }

        if let Some(id) = self.editing.clone() {
            let mut open = true;
            egui::Window::new("Edit text")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.edit_buffer).desired_width(240.0),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            if let Some(list) = state.doc["shapes"].as_array_mut() {
                                if let Some(sh) =
                                    list.iter_mut().find(|s| s["id"].as_str() == Some(&id))
                                {
                                    sh["text"] = json!(self.edit_buffer.clone());
                                    state.doc_dirty = true;
                                }
                            }
                            self.editing = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.editing = None;
                        }
                    });
                });
            if !open {
                self.editing = None;
            }
        }

        // ---- hint ---------------------------------------------------------
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
                "Ask the agent for something, or use a front-door tool in Details.",
                egui::FontId::proportional(11.5),
                FAINT,
            );
        } else {
            painter.text(
                rect.left_bottom() + egui::vec2(12.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "{} cards · drag to move · empty space to pan · double-click to edit",
                    shapes.len()
                ),
                egui::FontId::proportional(10.5),
                FAINT,
            );
        }

        if state.doc_dirty {
            state.send_json(&json!({"kind": "surface_edit"}));
        }
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
        let mut y = rect.top() + offset.y;
        while y < rect.bottom() {
            let mut x = rect.left() + offset.x;
            while x < rect.right() {
                painter.circle_filled(egui::pos2(x, y), 1.0, DOT);
                x += step;
            }
            y += step;
        }
    }
}

fn card_rect(sh: &Value, zoom: f32, to_screen: &impl Fn(Pos2) -> Pos2) -> Rect {
    Rect::from_min_size(
        to_screen(egui::pos2(num(sh, "x"), num(sh, "y"))),
        egui::vec2(num(sh, "w").max(60.0), num(sh, "h").max(44.0)) * zoom,
    )
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

fn num(v: &Value, key: &str) -> f32 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32
}

fn find(shapes: &[Value], id: &str) -> Option<Value> {
    shapes.iter().find(|s| s["id"].as_str() == Some(id)).cloned()
}

/// (border, chip tint) per declared colour.
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

localspace_surface_sdk::export_surface!(Board);
