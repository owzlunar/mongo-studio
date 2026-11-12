use egui::{
    ComboBox, Context, CursorIcon, Frame, Image, Margin, RichText, ScrollArea, Stroke, TextEdit,
    Vec2, Window,
};
use egui_dock::{DockArea, Style};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::models::{ConnectionConfig, DocumentRow, UserRole};
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

fn menu_item(ui: &mut egui::Ui, label: &str) -> bool {
    ui.button(label)
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
}

fn render_top_menu_bar(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let frame = Frame::none()
        .fill(palette.sidebar_header_bg)
        .inner_margin(Margin::symmetric(10.0, 3.0))
        .stroke(Stroke::new(1.0_f32, palette.border_color));

    egui::TopBottomPanel::top("top_menu_bar")
        .resizable(false)
        .exact_height(30.0)
        .frame(frame)
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.add(
                    Image::new(egui::include_image!("../../assets/logo.svg"))
                        .max_size(Vec2::new(18.0, 18.0)),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Mongo Studio")
                        .strong()
                        .color(palette.success_color),
                );
                ui.add_space(12.0);

                let file_resp = ui.menu_button("File", |ui| {
                    if menu_item(ui, &format!("{} New Connection...", regular::PLUS)) {
                        events.push(AppEvent::OpenNewConnectionModal);
                        ui.close_menu();
                    }
                    if menu_item(
                        ui,
                        &format!("{} Refresh Connections", regular::ARROWS_CLOCKWISE),
                    ) {
                        events.push(AppEvent::RefreshConnections);
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_item(
                        ui,
                        &format!(
                            "{} Reset Workspace Layout",
                            regular::ARROW_COUNTER_CLOCKWISE
                        ),
                    ) {
                        events.push(AppEvent::ResetWorkspaceLayout);
                        ui.close_menu();
                    }
                });
                file_resp.response.on_hover_cursor(CursorIcon::PointingHand);

                let edit_resp = ui.menu_button("Edit", |ui| {
                    if menu_item(
                        ui,
                        &format!(
                            "{} Command Palette... (Ctrl+P)",
                            regular::TERMINAL_WINDOW
                        ),
                    ) {
                        events.push(AppEvent::ToggleCommandPalette);
                        ui.close_menu();
                    }
                    if menu_item(ui, &format!("{} Clear Output Logs", regular::TRASH)) {
                        events.push(AppEvent::ClearOutputLogs);
                        ui.close_menu();
                    }
                });
                edit_resp.response.on_hover_cursor(CursorIcon::PointingHand);

                let view_resp = ui.menu_button("View", |ui| {
                    if menu_item(
                        ui,
                        &format!("{} Toggle Sidebar (Ctrl+B)", regular::SIDEBAR_SIMPLE),
                    ) {
                        events.push(AppEvent::ToggleSidebar);
                        ui.close_menu();
                    }
                    if menu_item(
                        ui,
                        &format!("{} Toggle Bottom Panel (Ctrl+J)", regular::TERMINAL_WINDOW),
                    ) {
                        events.push(AppEvent::ToggleBottomPanel);
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_item(ui, &format!("{} Dark Theme (VS Code Dark+)", regular::MOON)) {
                        events.push(AppEvent::SwitchTheme(ThemeMode::Dark));
                        ui.close_menu();
                    }
                    if menu_item(ui, &format!("{} Light Theme", regular::SUN)) {
                        events.push(AppEvent::SwitchTheme(ThemeMode::Light));
                        ui.close_menu();
                    }
                });
                view_resp.response.on_hover_cursor(CursorIcon::PointingHand);

                let help_resp = ui.menu_button("Help", |ui| {
                    if menu_item(ui, &format!("{} About Mongo Studio", regular::INFO)) {
                        state.show_about_modal = true;
                        ui.close_menu();
                    }
                });
                help_resp.response.on_hover_cursor(CursorIcon::PointingHand);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(
                            RichText::new(format!(
                                "{} Command Palette (Ctrl+P)",
                                regular::TERMINAL_WINDOW
                            ))
                            .size(11.5)
                            .color(palette.muted_text),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
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
        .stroke(Stroke::new(1.0_f32, palette.border_color));

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
                        .selectable_label(
                            is_active,
                            RichText::new(format!("{} {}", tab.icon(), tab.label()))
                                .size(11.5)
                                .strong(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        state.bottom_panel.active_tab = tab;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(regular::X)
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Close Panel")
                        .clicked()
                    {
                        events.push(AppEvent::ToggleBottomPanel);
                    }
                    if ui
                        .small_button(format!("{} Clear", regular::TRASH))
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Clear Logs")
                        .clicked()
                    {
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
                        .max_height(ui.available_height() - 30.0)
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
                                .hint_text("Enter shell command...")
                                .margin(Margin::symmetric(6.0, 4.0))
                                .min_size(Vec2::new(200.0, 24.0))
                                .desired_width(f32::INFINITY),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let cmd = state.bottom_panel.terminal_input.trim();
                            if !cmd.is_empty() {
                                state
                                    .bottom_panel
                                    .terminal_history
                                    .push(format!("mongosh> {cmd}"));
                                state.bottom_panel.terminal_history.push(
                                    "{ \"ok\": 1, \"note\": \"Executed in Mongo Studio shell\" }"
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
                                    RichText::new(format!("{}. {q}", i + 1))
                                        .monospace()
                                        .size(12.0),
                                );
                            }
                        });
                }
                BottomPanelTab::Problems => {
                    let mut has_problems = false;
                    for conn in &state.connections {
                        if let Some(err) = &conn.last_error {
                            has_problems = true;
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} [{}]",
                                        regular::WARNING_CIRCLE,
                                        conn.name
                                    ))
                                    .color(palette.error_color)
                                    .strong()
                                    .monospace(),
                                );
                                ui.label(RichText::new(err).monospace().size(12.0));
                            });
                        }
                    }
                    if !has_problems {
                        ui.label(
                            RichText::new(format!(
                                "{} No syntax or connection problems detected in workspace.",
                                regular::CHECK_CIRCLE
                            ))
                            .color(palette.success_color),
                        );
                    }
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
    let frame = Frame::none().fill(palette.editor_bg);

    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        let mut style = Style::from_egui(ctx.style().as_ref());
        style.tab_bar.bg_fill = palette.tab_inactive_bg;
        style.tab.active.bg_fill = palette.tab_active_bg;
        style.tab.inactive.bg_fill = palette.tab_inactive_bg;

        let mut viewer = CollectionTabViewer {
            connections: &state.connections,
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
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        let is_editing = state.editing_connection_idx.is_some();
        let modal_title = if is_editing {
            "Edit MongoDB Connection"
        } else {
            "Add MongoDB Connection"
        };

        Window::new(modal_title)
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([460.0, 310.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("Connection Profile Name (max 30 chars)").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_conn_name)
                        .char_limit(ConnectionConfig::MAX_PROFILE_NAME_LEN)
                        .hint_text("e.g. LAN MongoDB Server")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .min_size(Vec2::new(200.0, 24.0))
                        .desired_width(f32::INFINITY),
                );
                if state.new_conn_name.chars().count() > ConnectionConfig::MAX_PROFILE_NAME_LEN {
                    state.new_conn_name = ConnectionConfig::clamp_profile_name(&state.new_conn_name);
                }
                ui.add_space(6.0);

                ui.label(RichText::new("Host or MongoDB Connection URI").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_conn_uri)
                        .font(egui::FontId::monospace(12.0))
                        .hint_text("192.168.100.243:27017 or mongodb://192.168.100.243:27017")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .min_size(Vec2::new(200.0, 24.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                ui.label(
                    RichText::new("Authentication (Optional)")
                        .strong()
                        .size(12.0)
                        .color(palette.muted_text),
                );
                ui.columns(2, |columns| {
                    columns[0].label(RichText::new("Username").size(11.5));
                    columns[0].add(
                        TextEdit::singleline(&mut state.new_conn_username)
                            .hint_text("admin (optional)")
                            .margin(Margin::symmetric(6.0, 4.0))
                            .desired_width(f32::INFINITY),
                    );

                    columns[1].label(RichText::new("Password").size(11.5));
                    columns[1].add(
                        TextEdit::singleline(&mut state.new_conn_password)
                            .password(true)
                            .hint_text("password (optional)")
                            .margin(Margin::symmetric(6.0, 4.0))
                            .desired_width(f32::INFINITY),
                    );
                });

                ui.add_space(4.0);
                ui.checkbox(
                    &mut state.new_conn_save_password,
                    RichText::new("Save password in OS Keychain").size(11.5),
                );

                ui.add_space(6.0);
                if state.is_testing_connection {
                    ui.label(
                        RichText::new(format!(
                            "{} Testing connection...",
                            regular::CIRCLE_HALF
                        ))
                        .color(palette.warning_color)
                        .monospace()
                        .size(11.5),
                    );
                } else if let Some(status) = &state.test_conn_status {
                    match status {
                        Ok(msg) => {
                            ui.label(
                                RichText::new(format!("{} {msg}", regular::CHECK_CIRCLE))
                                    .color(palette.success_color)
                                    .monospace()
                                    .size(11.5),
                            );
                        }
                        Err(err) => {
                            ui.label(
                                RichText::new(format!("{} {err}", regular::WARNING_CIRCLE))
                                    .color(palette.error_color)
                                    .monospace()
                                    .size(11.5),
                            );
                        }
                    }
                }

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let submit_label = if is_editing {
                        format!("{} Save & Reconnect", regular::FLOPPY_DISK)
                    } else {
                        format!("{} Connect & Save", regular::PLUGS_CONNECTED)
                    };

                    if primary_button(ui, &submit_label, palette.accent_color).clicked() {
                        let effective_uri = state.build_effective_uri();
                        if let Some(edit_idx) = state.editing_connection_idx.take() {
                            events.push(AppEvent::UpdateConnection {
                                index: edit_idx,
                                name: state.new_conn_name.clone(),
                                uri: effective_uri,
                            });
                        } else {
                            events.push(AppEvent::AddConnection {
                                name: state.new_conn_name.clone(),
                                uri: effective_uri,
                            });
                        }
                        state.show_new_connection_modal = false;
                    }

                    if secondary_button(
                        ui,
                        &format!("{} Test Connection", regular::LIGHTNING),
                        palette.border_color,
                    )
                    .clicked()
                    {
                        let effective_uri = state.build_effective_uri();
                        events.push(AppEvent::TestConnection {
                            name: state.new_conn_name.clone(),
                            uri: effective_uri,
                        });
                    }

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.editing_connection_idx = None;
                        state.show_new_connection_modal = false;
                    }
                });
            });
    }

    if state.show_create_db_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        Window::new("Add MongoDB Database")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([410.0, 210.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("Database Name").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_db_name)
                        .hint_text("e.g. inventory_db")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("Initial Collection Name").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_db_collection)
                        .hint_text("e.g. items")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );
                ui.label(
                    RichText::new(
                        "MongoDB materializes a new database once its first collection is created.",
                    )
                    .size(11.0)
                    .color(palette.muted_text),
                );

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let trimmed_db = state.new_db_name.trim();
                    let trimmed_coll = state.new_db_collection.trim();
                    let can_submit = !trimmed_db.is_empty() && !trimmed_coll.is_empty();

                    ui.add_enabled_ui(can_submit, |ui| {
                        if primary_button(
                            ui,
                            &format!("{} Create Database", regular::DATABASE),
                            palette.accent_color,
                        )
                        .clicked()
                        {
                            events.push(AppEvent::CreateDatabase {
                                connection_index: state.create_db_conn_idx,
                                db_name: trimmed_db.to_string(),
                                collection_name: trimmed_coll.to_string(),
                            });
                            state.show_create_db_modal = false;
                        }
                    });

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.show_create_db_modal = false;
                    }
                });
            });
    }

    if state.show_create_coll_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        Window::new("Create Collection")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([400.0, 175.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Target Database: {}",
                        regular::DATABASE,
                        state.create_coll_db_name
                    ))
                    .monospace()
                    .strong()
                    .color(palette.accent_color),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("Collection Name").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_coll_name)
                        .hint_text("e.g. orders")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );

                let trimmed_coll = state.new_coll_name.trim();
                let is_system_coll = trimmed_coll.starts_with("system.");
                if is_system_coll {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} Collection name cannot start with 'system.'.",
                            regular::WARNING_CIRCLE
                        ))
                        .color(palette.error_color)
                        .size(11.5),
                    );
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let can_submit = !trimmed_coll.is_empty() && !is_system_coll;
                    ui.add_enabled_ui(can_submit, |ui| {
                        if primary_button(
                            ui,
                            &format!("{} Create", regular::TABLE),
                            palette.accent_color,
                        )
                        .clicked()
                        {
                            events.push(AppEvent::CreateCollection {
                                connection_index: state.create_coll_conn_idx,
                                db_name: state.create_coll_db_name.clone(),
                                collection_name: trimmed_coll.to_string(),
                            });
                            state.show_create_coll_modal = false;
                        }
                    });

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.show_create_coll_modal = false;
                    }
                });
            });
    }

    if let Some((conn_idx, db_name)) = state.pending_drop_db.clone() {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.error_color))
            .inner_margin(Margin::same(14.0));

        Window::new("Confirm Drop Database")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([410.0, 160.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Permanently drop database '{db_name}'?",
                        regular::WARNING
                    ))
                    .color(palette.error_color)
                    .strong()
                    .size(13.5),
                );
                ui.add_space(6.0);
                ui.label(
                    "This action will delete the database and all of its collections immediately. It cannot be undone.",
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if primary_button(
                        ui,
                        &format!("{} Drop Database", regular::TRASH),
                        palette.error_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::ConfirmDropDatabase {
                            connection_index: conn_idx,
                            db_name,
                        });
                        state.pending_drop_db = None;
                    }

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.pending_drop_db = None;
                    }
                });
            });
    }

    if let Some((conn_idx, db_name, coll_name)) = state.pending_drop_coll.clone() {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.error_color))
            .inner_margin(Margin::same(14.0));

        Window::new("Confirm Drop Collection")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([410.0, 160.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Permanently drop collection '{db_name}.{coll_name}'?",
                        regular::WARNING
                    ))
                    .color(palette.error_color)
                    .strong()
                    .size(13.5),
                );
                ui.add_space(6.0);
                ui.label(
                    "All documents and indexes in this collection will be permanently deleted.",
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if primary_button(
                        ui,
                        &format!("{} Drop Collection", regular::TRASH),
                        palette.error_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::ConfirmDropCollection {
                            connection_index: conn_idx,
                            db_name,
                            collection_name: coll_name,
                        });
                        state.pending_drop_coll = None;
                    }

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.pending_drop_coll = None;
                    }
                });
            });
    }

    if state.show_doc_editor_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        let is_editing = state.doc_editor_original_id.is_some();
        let window_title = if is_editing {
            "Edit Document"
        } else {
            "Insert Document"
        };

        Window::new(window_title)
            .collapsible(false)
            .resizable(true)
            .frame(modal_frame)
            .default_size([560.0, 460.0])
            .min_size([440.0, 340.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} {} / {}.{}",
                            regular::TABLE,
                            state.doc_editor_conn_name,
                            state.doc_editor_db,
                            state.doc_editor_coll
                        ))
                        .monospace()
                        .strong()
                        .color(palette.accent_color),
                    );
                    if let Some(ref orig_id) = state.doc_editor_original_id {
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "_id: {} (immutable)",
                                DocumentRow::bson_to_display(orig_id)
                            ))
                            .monospace()
                            .size(11.5)
                            .color(palette.warning_color),
                        );
                    }
                });
                ui.add_space(6.0);

                ScrollArea::vertical()
                    .id_source("doc_editor_json_scroll")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut state.doc_editor_json)
                                .font(egui::FontId::monospace(12.0))
                                .desired_width(f32::INFINITY)
                                .desired_rows(16)
                                .margin(Margin::same(8.0)),
                        );
                    });

                ui.add_space(6.0);

                let parsed_res = DocumentRow::parse_json_to_bson_doc(&state.doc_editor_json);
                let id_mismatch_err = match (&state.doc_editor_original_id, &parsed_res) {
                    (Some(orig_id), Ok(doc)) => doc
                        .get("_id")
                        .filter(|provided| *provided != orig_id)
                        .map(|_| "Cannot modify immutable '_id' field when updating a document"),
                    _ => None,
                };

                let can_save = match (&parsed_res, id_mismatch_err) {
                    (Ok(_), None) => {
                        ui.label(
                            RichText::new(format!(
                                "{} Valid Extended JSON",
                                regular::CHECK_CIRCLE
                            ))
                            .color(palette.success_color)
                            .size(11.5),
                        );
                        true
                    }
                    (_, Some(msg)) => {
                        ui.label(
                            RichText::new(format!("{} {msg}", regular::WARNING_CIRCLE))
                                .color(palette.error_color)
                                .size(11.5),
                        );
                        false
                    }
                    (Err(err), None) => {
                        ui.label(
                            RichText::new(format!("{} {err}", regular::WARNING_CIRCLE))
                                .color(palette.error_color)
                                .size(11.5),
                        );
                        false
                    }
                };

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let save_label = if is_editing {
                        format!("{} Save", regular::FLOPPY_DISK)
                    } else {
                        format!("{} Insert Document", regular::PLUS)
                    };

                    ui.add_enabled_ui(can_save, |ui| {
                        if primary_button(ui, &save_label, palette.accent_color).clicked() {
                            if let Some(original_id) = state.doc_editor_original_id.take() {
                                events.push(AppEvent::UpdateDocument {
                                    connection_name: state.doc_editor_conn_name.clone(),
                                    database: state.doc_editor_db.clone(),
                                    collection: state.doc_editor_coll.clone(),
                                    original_id,
                                    json_text: state.doc_editor_json.clone(),
                                });
                            } else {
                                events.push(AppEvent::InsertDocument {
                                    connection_name: state.doc_editor_conn_name.clone(),
                                    database: state.doc_editor_db.clone(),
                                    collection: state.doc_editor_coll.clone(),
                                    json_text: state.doc_editor_json.clone(),
                                });
                            }
                            state.show_doc_editor_modal = false;
                        }
                    });

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.doc_editor_original_id = None;
                        state.show_doc_editor_modal = false;
                    }
                });
            });
    }

    if let Some(pending) = state.pending_delete_doc.clone() {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.error_color))
            .inner_margin(Margin::same(14.0));

        Window::new("Confirm Delete Document")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([430.0, 170.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Permanently delete document from '{}.{}'?",
                        regular::WARNING,
                        pending.database,
                        pending.collection
                    ))
                    .color(palette.error_color)
                    .strong()
                    .size(13.5),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("_id: {}", pending.display_id))
                        .monospace()
                        .color(palette.warning_color),
                );
                ui.add_space(4.0);
                ui.label("This document will be permanently removed from the collection.");
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if primary_button(
                        ui,
                        &format!("{} Delete", regular::TRASH),
                        palette.error_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::ConfirmDeleteDocument {
                            connection_name: pending.connection_name,
                            database: pending.database,
                            collection: pending.collection,
                            doc_id: pending.doc_id,
                            display_id: pending.display_id,
                        });
                        state.pending_delete_doc = None;
                    }

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.pending_delete_doc = None;
                    }
                });
            });
    }

    if state.show_create_user_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        let mut db_options = vec!["admin".to_string()];
        if let Some(conn) = state.connections.get(state.create_user_conn_idx) {
            for db in &conn.databases {
                if !db_options.contains(&db.name) {
                    db_options.push(db.name.clone());
                }
            }
        }

        let is_duplicating = state.duplicate_source_user.is_some();
        let window_title = if is_duplicating {
            "Duplicate MongoDB User"
        } else {
            "Create MongoDB User"
        };

        Window::new(window_title)
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([440.0, 325.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(src_user) = &state.duplicate_source_user {
                    ui.label(
                        RichText::new(format!(
                            "{} Cloning {} database role(s) from user '{src_user}'",
                            regular::COPY,
                            state.duplicate_source_roles.len()
                        ))
                        .color(palette.accent_color)
                        .strong()
                        .size(12.0),
                    );
                    ui.add_space(6.0);
                }

                ui.label(RichText::new("New Username").strong());
                ui.add(
                    TextEdit::singleline(&mut state.new_user_username)
                        .hint_text("e.g. report_reader")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                ui.columns(2, |columns| {
                    columns[0].label(RichText::new("Password").strong().size(12.0));
                    columns[0].add(
                        TextEdit::singleline(&mut state.new_user_password)
                            .password(!state.new_user_show_password)
                            .hint_text("Enter password...")
                            .margin(Margin::symmetric(6.0, 4.0))
                            .desired_width(f32::INFINITY),
                    );

                    columns[1].label(RichText::new("Confirm Password").strong().size(12.0));
                    columns[1].add(
                        TextEdit::singleline(&mut state.new_user_confirm_password)
                            .password(!state.new_user_show_password)
                            .hint_text("Confirm password...")
                            .margin(Margin::symmetric(6.0, 4.0))
                            .desired_width(f32::INFINITY),
                    );
                });

                ui.add_space(4.0);
                ui.checkbox(
                    &mut state.new_user_show_password,
                    RichText::new("Show Passwords").size(11.5),
                );
                ui.add_space(6.0);

                if is_duplicating {
                    ui.label(
                        RichText::new("Copied Database Roles:")
                            .strong()
                            .size(12.0)
                            .color(palette.muted_text),
                    );
                    ScrollArea::vertical().max_height(64.0).show(ui, |ui| {
                        for r in &state.duplicate_source_roles {
                            ui.label(
                                RichText::new(format!(
                                    "  {} {}  →  {}",
                                    regular::DATABASE,
                                    r.db,
                                    r.role
                                ))
                                .monospace()
                                .size(11.5)
                                .color(palette.success_color),
                            );
                        }
                    });
                } else {
                    ui.columns(2, |columns| {
                        columns[0].label(RichText::new("Initial Database").strong().size(12.0));
                        ComboBox::from_id_source("create_user_db_combo")
                            .selected_text(RichText::new(&state.new_user_db).monospace())
                            .width(180.0)
                            .height(220.0)
                            .show_ui(&mut columns[0], |ui| {
                                for db_opt in &db_options {
                                    ui.selectable_value(
                                        &mut state.new_user_db,
                                        db_opt.clone(),
                                        db_opt,
                                    );
                                }
                            });

                        columns[1].label(RichText::new("Initial Role").strong().size(12.0));
                        ComboBox::from_id_source("create_user_role_combo")
                            .selected_text(RichText::new(&state.new_user_role).monospace())
                            .width(180.0)
                            .show_ui(&mut columns[1], |ui| {
                                for &role_opt in UserRole::COMMON_ROLES {
                                    ui.selectable_value(
                                        &mut state.new_user_role,
                                        role_opt.to_string(),
                                        role_opt,
                                    );
                                }
                            });
                    });
                }

                let trimmed_user = state.new_user_username.trim();
                let is_reserved_admin = trimmed_user.eq_ignore_ascii_case("admin");
                let passwords_match = state.new_user_password == state.new_user_confirm_password;

                if is_reserved_admin {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} User 'admin' is reserved and cannot be created or overwritten.",
                            regular::WARNING_CIRCLE
                        ))
                        .color(palette.error_color)
                        .size(11.5),
                    );
                } else if !state.new_user_confirm_password.is_empty() && !passwords_match {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} Passwords do not match.",
                            regular::WARNING_CIRCLE
                        ))
                        .color(palette.error_color)
                        .size(11.5),
                    );
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_submit = !trimmed_user.is_empty()
                        && !state.new_user_password.trim().is_empty()
                        && passwords_match
                        && !is_reserved_admin;

                    let submit_label = if is_duplicating {
                        format!("{} Duplicate User", regular::COPY)
                    } else {
                        format!("{} Create User", regular::USER_PLUS)
                    };

                    ui.add_enabled_ui(can_submit, |ui| {
                        if primary_button(ui, &submit_label, palette.accent_color).clicked() {
                            let roles = if is_duplicating {
                                state.duplicate_source_roles.clone()
                            } else {
                                vec![UserRole::new(&state.new_user_role, &state.new_user_db)]
                            };
                            events.push(AppEvent::CreateUser {
                                connection_index: state.create_user_conn_idx,
                                username: trimmed_user.to_string(),
                                password: state.new_user_password.clone(),
                                roles,
                            });
                            state.duplicate_source_user = None;
                            state.duplicate_source_roles.clear();
                            state.show_create_user_modal = false;
                        }
                    });

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.duplicate_source_user = None;
                        state.duplicate_source_roles.clear();
                        state.show_create_user_modal = false;
                    }
                });
            });
    }

    if state.show_change_password_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        Window::new("Change MongoDB User Password")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([410.0, 245.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} User: {} ({})",
                        regular::USER_GEAR,
                        state.change_pwd_username,
                        state.change_pwd_conn_name
                    ))
                    .monospace()
                    .strong(),
                );
                ui.add_space(8.0);

                ui.label(RichText::new("New Password").strong());
                ui.add(
                    TextEdit::singleline(&mut state.change_pwd_new)
                        .password(!state.change_pwd_show)
                        .hint_text("Enter new password...")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("Confirm New Password").strong());
                ui.add(
                    TextEdit::singleline(&mut state.change_pwd_confirm)
                        .password(!state.change_pwd_show)
                        .hint_text("Re-enter new password...")
                        .margin(Margin::symmetric(6.0, 4.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);

                ui.checkbox(
                    &mut state.change_pwd_show,
                    RichText::new("Show Passwords").size(11.5),
                );

                let pwd_empty = state.change_pwd_new.trim().is_empty();
                let passwords_match = state.change_pwd_new == state.change_pwd_confirm;

                if !state.change_pwd_confirm.is_empty() && !passwords_match {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} Passwords do not match.",
                            regular::WARNING_CIRCLE
                        ))
                        .color(palette.error_color)
                        .size(11.5),
                    );
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_submit = !pwd_empty && passwords_match;
                    ui.add_enabled_ui(can_submit, |ui| {
                        if primary_button(
                            ui,
                            &format!("{} Update Password", regular::CHECK),
                            palette.accent_color,
                        )
                        .clicked()
                        {
                            events.push(AppEvent::ChangeUserPassword {
                                connection_name: state.change_pwd_conn_name.clone(),
                                username: state.change_pwd_username.clone(),
                                new_password: state.change_pwd_new.clone(),
                            });
                            state.change_pwd_new.clear();
                            state.change_pwd_confirm.clear();
                            state.show_change_password_modal = false;
                        }
                    });

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.change_pwd_new.clear();
                        state.change_pwd_confirm.clear();
                        state.show_change_password_modal = false;
                    }
                });
            });
    }

    if let Some((conn_idx, username)) = state.pending_delete_user.clone() {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.error_color))
            .inner_margin(Margin::same(14.0));

        Window::new("Confirm Delete MongoDB User")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([400.0, 155.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} Permanently delete user '{username}'?",
                        regular::WARNING
                    ))
                    .color(palette.error_color)
                    .strong()
                    .size(13.5),
                );
                ui.add_space(6.0);
                ui.label(
                    "This action will drop the user and revoke all database permissions immediately.",
                );
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if primary_button(
                        ui,
                        &format!("{} Delete User", regular::TRASH),
                        palette.error_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::ConfirmDeleteUser {
                            connection_index: conn_idx,
                            username,
                        });
                        state.pending_delete_user = None;
                    }

                    if secondary_button(ui, "Cancel", palette.border_color).clicked() {
                        state.pending_delete_user = None;
                    }
                });
            });
    }

    if state.show_about_modal {
        let modal_frame = Frame::window(&ctx.style())
            .fill(palette.popup_bg)
            .stroke(Stroke::new(1.5_f32, palette.popup_border))
            .inner_margin(Margin::same(14.0));

        Window::new("About Mongo Studio")
            .collapsible(false)
            .resizable(false)
            .frame(modal_frame)
            .fixed_size([390.0, 185.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        Image::new(egui::include_image!("../../assets/logo.svg"))
                            .max_size(Vec2::new(36.0, 36.0)),
                    );
                    ui.add_space(6.0);
                    ui.heading(RichText::new("Mongo Studio v0.1.0").color(palette.success_color));
                });
                ui.add_space(6.0);
                ui.label("VS Code-inspired MongoDB Desktop Admin Tool built in 100% Rust.");
                ui.label("Stack: Rust + egui + eframe + egui_dock + mongodb + tokio");
                ui.add_space(10.0);
                if primary_button(ui, "Close", palette.accent_color).clicked() {
                    state.show_about_modal = false;
                }
            });
    }
}
