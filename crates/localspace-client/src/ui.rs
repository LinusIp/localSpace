//! The Client shell.
//!
//! Top bar, an icon rail, a canvas, a floating composer, a details panel and a
//! status bar. The rail switches what the canvas shows; the details panel is
//! always about whatever the canvas has focused.
//!
//! Two things here are load-bearing rather than decorative: the network mode is
//! visible at all times (a user in a bank has to know at a glance whether this
//! environment can reach the internet), and the status bar states plainly that
//! nothing is reported anywhere.

use crate::theme::{self, Icon, Tone, P};
use crate::{App, OpenSurface, RailTab};
use egui::{Color32, CornerRadius, Margin, Stroke, Vec2};
use localspace_proto as proto;

const RAIL: [(RailTab, Icon, &str); 8] = [
    (RailTab::Canvas, Icon::Canvas, "Canvas"),
    (RailTab::Agent, Icon::Agent, "Agent"),
    (RailTab::Tools, Icon::Tool, "Tools"),
    (RailTab::Models, Icon::Model, "Models"),
    (RailTab::Data, Icon::Data, "Data"),
    (RailTab::History, Icon::History, "History"),
    (RailTab::Library, Icon::Library, "Library"),
    (RailTab::Marketplace, Icon::Store, "Market"),
];

