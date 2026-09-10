//! The Client's visual language: palette, spacing, and the handful of primitives
//! every panel is built from.
//!
//! Light surfaces on a near-white page, 1 px hairline borders, 8–10 px radii,
//! small type, and one green accent. Type colours (green / purple / amber / blue)
//! are reserved for classifying things — a harness tier, a tool kind, a status —
//! never for decoration.

use egui::{Color32, CornerRadius, Margin, Response, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy)]
pub struct Palette {
    pub page: Color32,
    pub rail: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub purple: Color32,
    pub purple_soft: Color32,
    pub amber: Color32,
    pub amber_soft: Color32,
    pub blue: Color32,
    pub blue_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
}

pub const P: Palette = Palette {
    page: Color32::from_rgb(0xF7, 0xF8, 0xF8),
    rail: Color32::from_rgb(0xF3, 0xF4, 0xF4),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_alt: Color32::from_rgb(0xFA, 0xFB, 0xFB),
    border: Color32::from_rgb(0xE6, 0xE8, 0xE8),
    border_strong: Color32::from_rgb(0xD3, 0xD7, 0xD7),
    text: Color32::from_rgb(0x1F, 0x23, 0x28),
    muted: Color32::from_rgb(0x6B, 0x72, 0x80),
    faint: Color32::from_rgb(0x9C, 0xA3, 0xAF),
    accent: Color32::from_rgb(0x16, 0xA3, 0x4A),
    accent_soft: Color32::from_rgb(0xEC, 0xFD, 0xF3),
    purple: Color32::from_rgb(0x7C, 0x5C, 0xFF),
    purple_soft: Color32::from_rgb(0xF3, 0xF0, 0xFF),
    amber: Color32::from_rgb(0xC8, 0x86, 0x14),
    amber_soft: Color32::from_rgb(0xFF, 0xF8, 0xEC),
    blue: Color32::from_rgb(0x2E, 0x6F, 0xE8),
    blue_soft: Color32::from_rgb(0xEF, 0xF6, 0xFF),
    danger: Color32::from_rgb(0xDC, 0x26, 0x26),
    danger_soft: Color32::from_rgb(0xFE, 0xF2, 0xF2),
};

pub const RADIUS: u8 = 8;
pub const RAIL_WIDTH: f32 = 76.0;
pub const DETAILS_WIDTH: f32 = 360.0;
pub const TOP_BAR_HEIGHT: f32 = 56.0;
pub const STATUS_BAR_HEIGHT: f32 = 30.0;

/// Install the palette into egui's own style, so stock widgets match.
pub fn apply(ctx: &egui::Context) {
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Light);

    ctx.all_styles_mut(|style| tune(style));
}

fn tune(style: &mut egui::Style) {
    let v = &mut style.visuals;
    v.dark_mode = false;
    v.panel_fill = P.surface;
    v.window_fill = P.surface;
    v.extreme_bg_color = P.surface;
    v.faint_bg_color = P.surface_alt;
    v.window_stroke = Stroke::new(1.0, P.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.override_text_color = Some(P.text);
    v.selection.bg_fill = P.accent_soft;
    v.selection.stroke = Stroke::new(1.0, P.accent);
    v.hyperlink_color = P.accent;

    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
        w.bg_stroke = Stroke::new(1.0, P.border);
        w.fg_stroke = Stroke::new(1.0, P.text);
    }
    v.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, P.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, P.text);
    v.widgets.inactive.weak_bg_fill = P.surface;
    v.widgets.inactive.bg_fill = P.surface;
    v.widgets.hovered.weak_bg_fill = P.surface_alt;
    v.widgets.hovered.bg_fill = P.surface_alt;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, P.border_strong);
    v.widgets.active.weak_bg_fill = P.accent_soft;
    v.widgets.active.bg_fill = P.accent_soft;
    v.widgets.active.bg_stroke = Stroke::new(1.0, P.accent);

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.window_margin = Margin::same(12);
    style.spacing.interact_size.y = 26.0;

    use egui::{FontFamily::Proportional, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(15.0, Proportional)),
        (TextStyle::Body, FontId::new(12.5, Proportional)),
        (TextStyle::Button, FontId::new(12.5, Proportional)),
        (TextStyle::Small, FontId::new(10.5, Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(11.5, egui::FontFamily::Monospace),
        ),
    ]
    .into();
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

pub fn title(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(14.0).strong().color(P.text)
}

pub fn body(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(12.5).color(P.text)
}

pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(11.5).color(P.muted)
}

pub fn tiny(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(10.5).color(P.faint)
}

