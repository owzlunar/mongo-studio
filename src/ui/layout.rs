use egui::{Context, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit, Window};
use egui_dock::{DockArea, Style};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::models::CollectionInfo;
use crate::theme::{ThemeMode, ThemePalette};
use crate::ui::activity_bar::render_activity_bar;
use crate::ui::command_palette::render_command_palette;
use crate::ui::components::buttons::{primary_button, secondary_button};
use crate::ui::sidebar::render_sidebar;
use crate::ui::status_bar::render_status_bar;
use crate::ui::tab_bar::CollectionTabViewer;
use crate::workspace::{BottomPanelTab, LogLevel};

pub fn render_main_layout(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    render_top_menu_bar(ctx, state, palette, events);
    render_status_bar(ctx, state, palette, events);
    render_activity_bar(ctx, state, palette, events);
    render_sidebar(ctx, state, palette, events);
    render_bottom_panel(ctx, state, palette, events);
    render_central_workspace(ctx, state, palette, events);
    render_command_palette(ctx, state, palette, events);
    render_modals(ctx, state, palette, events);
}

fn render_top_menu_bar(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let frame = Frame::none()
        .fill(palette.sidebar_header_bg)
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .stroke(Stroke::new(1.0, palette.border_color));

    egui::TopBottomPanel::top("top_menu_bar")
        .resizable(false)
        .exact_height(28.0)
        .frame(frame)
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(
                    RichText::new("🍃 Mongo Studio")
                        .strong()
                        .color(palette.success_color),
                );
                ui.add_space(12.0);

                ui.menu_button("File", |ui| {
                    if ui.button("New Connection...").clicked() {
                        events.push(AppEvent::OpenNewConnectionModal);
                        ui.close_menu();
                    }
                    if ui.button("Refresh Connections").clicked() {
                        events.push(AppEvent::RefreshConnections);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Reset Workspace Layout").clicked() {
                        events.push(AppEvent::ResetWorkspaceLayout);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Command Palette... (Ctrl+Shift+P)").clicked() {
                        events.push(AppEvent::ToggleCommandPalette);
                        ui.close_menu();
                    }
                    if ui.button("Clear Output Logs").clicked() {
                        events.push(AppEvent::ClearOutputLogs);
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Toggle Sidebar (Ctrl+B)").clicked() {
                        events.push(AppEvent::ToggleSidebar);
                        ui.close_menu();
                    }
                    if ui.button("Toggle Bottom Panel (Ctrl+J)").clicked() {
                        events.push(AppEvent::ToggleBottomPanel);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Dark Theme (VS Code Dark+)").clicked() {
                        events.push(AppEvent::SwitchTheme(ThemeMode::Dark));
                        ui.close_menu();
                    }
                    if ui.button("Light Theme").clicked() {
                        events.push(AppEvent::SwitchTheme(ThemeMode::Light));
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About Mongo Studio").clicked() {
                        state.show_about_modal = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(
                            RichText::new("⌘ Command Palette (Ctrl+P)")
                                .size(11.5)
                                .color(palette.muted_text),
                        )
                        .clicked()
                    {
                        events.push(AppEvent::ToggleCommandPalette);
                    }
                });
            });
        });
}