impl App {
    // -----------------------------------------------------------------------
    // Top bar
    // -----------------------------------------------------------------------

    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("topbar")
            .resizable(false)
            .exact_size(theme::TOP_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(P.surface)
                    .inner_margin(Margin::symmetric(14, 8)),
            )
            .show(ui, |ui| {
                let line_y = ui.max_rect().bottom() + 8.0;
                ui.painter().hline(
                    ui.max_rect().x_range().expand(14.0),
                    line_y,
                    Stroke::new(1.0, P.border),
                );

                ui.horizontal_centered(|ui| {
                    self.wordmark(ui);
                    ui.add_space(8.0);

                    let workspace = self
                        .env
                        .as_ref()
                        .map(|e| e.workspace.clone())
                        .unwrap_or_else(|| "Workspace".into());
                    ui.menu_button(
                        egui::RichText::new(format!("{workspace}")).size(12.0),
                        |ui| {
                            ui.label(theme::muted(
                                "A workspace is the unit of access control, quota and audit.",
                            ));
                            if let Some(env) = &self.env {
                                ui.label(theme::muted(format!("signed in as {}", env.user)));
                            }
                        },
                    );

                    ui.add_space(16.0);
                    if theme::ghost_button(ui, "Undo", true).clicked() {
                        self.send(proto::Request::Undo);
                    }
                    if theme::ghost_button(ui, "Redo", true).clicked() {
                        self.send(proto::Request::Redo);
                    }

                    ui.add_space(12.0);
                    if theme::ghost_button(ui, "−", self.zoom > 25).clicked() {
                        self.set_zoom(self.zoom - 10);
                    }
                    ui.label(theme::body(format!("{}%", self.zoom)));
                    if theme::ghost_button(ui, "+", self.zoom < 400).clicked() {
                        self.set_zoom(self.zoom + 10);
                    }
                    if theme::ghost_button(ui, "Fit", true).clicked() {
                        self.set_zoom(100);
                        self.surface_command(serde_json::json!({"kind": "fit"}));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.avatar(ui);
                        ui.add_space(4.0);
                        if theme::ghost_button(ui, "Settings", true).clicked() {
                            self.rail = RailTab::Settings;
                        }
                        ui.add_space(2.0);
                        self.ready_pill(ui);
                        ui.add_space(2.0);
                        self.network_menu(ui);
                    });
                });
            });
    }

    fn wordmark(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
        ui.painter()
            .circle_stroke(rect.center(), 9.0, Stroke::new(2.0, P.accent));
        ui.painter().circle_filled(rect.center(), 3.5, P.accent);
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("localSpace")
                .size(15.0)
                .strong()
                .color(P.text),
        );
    }

    fn avatar(&self, ui: &mut egui::Ui) {
        let initial = self
            .env
            .as_ref()
            .and_then(|e| e.user.chars().next())
            .unwrap_or('?')
            .to_ascii_uppercase();
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 12.0, P.accent_soft);
        ui.painter()
            .circle_stroke(rect.center(), 12.0, Stroke::new(1.0, P.border));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(12.0),
            P.accent,
        );
    }

    fn ready_pill(&self, ui: &mut egui::Ui) {
        let (text, tone) = if self.busy {
            ("Working", Tone::Warn)
        } else {
            ("Ready", Tone::Good)
        };
        theme::pill(ui, text, tone, true);
    }

    fn network_menu(&mut self, ui: &mut egui::Ui) {
        let (label, tip) = match self.env.as_ref().map(|e| e.network) {
            Some(proto::NetworkMode::Airgapped) => (
                "airgapped",
                "No egress at all. The agent has no web tools this turn.",
            ),
            Some(proto::NetworkMode::Ask) => (
                "ask",
                "The agent may search and fetch, with your approval per domain.",
            ),
            Some(proto::NetworkMode::Online) => (
                "online",
                "Egress without prompting, still allowlisted and fully logged.",
            ),
            None => ("connecting", "Waiting for Core."),
        };
        let ceiling = self
            .env
            .as_ref()
            .map(|e| e.network_ceiling)
            .unwrap_or(proto::NetworkMode::Ask);

        ui.menu_button(
            egui::RichText::new(format!("network: {label}")).size(11.0),
            |ui| {
                ui.label(theme::muted(tip));
                ui.separator();
                for mode in [
                    proto::NetworkMode::Airgapped,
                    proto::NetworkMode::Ask,
                    proto::NetworkMode::Online,
                ] {
                    let allowed = mode.rank() <= ceiling.rank();
                    if ui
                        .add_enabled(allowed, egui::Button::new(theme::body(mode.label())))
                        .clicked()
                    {
                        self.send(proto::Request::SetNetworkMode { mode });
                        ui.close();
                    }
                }
                if ceiling.rank() < proto::NetworkMode::Online.rank() {
                    ui.label(theme::tiny(format!(
                        "the administrator's ceiling is `{}`",
                        ceiling.label()
                    )));
                }
            },
        )
        .response
        .on_hover_text(tip);
    }

    // -----------------------------------------------------------------------
    // Rail
    // -----------------------------------------------------------------------

    pub(crate) fn rail(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("rail")
            .resizable(false)
            .exact_size(theme::RAIL_WIDTH)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(P.rail)
                    .inner_margin(Margin::symmetric(6, 8)),
            )
            .show(ui, |ui| {
                ui.painter().vline(
                    ui.max_rect().right() + 6.0,
                    ui.max_rect().y_range().expand(8.0),
                    Stroke::new(1.0, P.border),
                );
                let bottom_block = 2.0 * 54.0;
                ui.vertical_centered(|ui| {
                    for (tab, icon, label) in RAIL {
                        self.rail_item(ui, tab, icon, label);
                    }
                    let room = ui.available_height() - bottom_block;
                    if room > 0.0 {
                        ui.add_space(room);
                    }
                    self.rail_item(ui, RailTab::Settings, Icon::Settings, "Settings");
                    self.rail_item(ui, RailTab::Help, Icon::Help, "Help");
                });
            });
    }

    fn rail_item(&mut self, ui: &mut egui::Ui, tab: RailTab, icon: Icon, label: &str) {
        let selected = self.rail == tab;
        let size = Vec2::new(theme::RAIL_WIDTH - 14.0, 50.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), P.surface);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(8),
                Stroke::new(1.0, P.border),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), P.surface_alt);
        }

        let colour = if selected { P.accent } else { P.muted };
        let icon_rect =
            egui::Rect::from_center_size(rect.center() - Vec2::new(0.0, 8.0), Vec2::splat(19.0));
        theme::draw_icon(ui.painter(), icon_rect, icon, colour);
        ui.painter().text(
            rect.center() + Vec2::new(0.0, 15.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(9.5),
            colour,
        );

        if response.clicked() {
            self.rail = tab;
            if tab == RailTab::History {
                self.send(proto::Request::GetHistory { limit: 50 });
            }
            if tab == RailTab::Tools {
                self.send(proto::Request::GetActiveSet);
            }
            if tab == RailTab::Marketplace {
                self.send(proto::Request::ListCatalog);
            }
            if tab == RailTab::Agent {
                self.send(proto::Request::GetTask);
            }
        }
        ui.add_space(2.0);
    }

    // -----------------------------------------------------------------------
    // Status bar
    // -----------------------------------------------------------------------

    pub(crate) fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .resizable(false)
            .exact_size(theme::STATUS_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(P.surface)
                    .inner_margin(Margin::symmetric(14, 5)),
            )
            .show(ui, |ui| {
                ui.painter().hline(
                    ui.max_rect().x_range().expand(14.0),
                    ui.max_rect().top() - 5.0,
                    Stroke::new(1.0, P.border),
                );
                ui.horizontal_centered(|ui| {
                    let (running, detail) = self
                        .env
                        .as_ref()
                        .map(|e| (e.engine.running, e.engine.detail.clone()))
                        .unwrap_or((false, "connecting".into()));

                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        rect.center(),
                        4.0,
                        if running { P.accent } else { P.faint },
                    );
                    ui.label(theme::muted("Local AI Engine:"));
                    ui.label(
                        egui::RichText::new(if running { "running" } else { "not loaded" })
                            .size(11.5)
                            .color(if running { P.accent } else { P.faint }),
                    )
                    .on_hover_text(detail);

                    ui.add_space(12.0);
                    ui.label(theme::muted("No telemetry")).on_hover_text(
                        "Nothing leaves this machine except through the gateway, and only when you allow it.",
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(env) = &self.env {
                            ui.label(theme::muted(format!("Workspace: {}", env.workspace)));
                            ui.add_space(12.0);
                            ui.label(theme::muted(env.profile.clone())).on_hover_text(
                                "Every budget in this environment comes from this profile.",
                            );
                            ui.add_space(12.0);
                            ui.label(theme::muted(env.machine.clone()));
                        }
                    });
                });
            });
    }

    // -----------------------------------------------------------------------
    // Details panel
    // -----------------------------------------------------------------------

    pub(crate) fn details(&mut self, ui: &mut egui::Ui) {
        if !self.details_open {
            return;
        }
        egui::Panel::right("details")
            .default_size(theme::DETAILS_WIDTH)
            .min_size(280.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(P.surface)
                    .inner_margin(Margin::symmetric(14, 12)),
            )
            .show(ui, |ui| {
                ui.painter().vline(
                    ui.max_rect().left() - 14.0,
                    ui.max_rect().y_range().expand(12.0),
                    Stroke::new(1.0, P.border),
                );

                let title = self.details_title();
                ui.horizontal(|ui| {
                    ui.label(theme::title(title));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(theme::muted("×"))
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(Stroke::NONE),
                            )
                            .clicked()
                        {
                            self.details_open = false;
                        }
                    });
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.rail {
                        RailTab::Tools => self.details_tools(ui),
                        RailTab::Models => self.details_model(ui),
                        RailTab::Settings => self.details_environment(ui),
                        _ => self.details_harness(ui),
                    });
            });
    }

    fn details_title(&self) -> &'static str {
        match self.rail {
            RailTab::Tools => "Tool set",
            RailTab::Models => "Model",
            RailTab::Settings => "Environment",
            _ => "Harness details",
        }
    }

    fn details_harness(&mut self, ui: &mut egui::Ui) {
        let Some(env) = self.env.clone() else { return };
        let Some(h) = env
            .focus
            .as_ref()
            .and_then(|f| env.harnesses.iter().find(|h| &h.id == f))
            .cloned()
        else {
            ui.label(theme::muted("No harness is focused."));
            return;
        };

        let (fg, bg) = match h.tier {
            proto::Tier::Wasm => (P.purple, P.purple_soft),
            proto::Tier::Native => (P.amber, P.amber_soft),
        };
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                theme::icon_chip(ui, Icon::Canvas, fg, bg, 30.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(match h.tier {
                            proto::Tier::Wasm => "TIER A · WASM",
                            proto::Tier::Native => "TIER B · NATIVE",
                        })
                        .size(9.5)
                        .color(fg),
                    );
                    ui.label(theme::title(h.title.clone()));
                });
            });
            ui.add_space(4.0);
            ui.label(theme::muted(format!("{} · {}", h.id, h.version)));
        });

        theme::section(ui, "Description");
        ui.label(theme::body(format!(
            "{} tools, {} of them front door. {} a context provider. Its document is {}.",
            h.tool_count,
            h.front_door.len(),
            if h.has_context_provider {
                "Ships"
            } else {
                "Ships no"
            },
            match h.doc_kind {
                proto::DocKind::Crdt => "a CRDT, so undo and multi-user editing come free",
                proto::DocKind::Blob => "an opaque blob, snapshotted before each write",
            }
        )));
        if let Some(note) = &h.degraded {
            ui.add_space(6.0);
            theme::pill(ui, note, Tone::Warn, false);
        }

        theme::section(ui, "Quick actions");
        ui.label(theme::tiny(
            "Front-door tools — the ones the agent can reach even when this harness is not focused.",
        ));
        ui.add_space(4.0);
        let mut invoke: Option<String> = None;
        for tool in &h.front_door {
            if theme::ghost_button(ui, tool, true).clicked() {
                invoke = Some(tool.clone());
            }
        }
        if let Some(tool) = invoke {
            self.run_front_door(&tool);
        }

        theme::section(ui, "Capabilities");
        let c = h.capabilities.clone();
        for (label, value, granted) in [
            ("Filesystem", c.fs.clone(), c.fs != "none"),
            ("Network", c.net.clone(), c.net != "none"),
            ("GPU", c.gpu.clone(), c.gpu != "none"),
            ("Documents", c.docs.clone(), c.docs != "none"),
            (
                "Inference",
                if c.model.is_empty() {
                    "none".into()
                } else {
                    c.model.join(", ")
                },
                !c.model.is_empty(),
            ),
        ] {
            theme::row(ui, label, |ui| {
                let mut on = granted;
                theme::toggle(ui, &mut on, false);
                ui.label(theme::muted(value));
            });
        }
        ui.add_space(2.0);
        ui.label(theme::tiny(
            "Read-only: these are what the package declared and policy allowed. Anything not declared is unavailable.",
        ));

        theme::section(ui, "Evaluation");
        if theme::ghost_button(ui, "Run the agent-compatibility suite", true).clicked() {
            self.send(proto::Request::RunEvals {
                harness: h.id.clone(),
            });
        }

        ui.add_space(14.0);
        if theme::danger_button(ui, "Uninstall harness").clicked() {
            self.send(proto::Request::UninstallHarness {
                harness: h.id.clone(),
            });
        }
    }

    fn details_tools(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active.clone() else {
            ui.label(theme::muted("No active set yet."));
            return;
        };
        theme::card(ui, |ui| {
            theme::row(ui, "In context", |ui| {
                ui.label(theme::body(format!("{} tools", active.tools.len())))
            });
            theme::row(ui, "Token budget", |ui| {
                ui.label(theme::body(format!(
                    "{} / {}",
                    active.token_estimate, active.budget
                )))
            });
            theme::row(ui, "Grammar", |ui| {
                ui.label(theme::muted(active.grammar_hash.clone()))
            });
        });
        if !active.dropped.is_empty() {
            ui.add_space(6.0);
            theme::pill(
                ui,
                &format!("over budget: dropped {}", active.dropped.join(", ")),
                Tone::Warn,
                false,
            );
        }
        theme::section(ui, "Why these");
        ui.label(theme::body(
            "The focused harness contributes all of its tools; pinned and touched harnesses \
             contribute their front doors only. Everything else is reachable through \
             find_capability, which promotes its harness on the next turn.",
        ));
    }

    fn details_model(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Endpoint");
        ui.add(
            egui::TextEdit::singleline(&mut self.model_endpoint)
                .desired_width(f32::INFINITY)
                .hint_text("http://localhost:1234/v1"),
        );
        theme::section(ui, "Model id");
        ui.add(
            egui::TextEdit::singleline(&mut self.model_name)
                .desired_width(f32::INFINITY)
                .hint_text("the id the endpoint reports"),
        );
        ui.add_space(8.0);
        if theme::primary_button(ui, "Use this model", !self.model_name.trim().is_empty()).clicked()
        {
            self.send(proto::Request::SelectModel {
                id: format!("{}|{}", self.model_endpoint, self.model_name),
            });
        }
        ui.add_space(6.0);
        ui.label(theme::tiny(
            "Any OpenAI-compatible endpoint: llama.cpp's server, LM Studio, mistral.rs, vLLM, SGLang.",
        ));

        if let Some(env) = self.env.clone() {
            theme::section(ui, "Profile");
            ui.label(theme::body(env.profile));
            ui.label(theme::muted(env.machine));
        }
    }

    fn details_environment(&mut self, ui: &mut egui::Ui) {
        let Some(env) = self.env.clone() else { return };
        theme::section(ui, "Network");
        ui.label(theme::body(format!("mode: {}", env.network.label())));
        ui.label(theme::muted(format!(
            "administrator ceiling: {}",
            env.network_ceiling.label()
        )));
        theme::section(ui, "Topology");
        ui.label(theme::body(match env.topology {
            proto::Topology::Personal => "personal — Core and Client in one process",
            proto::Topology::Organisation => "organisation — Core on the server",
        }));
        theme::section(ui, "Native harnesses");
        theme::row(ui, "Tier B permitted", |ui| {
            let mut on = env.tier_b_permitted;
            theme::toggle(ui, &mut on, false);
        });
        ui.label(theme::tiny(
            "Tier B runs outside the wasm sandbox and needs an approved reason per package.",
        ));
    }

    // -----------------------------------------------------------------------
    // Canvas and pages
    // -----------------------------------------------------------------------

    pub(crate) fn central(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(P.page).inner_margin(Margin::same(0)))
            .show(ui, |ui| {
                self.canvas_header(ui);
                let area = ui.available_rect_before_wrap();
                match self.rail {
                    RailTab::Canvas => self.canvas(ui, area),
                    RailTab::Agent => self.agent_page(ui),
                    RailTab::Tools => self.tools_page(ui),
                    RailTab::Models => self.models_page(ui),
                    RailTab::Data => self.data_page(ui),
                    RailTab::History => self.history_page(ui),
                    RailTab::Library => self.library_page(ui),
                    RailTab::Marketplace => self.marketplace_page(ui),
                    RailTab::Settings => self.settings_page(ui),
                    RailTab::Help => self.help_page(ui),
                }
            });

        if matches!(self.rail, RailTab::Canvas | RailTab::Agent) {
            self.composer(ui.ctx());
        }
    }

    fn canvas_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(P.page)
            .inner_margin(Margin::symmetric(16, 9))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let title = self.page_title();
                    ui.label(theme::title(title));
                    ui.add_space(8.0);
                    if self.rail == RailTab::Canvas {
                        if self.dirty_since_commit {
                            theme::pill(ui, "unsaved", Tone::Warn, true);
                        } else {
                            theme::pill(ui, "Auto-saved", Tone::Good, true);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.details_open && theme::ghost_button(ui, "Details", true).clicked() {
                            self.details_open = true;
                        }
                        let branch = self
                            .history
                            .first()
                            .and_then(|c| c.run.clone())
                            .map(|r| r.chars().take(12).collect::<String>())
                            .unwrap_or_else(|| "main".into());
                        ui.menu_button(
                            egui::RichText::new(format!("Branch: {branch}")).size(11.5),
                            |ui| {
                                ui.label(theme::muted(
                                    "Every agent run is a branch. Discard one from History.",
                                ));
                            },
                        );
                    });
                });
            });
    }

    fn page_title(&self) -> String {
        match self.rail {
            RailTab::Canvas => self
                .env
                .as_ref()
                .and_then(|e| {
                    let focus = e.focus.as_ref()?;
                    e.harnesses
                        .iter()
                        .find(|h| &h.id == focus)
                        .map(|h| h.title.clone())
                })
                .unwrap_or_else(|| "Canvas".into()),
            RailTab::Agent => "Agent".into(),
            RailTab::Tools => "Tools in context".into(),
            RailTab::Models => "Models".into(),
            RailTab::Data => "Documents".into(),
            RailTab::History => "History".into(),
            RailTab::Library => "Library".into(),
            RailTab::Marketplace => "Marketplace".into(),
            RailTab::Settings => "Settings".into(),
            RailTab::Help => "Help".into(),
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, area: egui::Rect) {
        let Some(env) = self.env.clone() else {
            self.empty_state(ui, area, "Connecting to Core…", "");
            return;
        };
        let Some(focus) = env.focus.clone() else {
            self.empty_state(
                ui,
                area,
                "No harness is focused",
                "Open Library and pick one, or ask the agent and it will focus itself.",
            );
            return;
        };
        let Some(h) = env.harnesses.iter().find(|h| h.id == focus).cloned() else {
            return;
        };
        let Some(view) = h
            .views
            .iter()
            .find(|v| v.placement == proto::Placement::Main)
            .cloned()
        else {
            self.empty_state(
                ui,
                area,
                &format!("{} has no canvas", h.title),
                "It contributes tools and context. Talk to the agent instead.",
            );
            return;
        };

        let sheet = egui::Rect::from_min_max(
            area.min + Vec2::new(16.0, 0.0),
            area.max - Vec2::new(16.0, 12.0),
        );
        ui.painter()
            .rect_filled(sheet, CornerRadius::same(10), P.surface);
        ui.painter().rect_stroke(
            sheet,
            CornerRadius::same(10),
            Stroke::new(1.0, P.border),
            egui::StrokeKind::Inside,
        );

        match view.kind {
            proto::SurfaceKind::Widgets => {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(sheet.shrink(12.0))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                self.show_widget_view(&mut child, &focus, &view.id);
            }
            proto::SurfaceKind::Egui => self.show_egui_surface(ui, &focus, &view.id, sheet),
            proto::SurfaceKind::Web => self.empty_state(
                ui,
                sheet,
                "This view runs in the web client",
                "A web view is an iframe on its own origin (architecture v2 §6.3); open it in the browser or the desktop app.",
            ),
            proto::SurfaceKind::Stream => self.empty_state(
                ui,
                sheet,
                "Stream surfaces need a Tier B process",
                "Not built in this configuration — see docs/STATUS.md.",
            ),
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui, area: egui::Rect, title: &str, body: &str) {
        let painter = ui.painter();
        painter.text(
            area.center() - Vec2::new(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(14.0),
            P.muted,
        );
        if !body.is_empty() {
            painter.text(
                area.center() + Vec2::new(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                body,
                egui::FontId::proportional(11.5),
                P.faint,
            );
        }
    }

    // -----------------------------------------------------------------------
    // Composer
    // -----------------------------------------------------------------------

    fn composer(&mut self, ctx: &egui::Context) {
        let screen = ctx.viewport_rect();
        let right = if self.details_open {
            theme::DETAILS_WIDTH + 14.0
        } else {
            0.0
        };
        let usable = screen.width() - theme::RAIL_WIDTH - right;
        let width = 640.0_f32.min(usable - 60.0).max(320.0);
        let left = theme::RAIL_WIDTH + (usable - width) / 2.0;
        let top = screen.bottom() - theme::STATUS_BAR_HEIGHT - 108.0;

        egui::Area::new(egui::Id::new("composer"))
            .fixed_pos(egui::pos2(left, top))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(P.surface)
                    .stroke(Stroke::new(1.0, P.border))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(12, 9))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(16),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.menu_button(egui::RichText::new("You").size(11.5), |ui| {
                                ui.label(theme::muted(
                                    "Your messages are attributed to you; the agent's writes are attributed to its run.",
                                ));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let model = self
                                    .env
                                    .as_ref()
                                    .and_then(|e| e.model.as_ref().map(|m| m.id.clone()))
                                    .unwrap_or_else(|| "no model".into());
                                if ui
                                    .button(egui::RichText::new(format!("{model}")).size(11.5))
                                    .clicked()
                                {
                                    self.rail = RailTab::Models;
                                    self.details_open = true;
                                }
                                let tools = self.active.as_ref().map(|a| a.tools.len()).unwrap_or(0);
                                ui.menu_button(egui::RichText::new("Agent").size(11.5), |ui| {
                                    ui.label(theme::muted(format!(
                                        "{tools} tools in context this turn"
                                    )));
                                });
                            });
                        });

                        ui.add_space(2.0);
                        let mut submit = false;
                        ui.horizontal(|ui| {
                            let response = ui.add_sized(
                                [ui.available_width() - 92.0, 30.0],
                                egui::TextEdit::singleline(&mut self.input)
                                    .frame(egui::Frame::NONE)
                                    .hint_text("Type your message or add instructions…"),
                            );
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                submit = true;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let ready = !self.busy && !self.input.trim().is_empty();
                                if theme::primary_button(ui, "Send", ready).clicked() {
                                    submit = true;
                                }
                            });
                        });
                        if submit && !self.busy && !self.input.trim().is_empty() {
                            let text = std::mem::take(&mut self.input);
                            self.busy = true;
                            self.rail = RailTab::Agent;
                            self.send(proto::Request::SendMessage { text });
                        }
                    });
            });
    }

    // -----------------------------------------------------------------------
    // Pages
    // -----------------------------------------------------------------------

    fn page(&mut self, ui: &mut egui::Ui, add: impl FnOnce(&mut Self, &mut egui::Ui)) {
        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 2))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        add(self, ui);
                        ui.add_space(120.0);
                    });
            });
    }

    /// The task ledger (spec §18.1): what the agent is doing across harnesses,
    /// shown exactly as the model sees it — goal, plan, artifacts, notes.
    fn ledger_card(&mut self, ui: &mut egui::Ui) {
        let Some(task) = self.task.clone() else { return };
        if task.goal.is_empty() && task.plan.is_empty() && task.artifacts.is_empty() {
            return;
        }
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TASK LEDGER").size(9.5).color(P.muted));
                ui.label(theme::tiny(task.id.clone()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(theme::tiny(
                        "in every prompt, whichever harness is focused",
                    ));
                });
            });
            if !task.goal.is_empty() {
                ui.label(theme::body(task.goal.clone()));
            }

            if !task.plan.is_empty() {
                theme::section(ui, "Plan");
                for (i, step) in task.plan.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (label, tone) = match step.status {
                            proto::StepStatus::Pending => ("pending", Tone::Neutral),
                            proto::StepStatus::Active => ("active", Tone::Info),
                            proto::StepStatus::Done => ("done", Tone::Good),
                            proto::StepStatus::Failed => ("failed", Tone::Bad),
                        };
                        theme::pill(ui, label, tone, false);
                        ui.label(theme::body(format!(
                            "{}. {} — {}",
                            i + 1,
                            step.harness,
                            step.intent
                        )));
                    });
                }
            }

            if !task.artifacts.is_empty() {
                theme::section(ui, "Artifacts");
                for a in &task.artifacts {
                    ui.horizontal(|ui| {
                        theme::pill(ui, &a.id, Tone::Info, true);
                        theme::pill(ui, &a.kind, Tone::Neutral, false);
                        ui.label(theme::body(a.summary.clone()));
                    });
                    ui.label(theme::tiny(format!(
                        "from {} · pinned to {}",
                        a.produced_by,
                        if a.commit.is_empty() {
                            "the live document".to_string()
                        } else {
                            a.commit.chars().take(12).collect()
                        }
                    )));
                }
            }

            if !task.notes.is_empty() {
                theme::section(ui, "Notes");
                for n in &task.notes {
                    ui.label(theme::muted(format!("· {n}")));
                }
            }
        });
        ui.add_space(10.0);
    }

    fn agent_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            app.ledger_card(ui);
            if app.transcript.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::muted("Nothing has been said yet."));
                    ui.label(theme::tiny(
                        "Ask for something. Every tool the agent runs appears here with what it changed.",
                    ));
                });
            }
            for m in app.transcript.clone() {
                app.transcript_entry(ui, &m);
            }
        });
    }

    fn transcript_entry(&mut self, ui: &mut egui::Ui, m: &proto::ChatMessage) {
        match m.role {
            proto::Role::User => {
                ui.add_space(8.0);
                theme::tinted_card(ui, P.surface_alt, P.border, |ui| {
                    ui.label(theme::tiny("YOU"));
                    ui.label(theme::body(m.content.clone()));
                });
            }
            proto::Role::Assistant => {
                ui.add_space(8.0);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon_chip(ui, Icon::Agent, P.purple, P.purple_soft, 22.0);
                        ui.label(theme::tiny("AGENT"));
                    });
                    ui.add_space(4.0);
                    ui.label(theme::body(m.content.clone()));
                });
            }
            proto::Role::Tool => {
                for call in &m.tool_calls {
                    ui.add_space(6.0);
                    let (fill, border, colour, label, detail) = match &call.outcome {
                        proto::ToolOutcome::Ok { diff_summary, .. } => {
                            (P.accent_soft, P.accent, P.accent, "ok", diff_summary.clone())
                        }
                        proto::ToolOutcome::Denied { reason } => {
                            (P.amber_soft, P.amber, P.amber, "denied", reason.clone())
                        }
                        proto::ToolOutcome::Error { message } => {
                            (P.danger_soft, P.danger, P.danger, "failed", message.clone())
                        }
                        proto::ToolOutcome::AwaitingConfirm { prompt } => (
                            P.amber_soft,
                            P.amber,
                            P.amber,
                            "needs you",
                            prompt.clone(),
                        ),
                        proto::ToolOutcome::Queued { job } => (
                            P.blue_soft,
                            P.blue,
                            P.blue,
                            "queued",
                            format!("job {job}"),
                        ),
                    };
                    theme::tinted_card(ui, fill, border, |ui| {
                        ui.horizontal(|ui| {
                            theme::icon_chip(ui, Icon::Tool, colour, P.surface, 22.0);
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(label).size(9.5).color(colour));
                                ui.label(theme::body(call.tool.clone()).strong());
                            });
                        });
                        ui.label(theme::muted(detail));
                    });
                }
            }
            proto::Role::System => {}
        }
    }

    fn tools_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            let Some(active) = app.active.clone() else {
                ui.label(theme::muted("No active set yet."));
                return;
            };
            ui.label(theme::muted(format!(
                "{} tools, about {} of {} tokens. Emitted in canonical order by harness id, so the \
                 prompt prefix survives a focus change.",
                active.tools.len(),
                active.token_estimate,
                active.budget
            )));
            ui.add_space(6.0);

            for t in &active.tools {
                let (fg, bg) = match t.kind {
                    proto::ToolKind::Read => (P.blue, P.blue_soft),
                    proto::ToolKind::Write => (P.amber, P.amber_soft),
                    proto::ToolKind::Compute => (P.purple, P.purple_soft),
                };
                ui.add_space(6.0);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon_chip(ui, Icon::Tool, fg, bg, 26.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(match t.kind {
                                        proto::ToolKind::Read => "READ",
                                        proto::ToolKind::Write => "WRITE",
                                        proto::ToolKind::Compute => "COMPUTE",
                                    })
                                    .size(9.5)
                                    .color(fg),
                                );
                                theme::pill(
                                    ui,
                                    match t.reason {
                                        proto::ExposureReason::Focused => "focused",
                                        proto::ExposureReason::Pinned => "pinned",
                                        proto::ExposureReason::Touched => "touched",
                                        proto::ExposureReason::CoreBuiltin => "core",
                                    },
                                    Tone::Neutral,
                                    false,
                                );
                            });
                            ui.label(theme::body(t.name.clone()).strong());
                            ui.label(theme::muted(t.summary.clone()));
                        });
                    });
                });
            }
        });
    }

    fn models_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            let Some(env) = app.env.clone() else { return };
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::icon_chip(ui, Icon::Model, P.amber, P.amber_soft, 30.0);
                    ui.vertical(|ui| {
                        ui.label(theme::tiny("CHAT WORKER"));
                        match &env.model {
                            Some(m) => {
                                ui.label(theme::title(m.id.clone()));
                                ui.label(theme::muted(m.backend.clone()));
                            }
                            None => {
                                ui.label(theme::title("No model selected"));
                                ui.label(theme::muted(
                                    "Open Details and point the Client at an OpenAI-compatible endpoint.",
                                ));
                            }
                        }
                    });
                });
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(theme::tiny("THIS MACHINE"));
                ui.label(theme::body(env.machine.clone()));
                ui.add_space(6.0);
                ui.label(theme::tiny("PROFILE"));
                ui.label(theme::body(env.profile.clone()));
                ui.label(theme::muted(
                    "Tool budget, working set and context budgets all come from the profile, \
                     never from a constant.",
                ));
            });
        });
    }

    fn data_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            let Some(env) = app.env.clone() else { return };
            ui.label(theme::muted(
                "One document per harness. Core holds the authoritative copy; surfaces hold replicas.",
            ));
            for h in &env.harnesses {
                ui.add_space(6.0);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon_chip(ui, Icon::Data, P.blue, P.blue_soft, 26.0);
                        ui.vertical(|ui| {
                            ui.label(theme::body(h.title.clone()).strong());
                            ui.label(theme::muted(match h.doc_kind {
                                proto::DocKind::Crdt => {
                                    "crdt — automerge, granular diffs, free undo"
                                }
                                proto::DocKind::Blob => {
                                    "blob — content-addressed, snapshot per write"
                                }
                            }));
                        });
                    });
                });
            }
        });
    }

    fn history_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            if app.history.is_empty() {
                ui.label(theme::muted("Nothing has been committed yet."));
                return;
            }
            let mut drop_run: Option<String> = None;
            for c in app.history.clone() {
                let (fg, bg, who) = match c.author {
                    proto::Author::User => (P.blue, P.blue_soft, "YOU"),
                    proto::Author::Agent => (P.purple, P.purple_soft, "AGENT"),
                    proto::Author::Harness => (P.amber, P.amber_soft, "HARNESS"),
                };
                ui.add_space(6.0);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon_chip(ui, Icon::History, fg, bg, 26.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(who).size(9.5).color(fg));
                                ui.label(theme::tiny(c.tool.clone()));
                            });
                            ui.label(theme::body(c.diff_summary.clone()));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(run) = &c.run {
                                if theme::ghost_button(ui, "Discard run", true).clicked() {
                                    drop_run = Some(run.clone());
                                }
                            }
                        });
                    });
                });
            }
            if let Some(run) = drop_run {
                app.send(proto::Request::DropRun { run });
            }
        });
    }

    fn library_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            let Some(env) = app.env.clone() else { return };
            if env.harnesses.is_empty() {
                ui.label(theme::muted(
                    "No harnesses installed. Start localspace with --harnesses <dir>.",
                ));
                return;
            }
            for h in &env.harnesses {
                let focused = env.focus.as_deref() == Some(h.id.as_str());
                let pinned = env.pinned.iter().any(|p| *p == h.id);
                let (fg, bg) = match h.tier {
                    proto::Tier::Wasm => (P.purple, P.purple_soft),
                    proto::Tier::Native => (P.amber, P.amber_soft),
                };
                ui.add_space(8.0);
                theme::tinted_card(
                    ui,
                    if focused { bg } else { P.surface },
                    if focused { fg } else { P.border },
                    |ui| {
                        ui.horizontal(|ui| {
                            theme::icon_chip(ui, Icon::Library, fg, P.surface, 30.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(match h.tier {
                                        proto::Tier::Wasm => "TIER A · WASM",
                                        proto::Tier::Native => "TIER B · NATIVE",
                                    })
                                    .size(9.5)
                                    .color(fg),
                                );
                                ui.label(theme::title(h.title.clone()));
                                ui.label(theme::muted(format!(
                                    "{} · {} · {} tools, {} front door",
                                    h.publisher,
                                    h.version,
                                    h.tool_count,
                                    h.front_door.len()
                                )));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let mut enabled = h.enabled;
                                if theme::toggle(ui, &mut enabled, true) {
                                    app.send(proto::Request::SetHarnessEnabled {
                                        harness: h.id.clone(),
                                        enabled,
                                    });
                                }
                                if theme::ghost_button(ui, if pinned { "Unpin" } else { "Pin" }, true)
                                    .clicked()
                                {
                                    app.send(proto::Request::SetPinned {
                                        harness: h.id.clone(),
                                        pinned: !pinned,
                                    });
                                }
                                if theme::ghost_button(ui, "Open", !focused).clicked() {
                                    app.send(proto::Request::SetFocus {
                                        harness: Some(h.id.clone()),
                                    });
                                    app.rail = RailTab::Canvas;
                                }
                            });
                        });
                    },
                );
            }
        });
    }

    /// The store page.
    ///
    /// Everything a reader needs to decide *before* the code runs: what it does,
    /// who published it, what it would be allowed to do in plain language, whether
    /// it runs outside the sandbox and why, and how many eval cases it ships.
    fn marketplace_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            if app.catalog.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::muted("No catalog is configured."));
                    ui.label(theme::tiny(
                        "Point localspace at a bundle with --registry <dir>. An offline bundle is \
                         a directory of packages, and is the only path an air-gapped site needs.",
                    ));
                });
                return;
            }

            let installed = app.catalog.iter().filter(|e| e.installed).count();
            ui.label(theme::muted(format!(
                "{} package(s) in the catalog, {installed} already installed. Nothing is \
                 downloaded: these are read from a local bundle.",
                app.catalog.len()
            )));

            let mut install: Option<String> = None;
            let mut uninstall: Option<String> = None;
            let mut run_evals: Option<String> = None;

            for entry in &app.catalog {
                let (fg, bg) = match entry.tier {
                    proto::Tier::Wasm => (P.purple, P.purple_soft),
                    proto::Tier::Native => (P.amber, P.amber_soft),
                };
                ui.add_space(10.0);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::icon_chip(ui, Icon::Store, fg, bg, 34.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(match entry.tier {
                                        proto::Tier::Wasm => "TIER A · WASM",
                                        proto::Tier::Native => "TIER B · NATIVE",
                                    })
                                    .size(9.5)
                                    .color(fg),
                                );
                                if entry.kind != "harness" {
                                    theme::pill(ui, &entry.kind, Tone::Neutral, false);
                                }
                                if entry.installed {
                                    theme::pill(ui, "installed", Tone::Good, true);
                                }
                                if !entry.widens.is_empty() {
                                    theme::pill(ui, "wants more access", Tone::Warn, false);
                                }
                                if entry.blocked.is_some() {
                                    theme::pill(ui, "not installable here", Tone::Bad, false);
                                }
                            });
                            ui.label(theme::title(entry.title.clone()));
                            ui.label(theme::muted(format!(
                                "{} · {} · from {}",
                                entry.publisher, entry.version, entry.source
                            )));
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(why) = &entry.blocked {
                                ui.label(theme::muted("unavailable")).on_hover_text(why);
                            } else if entry.installed {
                                if theme::ghost_button(ui, "Remove", true).clicked() {
                                    uninstall = Some(entry.id.clone());
                                }
                                if entry.eval_cases > 0
                                    && theme::ghost_button(ui, "Score", true).clicked()
                                {
                                    run_evals = Some(entry.id.clone());
                                }
                            } else if theme::primary_button(ui, "Install", true).clicked() {
                                install = Some(entry.path.clone());
                            }
                        });
                    });

                    ui.add_space(6.0);
                    ui.label(theme::body(entry.description.clone()));

                    if let Some(reason) = &entry.native_reason {
                        ui.add_space(6.0);
                        theme::tinted_card(ui, P.amber_soft, P.amber, |ui| {
                            ui.label(
                                egui::RichText::new("RUNS OUTSIDE THE SANDBOX")
                                    .size(9.5)
                                    .color(P.amber),
                            );
                            // Shown verbatim, as the spec requires.
                            ui.label(theme::body(reason.clone()));
                        });
                    }

                    theme::section(ui, "What it may do");
                    for line in &entry.capability_lines {
                        ui.horizontal_top(|ui| {
                            ui.label(theme::muted("·"));
                            ui.label(theme::body(line.clone()));
                        });
                    }

                    if !entry.widens.is_empty() {
                        theme::section(ui, "New since the installed version");
                        for line in &entry.widens {
                            ui.label(
                                egui::RichText::new(format!("· {line}"))
                                    .size(12.0)
                                    .color(P.amber),
                            );
                        }
                    }

                    theme::section(ui, "Fit for an agent");
                    ui.horizontal_wrapped(|ui| {
                        theme::pill(
                            ui,
                            &format!("{} tools", entry.tool_count),
                            Tone::Neutral,
                            false,
                        );
                        theme::pill(
                            ui,
                            &format!("{} front door", entry.front_door.len()),
                            Tone::Neutral,
                            false,
                        );
                        theme::pill(
                            ui,
                            if entry.has_context_provider {
                                "context provider"
                            } else {
                                "no context provider"
                            },
                            if entry.has_context_provider {
                                Tone::Good
                            } else {
                                Tone::Bad
                            },
                            false,
                        );
                        theme::pill(
                            ui,
                            match entry.doc_kind {
                                proto::DocKind::Crdt => "crdt document",
                                proto::DocKind::Blob => "blob document",
                            },
                            Tone::Neutral,
                            false,
                        );
                        theme::pill(
                            ui,
                            &if entry.eval_cases == 0 {
                                "no eval suite".to_string()
                            } else {
                                format!("{} eval cases", entry.eval_cases)
                            },
                            if entry.eval_cases == 0 {
                                Tone::Bad
                            } else {
                                Tone::Info
                            },
                            false,
                        );
                    });
                    ui.add_space(2.0);
                    ui.label(theme::tiny(
                        "A pass rate needs a loaded model and this package installed — install it, \
                         then press Score.",
                    ));
                });
            }

            if let Some(path) = install {
                app.send(proto::Request::InstallHarness { path });
                app.send(proto::Request::ListCatalog);
            }
            if let Some(harness) = uninstall {
                app.send(proto::Request::UninstallHarness { harness });
                app.send(proto::Request::ListCatalog);
            }
            if let Some(harness) = run_evals {
                app.send(proto::Request::RunEvals { harness });
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |app, ui| {
            let Some(env) = app.env.clone() else { return };
            theme::card(ui, |ui| {
                ui.label(theme::tiny("NETWORK"));
                ui.label(theme::body(format!(
                    "This environment is `{}`. The administrator's ceiling is `{}`.",
                    env.network.label(),
                    env.network_ceiling.label()
                )));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    for mode in [
                        proto::NetworkMode::Airgapped,
                        proto::NetworkMode::Ask,
                        proto::NetworkMode::Online,
                    ] {
                        let allowed = mode.rank() <= env.network_ceiling.rank();
                        let current = env.network == mode;
                        if ui
                            .add_enabled(
                                allowed && !current,
                                egui::Button::new(theme::body(mode.label()))
                                    .fill(if current { P.accent_soft } else { P.surface })
                                    .stroke(Stroke::new(
                                        1.0,
                                        if current { P.accent } else { P.border },
                                    )),
                            )
                            .clicked()
                        {
                            app.send(proto::Request::SetNetworkMode { mode });
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(theme::tiny(
                    "Under `airgapped` the web tools are absent from the model's tool set entirely, \
                     so it never proposes a search it cannot run.",
                ));
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(theme::tiny("MACHINE"));
                ui.label(theme::body(env.machine.clone()));
                ui.label(theme::muted(env.profile.clone()));
            });
        });
    }

    fn help_page(&mut self, ui: &mut egui::Ui) {
        self.page(ui, |_app, ui| {
            theme::card(ui, |ui| {
                ui.label(theme::title("What this is"));
                ui.label(theme::body(
                    "A workspace where every capability comes from an installed harness. A harness \
                     contributes a surface for you, tools for the agent, and a context provider that \
                     tells the model what state it is in.",
                ));
            });
            ui.add_space(10.0);
            theme::card(ui, |ui| {
                ui.label(theme::title("Command line"));
                ui.add_space(4.0);
                for (cmd, what) in [
                    ("localspace", "open this window"),
                    ("localspace doctor", "what this machine can run"),
                    ("localspace bench", "the efficiency budgets for this machine"),
                    ("localspace evals <id>", "a harness's agent-compatibility score"),
                    ("localspace-serve --bind …", "the same Core, for a browser Client"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(cmd).monospace().size(11.5).color(P.text));
                        ui.label(theme::muted(what));
                    });
                }
            });
        });
    }

    // -----------------------------------------------------------------------
    // Approvals
    // -----------------------------------------------------------------------

    /// Notices float above the status bar on every page.
    ///
    /// They used to render only in the conversation, which meant an install
    /// failure on the Marketplace was silent — the worst kind of error.
    pub(crate) fn notices_strip(&mut self, ctx: &egui::Context) {
        if self.notices.is_empty() {
            return;
        }
        let screen = ctx.viewport_rect();
        let width = 420.0_f32.min(screen.width() - theme::RAIL_WIDTH - 40.0);
        let mut dismissed: Option<usize> = None;

        egui::Area::new(egui::Id::new("notices"))
            .fixed_pos(egui::pos2(
                screen.right() - width - 20.0,
                screen.bottom() - theme::STATUS_BAR_HEIGHT - 20.0 - 56.0,
            ))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(width);
                for (i, (level, text)) in self.notices.iter().enumerate().rev().take(3) {
                    let (fill, border, colour, label) = match level {
                        proto::NoticeLevel::Info => (P.surface, P.border, P.muted, "note"),
                        proto::NoticeLevel::Warn => (P.amber_soft, P.amber, P.amber, "warning"),
                        proto::NoticeLevel::Error => (P.danger_soft, P.danger, P.danger, "failed"),
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(Stroke::new(1.0, border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(12, 9))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 4],
                            blur: 14,
                            spread: 0,
                            color: Color32::from_black_alpha(14),
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(label).size(9.5).color(colour));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(theme::muted("×"))
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE),
                                            )
                                            .clicked()
                                        {
                                            dismissed = Some(i);
                                        }
                                    },
                                );
                            });
                            ui.label(theme::body(text.clone()));
                        });
                    ui.add_space(6.0);
                }
            });

        if let Some(i) = dismissed {
            self.notices.remove(i);
        }
    }

    pub(crate) fn approvals_window(&mut self, ctx: &egui::Context) {
        if self.approvals.is_empty() {
            return;
        }
        let mut answered: Option<(String, bool)> = None;
        egui::Area::new(egui::Id::new("approvals"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, theme::TOP_BAR_HEIGHT + 16.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                egui::Frame::new()
                    .fill(P.surface)
                    .stroke(Stroke::new(1.0, P.amber))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(28),
                    })
                    .show(ui, |ui| {
                        for (id, kind, prompt) in self.approvals.clone() {
                            ui.label(
                                egui::RichText::new(match kind {
                                    proto::ApprovalKind::ToolConfirm => "TOOL NEEDS YOUR APPROVAL",
                                    proto::ApprovalKind::Egress => "NETWORK ACCESS",
                                    proto::ApprovalKind::Capability => "CAPABILITY",
                                    proto::ApprovalKind::NativeTier => "NATIVE HARNESS",
                                })
                                .size(9.5)
                                .color(P.amber),
                            );
                            ui.add_space(4.0);
                            ui.label(theme::body(prompt));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if theme::primary_button(ui, "Allow", true).clicked() {
                                    answered = Some((id.clone(), true));
                                }
                                if theme::ghost_button(ui, "Decline", true).clicked() {
                                    answered = Some((id.clone(), false));
                                }
                            });
                        }
                    });
            });
        if let Some((id, granted)) = answered {
            self.approvals.retain(|(a, _, _)| *a != id);
            // A capability-diff approval resumes an install; everything else is a
            // tool or egress gate held in Core's pending map.
            match id.strip_prefix("install:") {
                Some(rest) if granted => {
                    let (harness, token) = rest.split_once(':').unwrap_or((rest, ""));
                    self.send(proto::Request::ApproveInstall {
                        harness: harness.to_string(),
                        token: token.to_string(),
                    });
                }
                Some(_) => {}
                None => self.send(proto::Request::Approve { id, granted }),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Surfaces
    // -----------------------------------------------------------------------

    fn show_widget_view(&mut self, ui: &mut egui::Ui, harness: &str, view: &str) {
        let key = (harness.to_string(), view.to_string());
        if !self.widget_views.contains_key(&key) {
            self.request_widget_view(harness, view);
            ui.label(theme::muted("loading…"));
            return;
        }
        let tree = self.widget_views.get(&key).cloned().unwrap();
        let mut events = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                crate::widgets::show(ui, &tree, &mut events);
            });
        for event in events {
            self.send(proto::Request::WidgetEvent {
                harness: harness.to_string(),
                view: view.to_string(),
                event,
            });
            self.request_widget_view(harness, view);
        }
    }

    fn show_egui_surface(
        &mut self,
        ui: &mut egui::Ui,
        harness: &str,
        view: &str,
        rect: egui::Rect,
    ) {
        let key = (harness.to_string(), view.to_string());
        if !self.surfaces.contains_key(&key) {
            self.surfaces.insert(key.clone(), OpenSurface::default());
            self.request_surface(harness, view);
        }

        let (error, has_runner, ever_ran) = self
            .surfaces
            .get(&key)
            .map(|s| (s.error.clone(), s.runner.is_some(), s.ever_ran))
            .unwrap_or((None, false, false));
        if let Some(err) = error {
            let title = if ever_ran {
                "This surface stopped"
            } else {
                "This surface could not start"
            };
            self.empty_state(ui, rect, title, &err);
            // Once the automatic restart is spent, the next one is the user's
            // call. The document is in Core; only the viewport is lost.
            let button = egui::Rect::from_center_size(
                rect.center() + Vec2::new(0.0, 46.0),
                Vec2::new(150.0, 26.0),
            );
            let clicked = ui
                .scope_builder(egui::UiBuilder::new().max_rect(button), |ui| {
                    theme::ghost_button(ui, "Restart surface", true).clicked()
                })
                .inner;
            if clicked {
                let now = ui.input(|i| i.time);
                if let Some(entry) = self.surfaces.get_mut(&key) {
                    entry.error = None;
                    entry.runner = None;
                    entry.restarts.manual(now);
                }
                self.request_surface(harness, view);
                self.set_zoom(self.zoom);
            }
            return;
        }
        if !has_runner {
            self.empty_state(ui, rect, "Loading surface…", "");
            return;
        }

        let entry = self.surfaces.get_mut(&key).expect("checked above");
        entry.ever_ran = true;
        let inbox = std::mem::take(&mut entry.inbox);
        let runner = entry.runner.as_mut().expect("checked above");
        let (throttled, last_ms) = (runner.stats.throttled, runner.stats.last_ms);
        let result = runner.show(ui, rect.shrink(1.0), inbox);

        match result {
            Ok(paint) => {
                for message in paint.messages {
                    self.send(proto::Request::HarnessEvent {
                        harness: harness.to_string(),
                        view: view.to_string(),
                        payload: message,
                    });
                }
                if let Some(doc) = paint.doc {
                    self.dirty_since_commit = true;
                    self.pending_doc_write = Some(self.backend.request(
                        proto::Request::HarnessEvent {
                            harness: harness.to_string(),
                            view: view.to_string(),
                            payload: format!("{{\"doc\":{doc}}}").into_bytes(),
                        },
                    ));
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                let now = ui.input(|i| i.time);
                // A surface that broke its memory budget is restarted once on
                // its own account; its state is in the document. A second break
                // within the cooldown stays down, reason on screen, until the
                // user asks.
                let restart = match self.surfaces.get_mut(&key) {
                    Some(entry) => {
                        let broke_budget = entry
                            .runner
                            .as_ref()
                            .and_then(|r| r.over_budget())
                            .is_some();
                        entry.runner = None;
                        if broke_budget && entry.restarts.automatic(now) {
                            entry.error = None;
                            true
                        } else {
                            entry.error = Some(msg.clone());
                            false
                        }
                    }
                    None => false,
                };
                if restart {
                    self.notices.push((
                        proto::NoticeLevel::Warn,
                        format!("{harness}/{view}: {msg}. Restarted with a fresh instance."),
                    ));
                    crate::perf::log(format!("surface {harness}/{view} restarted: {msg}"));
                    self.request_surface(harness, view);
                    self.set_zoom(self.zoom);
                }
            }
        }

        if throttled {
            let badge =
                egui::Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::new(104.0, 20.0));
            ui.painter()
                .rect_filled(badge, CornerRadius::same(10), P.danger_soft);
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                format!("slow · {last_ms:.0} ms"),
                egui::FontId::proportional(10.5),
                P.danger,
            );
        }
    }
}