/// The small uppercase label that heads a section in the details panel.
pub fn section(ui: &mut Ui, label: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(label.to_uppercase())
            .size(9.5)
            .color(P.faint)
            .extra_letter_spacing(0.6),
    );
    ui.add_space(4.0);
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// A white card with a hairline border.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(P.surface)
        .stroke(Stroke::new(1.0, P.border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(10))
        .show(ui, add)
        .inner
}

/// A tinted card, used for a classified item (a node, a harness, a status).
pub fn tinted_card<R>(
    ui: &mut Ui,
    fill: Color32,
    border: Color32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(10))
        .show(ui, add)
        .inner
}

// ---------------------------------------------------------------------------
// Small widgets
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Good,
    Warn,
    Bad,
    Info,
}

impl Tone {
    pub fn colours(self) -> (Color32, Color32) {
        match self {
            Tone::Neutral => (P.surface_alt, P.muted),
            Tone::Good => (P.accent_soft, P.accent),
            Tone::Warn => (P.amber_soft, P.amber),
            Tone::Bad => (P.danger_soft, P.danger),
            Tone::Info => (P.blue_soft, P.blue),
        }
    }
}

/// A rounded label. `dot` prefixes a status dot in the tone's colour.
pub fn pill(ui: &mut Ui, text: &str, tone: Tone, dot: bool) -> Response {
    let (bg, fg) = tone.colours();
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                if dot {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 3.5, fg);
                }
                ui.label(egui::RichText::new(text).size(11.0).color(fg));
            });
        })
        .response
}

/// A bordered button that reads as a control, not a link.
pub fn ghost_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).size(12.0).color(if enabled {
            P.text
        } else {
            P.faint
        }))
        .fill(P.surface)
        .stroke(Stroke::new(1.0, P.border))
        .corner_radius(CornerRadius::same(6)),
    )
}

/// The green primary action.
pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(text)
                .size(12.0)
                .color(Color32::WHITE)
                .strong(),
        )
        .fill(if enabled { P.accent } else { P.faint })
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(6)),
    )
}

/// The red outlined action at the foot of the details panel.
pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    ui.add_sized(
        [ui.available_width(), 28.0],
        egui::Button::new(egui::RichText::new(text).size(12.0).color(P.danger))
            .fill(P.surface)
            .stroke(Stroke::new(1.0, P.danger))
            .corner_radius(CornerRadius::same(6)),
    )
}

/// A pill switch. Returns true when the user changed it.
pub fn toggle(ui: &mut Ui, on: &mut bool, enabled: bool) -> bool {
    let size = Vec2::new(34.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let changed = enabled && response.clicked();
    if changed {
        *on = !*on;
    }
    let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
    let bg = if !enabled {
        P.border
    } else if *on {
        P.accent
    } else {
        P.border_strong
    };
    ui.painter().rect_filled(rect, CornerRadius::same(9), bg);
    let knob_x = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), how_on);
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 7.0, Color32::WHITE);
    changed
}

/// A labelled row with a control on the right, as in the details panel.
pub fn row<R>(ui: &mut Ui, label: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label(body(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            out = Some(control(ui));
        });
    });
    out.expect("the control was not run")
}

/// A compact combo box in the house style. Returns the chosen value when changed.
pub fn select(ui: &mut Ui, id: &str, current: &str, options: &[String]) -> Option<String> {
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(egui::RichText::new(current).size(12.0))
        .width(140.0)
        .show_ui(ui, |ui| {
            for option in options {
                if ui
                    .selectable_label(option == current, egui::RichText::new(option).size(12.0))
                    .clicked()
                {
                    chosen = Some(option.clone());
                }
            }
        });
    chosen
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Canvas,
    Agent,
    Tool,
    Model,
    Data,
    History,
    Library,
    Store,
    Settings,
    Help,
    Memory,
    Condition,
    Start,
}

