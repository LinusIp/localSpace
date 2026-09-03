//! The `widgets` surface kind: a declarative tree the Client renders in the host
//! theme.
//!
//! Cheap to author, native-looking everywhere, and the only surface an MCP-only
//! harness needs. Events come back as `(id, value)` pairs and go to the harness's
//! logic like any other surface message.

use localspace_proto as proto;

/// Render a widget tree, collecting whatever the user touched.
pub fn show(ui: &mut egui::Ui, root: &proto::Widget, out: &mut Vec<proto::WidgetEvent>) {
    render(ui, root, out);
}

fn render(ui: &mut egui::Ui, w: &proto::Widget, out: &mut Vec<proto::WidgetEvent>) {
    use proto::Widget as W;
    match w {
        W::Column { children } => {
            ui.vertical(|ui| {
                for c in children {
                    render(ui, c, out);
                }
            });
        }
        W::Row { children } => {
            ui.horizontal_wrapped(|ui| {
                for c in children {
                    render(ui, c, out);
                }
            });
        }
        W::Text {
            text,
            strong,
            muted,
        } => {
            let mut rich = egui::RichText::new(text);
            if *strong {
                rich = rich.strong();
            }
            if *muted {
                rich = rich.weak();
            }
            ui.label(rich);
        }
        W::Heading { text } => {
            ui.heading(text);
        }
        W::Separator => {
            ui.separator();
        }
        W::Space { size } => {
            ui.add_space(*size);
        }
        W::Button { id, label, enabled } => {
            if ui
                .add_enabled(*enabled, egui::Button::new(label))
                .clicked()
            {
                out.push(proto::WidgetEvent {
                    id: id.clone(),
                    value: proto::WidgetValue::Clicked,
                });
            }
        }
        W::Input {
            id,
            label,
            value,
            multiline,
        } => {
            ui.horizontal(|ui| {
                if !label.is_empty() {
                    ui.label(label);
                }
                let mut text = value.clone();
                let response = if *multiline {
                    ui.text_edit_multiline(&mut text)
                } else {
                    ui.text_edit_singleline(&mut text)
                };
                if response.changed() {
                    out.push(proto::WidgetEvent {
                        id: id.clone(),
                        value: proto::WidgetValue::Text(text),
                    });
                }
            });
        }
        W::Checkbox { id, label, value } => {
            let mut v = *value;
            if ui.checkbox(&mut v, label).changed() {
                out.push(proto::WidgetEvent {
                    id: id.clone(),
                    value: proto::WidgetValue::Bool(v),
                });
            }
        }
        W::Select {
            id,
            label,
            value,
            options,
        } => {
            ui.horizontal(|ui| {
                if !label.is_empty() {
                    ui.label(label);
                }
                egui::ComboBox::from_id_salt(id)
                    .selected_text(value)
                    .show_ui(ui, |ui| {
                        for option in options {
                            if ui
                                .selectable_label(option == value, option)
                                .clicked()
                            {
                                out.push(proto::WidgetEvent {
                                    id: id.clone(),
                                    value: proto::WidgetValue::Text(option.clone()),
                                });
                            }
                        }
                    });
            });
        }
        W::Slider {
            id,
            label,
            value,
            min,
            max,
        } => {
            let mut v = *value;
            if ui
                .add(egui::Slider::new(&mut v, *min..=*max).text(label))
                .changed()
            {
                out.push(proto::WidgetEvent {
                    id: id.clone(),
                    value: proto::WidgetValue::Number(v),
                });
            }
        }
        W::List { items } => {
            for item in items {
                ui.label(format!("• {item}"));
            }
        }
        W::Table { headers, rows } => {
            egui::Grid::new(("table", headers.len(), rows.len()))
                .striped(true)
                .show(ui, |ui| {
                    for h in headers {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(cell);
                        }
                        ui.end_row();
                    }
                });
        }
        W::Badge { text, tone } => {
            let (bg, fg) = match tone {
                proto::Tone::Neutral => (egui::Color32::from_gray(70), egui::Color32::WHITE),
                proto::Tone::Good => (egui::Color32::from_rgb(28, 92, 48), egui::Color32::WHITE),
                proto::Tone::Warn => (egui::Color32::from_rgb(120, 90, 20), egui::Color32::WHITE),
                proto::Tone::Bad => (egui::Color32::from_rgb(120, 40, 40), egui::Color32::WHITE),
            };
            egui::Frame::new()
                .fill(bg)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).color(fg).small());
                });
        }
    }
}