fn render_bottom_panel(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    if !state.bottom_panel.visible {
        return;
    }

    let frame = Frame::none()
        .fill(palette.panel_bg)
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .stroke(Stroke::new(1.0, palette.border_color));

    egui::TopBottomPanel::bottom("bottom_output_panel")
        .resizable(true)
        .default_height(165.0)
        .height_range(100.0..=360.0)
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in [
                    BottomPanelTab::Terminal,
                    BottomPanelTab::Output,
                    BottomPanelTab::QueryHistory,
                    BottomPanelTab::Problems,
                ] {
                    let is_active = state.bottom_panel.active_tab == tab;
                    if ui
                        .selectable_label(is_active, RichText::new(tab.label()).size(11.5).strong())
                        .clicked()
                    {
                        state.bottom_panel.active_tab = tab;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text("Close Panel").clicked() {
                        events.push(AppEvent::ToggleBottomPanel);
                    }
                    if ui.small_button("🗑").on_hover_text("Clear Logs").clicked() {
                        events.push(AppEvent::ClearOutputLogs);
                    }
                });
            });

            ui.separator();

            match state.bottom_panel.active_tab {
                BottomPanelTab::Output => {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for entry in &state.bottom_panel.logs {
                                let lvl_color = match entry.level {
                                    LogLevel::Info => palette.accent_color,
                                    LogLevel::Success => palette.success_color,
                                    LogLevel::Warning => palette.warning_color,
                                    LogLevel::Error => palette.error_color,
                                };
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("[{}]", entry.timestamp))
                                            .monospace()
                                            .size(11.5)
                                            .color(palette.muted_text),
                                    );
                                    ui.label(
                                        RichText::new(format!("[{}]", entry.source))
                                            .monospace()
                                            .size(11.5)
                                            .color(lvl_color),
                                    );
                                    ui.label(RichText::new(&entry.message).monospace().size(12.0));
                                });
                            }
                        });
                }
                BottomPanelTab::Terminal => {
                    ScrollArea::vertical()
                        .max_height(ui.available_height() - 28.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &state.bottom_panel.terminal_history {
                                ui.label(RichText::new(line).monospace().size(12.0));
                            }
                        });

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("mongosh>")
                                .monospace()
                                .strong()
                                .color(palette.success_color),
                        );
                        let resp = ui.add(
                            TextEdit::singleline(&mut state.bottom_panel.terminal_input)
                                .font(egui::FontId::monospace(12.0))
                                .hint_text("Enter mock shell command...")
                                .desired_width(f32::INFINITY),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let cmd = state.bottom_panel.terminal_input.trim().to_string();
                            if !cmd.is_empty() {
                                state
                                    .bottom_panel
                                    .terminal_history
                                    .push(format!("mongosh> {}", cmd));
                                state.bottom_panel.terminal_history.push(
                                    "{ \"ok\": 1, \"note\": \"Executed in Phase 1 mock shell\" }"
                                        .to_string(),
                                );
                                state.bottom_panel.terminal_input.clear();
                            }
                            resp.request_focus();
                        }
                    });
                }
                BottomPanelTab::QueryHistory => {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (i, q) in state.bottom_panel.query_history.iter().enumerate() {
                                ui.label(
                                    RichText::new(format!("{}. {}", i + 1, q))
                                        .monospace()
                                        .size(12.0),
                                );
                            }
                        });
                }
                BottomPanelTab::Problems => {
                    ui.label(
                        RichText::new("✔ No syntax or connection problems detected in workspace.")
                            .color(palette.success_color),
                    );
                }
            }
        });
}

fn render_central_workspace(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let all_collections: Vec<CollectionInfo> = state
        .connections
        .iter()
        .flat_map(|conn| conn.databases.iter())
        .flat_map(|db| db.collections.iter().cloned())
        .collect();

    let frame = Frame::none().fill(palette.editor_bg);

    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.tab_bar.bg_fill = palette.tab_inactive_bg;
        style.tab.active.bg_fill = palette.tab_active_bg;
        style.tab.inactive.bg_fill = palette.tab_inactive_bg;

        let mut viewer = CollectionTabViewer {
            collections: &all_collections,
            palette,
            events,
        };

        DockArea::new(&mut state.dock_state)
            .style(style)
            .show_inside(ui, &mut viewer);
    });
}

fn render_modals(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    if state.show_new_connection_modal {
        Window::new("Add MongoDB Connection")
            .collapsible(false)
            .resizable(false)
            .fixed_size([420.0, 190.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("Connection Profile Name").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_conn_name).desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("MongoDB Connection URI").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_conn_uri)
                        .font(egui::FontId::monospace(12.5))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if primary_button(ui, "Connect & Save", palette.accent_color).clicked() {
                        events.push(AppEvent::AddConnection {
                            name: state.new_conn_name.clone(),
                            uri: state.new_conn_uri.clone(),
                        });
                        state.show_new_connection_modal = false;
                    }
                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.show_new_connection_modal = false;
                    }
                });
            });
    }

    if state.show_about_modal {
        Window::new("About Mongo Studio")
            .collapsible(false)
            .resizable(false)
            .fixed_size([380.0, 160.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.heading("🍃 Mongo Studio v0.1.0");
                ui.add_space(4.0);
                ui.label("VS Code-inspired MongoDB Desktop Admin Tool built in 100% Rust.");
                ui.label("Stack: Rust + egui + eframe + egui_dock");
                ui.add_space(10.0);
                if primary_button(ui, "Close", palette.accent_color).clicked() {
                    state.show_about_modal = false;
                }
            });
    }
}