/// Icons are drawn, not typed: no icon font, identical on every platform.
pub fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, colour: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new((s * 0.09).max(1.2), colour);
    let r = s * 0.34;

    match icon {
        Icon::Canvas => {
            let q = s * 0.17;
            let gap = s * 0.06;
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let centre = c + Vec2::new(dx * (q + gap), dy * (q + gap));
                painter.rect_filled(
                    egui::Rect::from_center_size(centre, Vec2::splat(q * 2.0)),
                    CornerRadius::same(2),
                    colour,
                );
            }
        }
        Icon::Agent => {
            painter.circle_stroke(c - Vec2::new(0.0, r * 0.45), r * 0.42, stroke);
            painter.add(egui::Shape::Path(epaint::PathShape {
                points: arc_points(c + Vec2::new(0.0, r * 0.75), r * 0.85, 180.0, 360.0),
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: stroke.into(),
            }));
        }
        Icon::Tool => {
            painter.line_segment([c - Vec2::splat(r * 0.7), c + Vec2::splat(r * 0.7)], stroke);
            painter.line_segment(
                [
                    c + Vec2::new(-r * 0.7, r * 0.7),
                    c + Vec2::new(r * 0.7, -r * 0.7),
                ],
                stroke,
            );
            painter.circle_filled(c, r * 0.22, colour);
        }
        Icon::Model => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::splat(r * 1.7)),
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(c, Vec2::splat(r * 0.7)),
                CornerRadius::same(2),
                colour,
            );
        }
        Icon::Data => {
            for i in -1..=1 {
                let y = c.y + i as f32 * r * 0.62;
                painter.rect_stroke(
                    egui::Rect::from_center_size(egui::pos2(c.x, y), Vec2::new(r * 1.8, r * 0.42)),
                    CornerRadius::same(2),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::History => {
            painter.circle_stroke(c, r * 0.85, stroke);
            painter.line_segment([c, c + Vec2::new(0.0, -r * 0.55)], stroke);
            painter.line_segment([c, c + Vec2::new(r * 0.42, 0.0)], stroke);
        }
        Icon::Library => {
            for (i, h) in [0.7_f32, 1.0, 0.85].into_iter().enumerate() {
                let x = c.x + (i as f32 - 1.0) * r * 0.66;
                painter.rect_stroke(
                    egui::Rect::from_center_size(
                        egui::pos2(x, c.y + r * (1.0 - h) * 0.5),
                        Vec2::new(r * 0.44, r * 1.7 * h),
                    ),
                    CornerRadius::same(2),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Store => {
            // An awning over a shopfront.
            let body = egui::Rect::from_center_size(
                c + Vec2::new(0.0, r * 0.35),
                Vec2::new(r * 1.7, r * 1.1),
            );
            painter.rect_stroke(
                body,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 1.0, c.y - r * 0.35),
                    egui::pos2(c.x + r * 1.0, c.y - r * 0.35),
                ],
                stroke,
            );
            for i in -1..=1 {
                let x = c.x + i as f32 * r * 0.66;
                painter.line_segment(
                    [egui::pos2(x, c.y - r * 0.35), egui::pos2(x, c.y - r * 0.95)],
                    stroke,
                );
            }
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 1.0, c.y - r * 0.95),
                    egui::pos2(c.x + r * 1.0, c.y - r * 0.95),
                ],
                stroke,
            );
        }
        Icon::Settings => {
            painter.circle_stroke(c, r * 0.5, stroke);
            for i in 0..6 {
                let a = std::f32::consts::TAU * i as f32 / 6.0;
                let dir = Vec2::new(a.cos(), a.sin());
                painter.line_segment([c + dir * r * 0.72, c + dir * r * 1.0], stroke);
            }
        }
        Icon::Help => {
            painter.circle_stroke(c, r * 0.85, stroke);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(s * 0.5),
                colour,
            );
        }
        Icon::Memory => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::splat(r * 1.5)),
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            for i in -1..=1 {
                let x = c.x + i as f32 * r * 0.5;
                painter.line_segment(
                    [egui::pos2(x, c.y - r * 1.05), egui::pos2(x, c.y - r * 0.75)],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(x, c.y + r * 0.75), egui::pos2(x, c.y + r * 1.05)],
                    stroke,
                );
            }
        }
        Icon::Condition => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -r),
                    c + Vec2::new(r, 0.0),
                    c + Vec2::new(0.0, r),
                    c + Vec2::new(-r, 0.0),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::Start => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(-r * 0.5, -r * 0.75),
                    c + Vec2::new(r * 0.8, 0.0),
                    c + Vec2::new(-r * 0.5, r * 0.75),
                ],
                colour,
                Stroke::NONE,
            ));
        }
    }
}

fn arc_points(centre: egui::Pos2, radius: f32, from_deg: f32, to_deg: f32) -> Vec<egui::Pos2> {
    let steps = 16;
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let a = (from_deg + (to_deg - from_deg) * t).to_radians();
            centre + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// A small rounded square holding an icon, as on every node card.
pub fn icon_chip(ui: &mut Ui, icon: Icon, fg: Color32, bg: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(5), bg);
    draw_icon(ui.painter(), rect.shrink(size * 0.26), icon, fg);
}
