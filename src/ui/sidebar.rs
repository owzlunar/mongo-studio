use egui::{
    ComboBox, Context, CursorIcon, Frame, Margin, RichText, Rounding, ScrollArea, Stroke, TextEdit,
    Vec2,
};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::database::ConnectionManager;
use crate::models::{QueryStore, QuickAccessFilter, WorkspaceTree};
use crate::query::AggregationStage;
use crate::theme::{ThemeMode, ThemePalette};
use crate::ui::activity_bar::ActivityView;
use crate::ui::components::buttons::{primary_button, secondary_button};
use crate::ui::components::tree_view::render_connection_tree;
use crate::workspace::{CollectionSubTab, WorkspacePersistence};

fn render_saved_queries_sidebar(
    ui: &mut egui::Ui,
    state: &AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("SAVED QUERIES")
                .size(11.5)
                .strong()
                .color(palette.muted_text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("({})", state.saved_queries.len()))
                    .size(11.0)
                    .color(palette.muted_text),
            );
        });
    });
    ui.separator();
    ui.add_space(6.0);

    if state.saved_queries.is_empty() {
        ui.label(
            RichText::new("No saved queries yet. Use 'Save Query' inside any collection tab.")
                .size(12.0)
                .color(palette.muted_text),
        );
        return;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for sq in &state.saved_queries {
                let tooltip = format!(
                    "Target: {} / {}.{}\nFilter: {}\nProjection: {}\nSort: {}\nLimit: {}",
                    sq.connection_name,
                    sq.database,
                    sq.collection,
                    sq.filter,
                    sq.projection,
                    sq.sort,
                    sq.limit
                );

                Frame::none()
                    .fill(palette.panel_bg)
                    .inner_margin(Margin::symmetric(8.0, 6.0))
                    .rounding(Rounding::same(4.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_color))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let run_resp = ui
                                .selectable_label(
                                    false,
                                    RichText::new(format!("{} {}", regular::FILE_TEXT, sq.name))
                                        .strong()
                                        .color(palette.text_primary),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text(&tooltip);

                            if run_resp.clicked() {
                                events.push(AppEvent::RunSavedQuery { query: sq.clone() });
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button(
                                            RichText::new(regular::TRASH)
                                                .color(palette.error_color),
                                        )
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text("Delete saved query")
                                        .clicked()
                                    {
                                        events.push(AppEvent::DeleteSavedQuery { id: sq.id });
                                    }
                                },
                            );
                        });

                        ui.label(
                            RichText::new(format!(
                                "{} / {}.{}",
                                sq.connection_name, sq.database, sq.collection
                            ))
                            .monospace()
                            .size(11.0)
                            .color(palette.muted_text),
                        )
                        .on_hover_text(tooltip);
                    });
                ui.add_space(4.0);
            }
        });
}

fn render_aggregation_sidebar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    ui.label(
        RichText::new("AGGREGATION PIPELINES")
            .size(11.5)
            .strong()
            .color(palette.muted_text),
    );
    ui.separator();
    ui.add_space(6.0);

    let maybe_tab = state
        .dock_state
        .iter_all_tabs_mut()
        .map(|(_, t)| t)
        .find(|t| t.selected_user.is_none());

    let Some(tab) = maybe_tab else {
        ui.label(
            RichText::new("Open a collection from Connections Explorer first to build an aggregation pipeline.")
                .color(palette.muted_text),
        );
        ui.add_space(8.0);
        if secondary_button(
            ui,
            &format!("{} Go to Connections", regular::PLUGS_CONNECTED),
            palette.border_color,
        )
        .clicked()
        {
            events.push(AppEvent::SelectActivity(ActivityView::Connections));
        }
        return;
    };

    ui.label(
        RichText::new(format!(
            "{} Target: {}",
            regular::TABLE,
            tab.full_namespace()
        ))
        .monospace()
        .strong()
        .color(palette.accent_color),
    );
    ui.label(
        RichText::new(format!("Connection: {}", tab.connection_name))
            .size(11.5)
            .color(palette.muted_text),
    );
    ui.add_space(8.0);

    if primary_button(
        ui,
        &format!("{} Open Aggregation Pipeline", regular::FUNNEL),
        palette.accent_color,
    )
    .clicked()
    {
        tab.active_subtab = CollectionSubTab::Aggregation;
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.label(
        RichText::new("QUICK PRESETS")
            .size(11.0)
            .strong()
            .color(palette.muted_text),
    );
    ui.add_space(4.0);

    let presets: [(&str, &str, Vec<AggregationStage>); 3] = [
        (
            "Count by Status",
            "Group documents by '$status' and sort descending by count",
            vec![
                AggregationStage::new(
                    "$group",
                    "{\n  \"_id\": \"$status\",\n  \"count\": { \"$sum\": 1 }\n}",
                    true,
                ),
                AggregationStage::new("$sort", "{\n  \"count\": -1\n}", true),
            ],
        ),
        (
            "Recent Top 10",
            "Sort by '_id' descending and limit to 10 documents",
            vec![
                AggregationStage::new("$sort", "{\n  \"_id\": -1\n}", true),
                AggregationStage::new("$limit", "10", true),
            ],
        ),
        (
            "Unwind Array",
            "Unwind '$items' array and limit to 20 documents",
            vec![
                AggregationStage::new("$unwind", "\"$items\"", true),
                AggregationStage::new("$limit", "20", true),
            ],
        ),
    ];

    for (label, hint, stages) in presets {
        if secondary_button(
            ui,
            &format!("{} {label}", regular::LIGHTNING),
            palette.border_color,
        )
        .on_hover_text(hint)
        .clicked()
        {
            tab.agg_stages = stages;
            tab.agg_raw_mode = false;
            if let Ok(pretty) = AggregationStage::stages_to_pretty_json(&tab.agg_stages) {
                tab.agg_raw_json = pretty;
            }
            tab.agg_error = None;
            tab.active_subtab = CollectionSubTab::Aggregation;
        }
        ui.add_space(4.0);
    }
}

fn render_schema_sidebar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    ui.label(
        RichText::new("SCHEMA ANALYZER")
            .size(11.5)
            .strong()
            .color(palette.muted_text),
    );
    ui.separator();
    ui.add_space(6.0);

    let target_info = {
        let maybe_tab = state
            .dock_state
            .iter_all_tabs_mut()
            .map(|(_, t)| t)
            .find(|t| t.selected_user.is_none());

        if let Some(tab) = maybe_tab {
            ui.label(
                RichText::new(format!(
                    "{} Active: {}",
                    regular::TABLE,
                    tab.full_namespace()
                ))
                .monospace()
                .strong()
                .color(palette.accent_color),
            );
            ui.label(
                RichText::new(format!("Connection: {}", tab.connection_name))
                    .size(11.5)
                    .color(palette.muted_text),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Sample Size")
                        .size(11.5)
                        .strong()
                        .color(palette.text_primary),
                );
                ComboBox::from_id_source("sidebar_schema_sample_size")
                    .selected_text(RichText::new(tab.schema_sample_size.to_string()).monospace())
                    .width(74.0)
                    .show_ui(ui, |ui| {
                        for sample_opt in [50_usize, 100, 250, 500, 1000] {
                            ui.selectable_value(
                                &mut tab.schema_sample_size,
                                sample_opt,
                                sample_opt.to_string(),
                            );
                        }
                    });
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.add_enabled_ui(!tab.schema_is_analyzing, |ui| {
                    if primary_button(
                        ui,
                        &format!("{} Analyze Active Collection", regular::CHART_BAR),
                        palette.accent_color,
                    )
                    .clicked()
                    {
                        tab.schema_is_analyzing = true;
                        tab.active_subtab = CollectionSubTab::Schema;
                        events.push(AppEvent::AnalyzeCollectionSchema {
                            connection_name: tab.connection_name.clone(),
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            sample_size: tab.schema_sample_size,
                        });
                    }
                });
                if tab.schema_is_analyzing {
                    ui.spinner();
                }
            });

            Some((
                tab.connection_name.clone(),
                tab.database.clone(),
                tab.collection.clone(),
            ))
        } else {
            None
        }
    };

    let Some((conn_name, db_name, coll_name)) = target_info else {
        ui.label(
            RichText::new(
                "Open a collection tab to inspect nested BSON field paths and type distributions.",
            )
            .color(palette.muted_text),
        );
        return;
    };

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    let Some(coll) = state.find_collection_by_conn(&conn_name, &db_name, &coll_name) else {
        ui.label(
            RichText::new("Collection metadata not loaded yet.")
                .size(11.5)
                .color(palette.muted_text),
        );
        return;
    };

    ui.label(
        RichText::new(format!("DISCOVERED FIELDS ({})", coll.schema_fields.len()))
            .size(11.0)
            .strong()
            .color(palette.muted_text),
    );
    ui.add_space(4.0);

    if coll.schema_fields.is_empty() {
        ui.label(
            RichText::new("No schema fields yet — click 'Analyze Active Collection'.")
                .size(11.5)
                .color(palette.muted_text),
        );
        return;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for field in &coll.schema_fields {
                Frame::none()
                    .fill(palette.panel_bg)
                    .inner_margin(Margin::symmetric(6.0, 4.0))
                    .rounding(Rounding::same(3.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_color))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&field.field_name)
                                    .monospace()
                                    .strong()
                                    .size(11.5)
                                    .color(palette.text_primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{:.0}%", field.frequency_pct))
                                            .monospace()
                                            .size(11.0)
                                            .color(palette.accent_color),
                                    );
                                },
                            );
                        });
                        let breakdown_text = if field.type_breakdown.is_empty() {
                            field.bson_type.clone()
                        } else {
                            field
                                .type_breakdown
                                .iter()
                                .map(|(ty, pct)| format!("{ty} {pct:.0}%"))
                                .collect::<Vec<_>>()
                                .join(" • ")
                        };
                        ui.label(
                            RichText::new(breakdown_text)
                                .monospace()
                                .size(10.5)
                                .color(palette.success_color),
                        );
                    });
                ui.add_space(3.0);
            }
        });
}

fn render_settings_sidebar(
    ui: &mut egui::Ui,
    state: &AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    ui.label(
        RichText::new("SETTINGS")
            .size(11.5)
            .strong()
            .color(palette.muted_text),
    );
    ui.separator();
    ui.add_space(6.0);

    ui.label(
        RichText::new("Color Theme")
            .strong()
            .size(12.0)
            .color(palette.text_primary),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let is_dark = state.theme_mode == ThemeMode::Dark;
        if ui
            .selectable_label(is_dark, format!("{} Dark", regular::MOON))
            .on_hover_cursor(CursorIcon::PointingHand)
            .clicked()
        {
            events.push(AppEvent::SwitchTheme(ThemeMode::Dark));
        }
        if ui
            .selectable_label(!is_dark, format!("{} Light", regular::SUN))
            .on_hover_cursor(CursorIcon::PointingHand)
            .clicked()
        {
            events.push(AppEvent::SwitchTheme(ThemeMode::Light));
        }
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    ui.label(
        RichText::new("Workspace Layout")
            .strong()
            .size(12.0)
            .color(palette.text_primary),
    );
    ui.add_space(4.0);
    if secondary_button(
        ui,
        &format!("{} Toggle Sidebar (Ctrl+B)", regular::SIDEBAR_SIMPLE),
        palette.border_color,
    )
    .clicked()
    {
        events.push(AppEvent::ToggleSidebar);
    }
    ui.add_space(4.0);
    if secondary_button(
        ui,
        &format!("{} Toggle Bottom Panel (Ctrl+J)", regular::TERMINAL_WINDOW),
        palette.border_color,
    )
    .clicked()
    {
        events.push(AppEvent::ToggleBottomPanel);
    }
    ui.add_space(4.0);
    if secondary_button(
        ui,
        &format!(
            "{} Reset Workspace Layout",
            regular::ARROW_COUNTER_CLOCKWISE
        ),
        palette.border_color,
    )
    .clicked()
    {
        events.push(AppEvent::ResetWorkspaceLayout);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    ui.label(
        RichText::new("Config File Locations")
            .strong()
            .size(12.0)
            .color(palette.text_primary),
    );
    ui.add_space(4.0);

    let config_paths = [
        ("Connections", ConnectionManager::profiles_file_path()),
        ("Saved Queries", QueryStore::config_file_path()),
        ("Workspace", WorkspacePersistence::config_file_path()),
    ];

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (label, maybe_path) in config_paths {
                ui.label(
                    RichText::new(label)
                        .size(11.0)
                        .strong()
                        .color(palette.accent_color),
                );
                let path_str = maybe_path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Unavailable on this platform".to_string());
                ui.label(
                    RichText::new(&path_str)
                        .monospace()
                        .size(10.5)
                        .color(palette.muted_text),
                )
                .on_hover_text(path_str);
                ui.add_space(4.0);
            }
        });
}

fn render_connection_sidebar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    // 1. Sidebar Top Header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} MongoDB Client", regular::DATABASE))
                .size(12.5)
                .strong()
                .color(palette.text_primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh_btn =
                egui::Button::new(RichText::new(regular::ARROWS_CLOCKWISE).size(13.0))
                    .min_size(Vec2::new(20.0, 20.0));
            if ui
                .add(refresh_btn)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Refresh Explorer")
                .clicked()
            {
                events.push(AppEvent::RefreshConnections);
            }
        });
    });

    ui.add_space(6.0);

    // 2. Full-Width Search Bar with ⌘ K Shortcut
    let search_id = ui.make_persistent_id("sidebar_search_input");
    let k_pressed =
        ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::K));

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let response = ui.add(
            TextEdit::singleline(&mut state.sidebar_search)
                .id(search_id)
                .hint_text("Search connections...")
                .margin(Margin::symmetric(6.0, 4.0))
                .min_size(Vec2::new(120.0, 24.0))
                .desired_width(ui.available_width() - 32.0),
        );
        if k_pressed {
            response.request_focus();
        }
        ui.label(
            RichText::new("⌘ K")
                .monospace()
                .size(10.0)
                .color(palette.muted_text),
        );
    });

    ui.add_space(6.0);

    // 3. Quick Access: Favorites & Recent
    let fav_active = state.quick_access_filter == QuickAccessFilter::Favorites;
    let recent_active = state.quick_access_filter == QuickAccessFilter::Recent;
    let fav_count = state.favorite_count();
    let recent_count = state.recent_count();

    ui.horizontal(|ui| {
        let fav_label = RichText::new(format!("{} Favorites", regular::STAR))
            .size(12.0)
            .color(if fav_active {
                palette.accent_color
            } else {
                palette.text_primary
            });
        let fav_btn = ui
            .selectable_label(fav_active, fav_label)
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if fav_active {
                "Showing Favorites only (click to clear)"
            } else {
                "Filter connections by Favorites"
            });
        if fav_btn.clicked() {
            events.push(AppEvent::ToggleQuickAccessFilter {
                filter: QuickAccessFilter::Favorites,
            });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if fav_count > 0 {
                ui.label(
                    RichText::new(fav_count.to_string())
                        .monospace()
                        .size(11.0)
                        .color(palette.muted_text),
                );
            }
        });
    });

    ui.horizontal(|ui| {
        let recent_label = RichText::new(format!("{} Recent", regular::CLOCK))
            .size(12.0)
            .color(if recent_active {
                palette.accent_color
            } else {
                palette.text_primary
            });
        let recent_btn = ui
            .selectable_label(recent_active, recent_label)
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if recent_active {
                "Showing Recent connections only (click to clear)"
            } else {
                "Filter to recent connections"
            });
        if recent_btn.clicked() {
            events.push(AppEvent::ToggleQuickAccessFilter {
                filter: QuickAccessFilter::Recent,
            });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if recent_count > 0 {
                ui.label(
                    RichText::new(recent_count.to_string())
                        .monospace()
                        .size(11.0)
                        .color(palette.muted_text),
                );
            }
        });
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    // 4. WORKSPACES Header with Dropdown & Smart '+' Button
    ui.horizontal(|ui| {
        let current_ws_label = match &state.active_workspace_filter {
            Some(ws) => format!("WORKSPACE: {ws}"),
            None => "WORKSPACES".to_string(),
        };

        ComboBox::from_id_source("sidebar_workspace_filter_dropdown")
            .selected_text(
                RichText::new(current_ws_label)
                    .size(11.0)
                    .strong()
                    .color(palette.muted_text),
            )
            .show_ui(ui, |ui| {
                let is_all = state.active_workspace_filter.is_none();
                if ui.selectable_label(is_all, "🌐 All Workspaces").clicked() {
                    events.push(AppEvent::FilterWorkspaceView {
                        workspace_name: None,
                    });
                }
                let all_workspaces = WorkspaceTree::extract_all_workspaces(&state.connections);
                for ws_name in all_workspaces {
                    let is_sel = state.active_workspace_filter.as_deref() == Some(&ws_name);
                    if ui
                        .selectable_label(is_sel, format!("🏢 {ws_name}"))
                        .clicked()
                    {
                        events.push(AppEvent::FilterWorkspaceView {
                            workspace_name: Some(ws_name),
                        });
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(false, "⚙ Manage Workspaces...")
                    .clicked()
                {
                    events.push(AppEvent::OpenManageWorkspacesModal);
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(
                RichText::new(regular::PLUS)
                    .size(13.0)
                    .strong()
                    .color(palette.text_primary),
                |ui| {
                    if ui
                        .button(format!("{} New Connection...", regular::PLUS))
                        .clicked()
                    {
                        events.push(AppEvent::OpenNewConnectionModal);
                        ui.close_menu();
                    }
                    if ui
                        .button(format!("{} New Project / Folder...", regular::FOLDER_PLUS))
                        .clicked()
                    {
                        events.push(AppEvent::OpenManageWorkspacesModal);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} New Workspace...", regular::BUILDINGS))
                        .clicked()
                    {
                        events.push(AppEvent::OpenManageWorkspacesModal);
                        ui.close_menu();
                    }
                },
            )
            .response
            .on_hover_text("Add Connection, Folder, or Workspace");
        });
    });

    ui.add_space(4.0);

    // 5. Scrollable Tree Area & Pinned Bottom Footer
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        // Bottom Footer: Manage Workspaces
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            let footer_btn = ui
                .selectable_label(
                    false,
                    RichText::new(format!("{} Manage Workspaces", regular::GEAR))
                        .size(12.0)
                        .color(palette.muted_text),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Organize connections into workspaces and folders");
            if footer_btn.clicked() {
                events.push(AppEvent::OpenManageWorkspacesModal);
            }
        });
        ui.separator();
        ui.add_space(2.0);

        // Center / Main Scrollable Connection Tree Area
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_connection_tree(ui, state, palette, events);
                });
        });
    });
}

pub fn render_sidebar(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    if !state.sidebar_visible {
        return;
    }

    let frame = Frame::none()
        .fill(palette.sidebar_bg)
        .inner_margin(Margin::symmetric(10.0, 8.0))
        .stroke(Stroke::new(1.0_f32, palette.border_color));

    egui::SidePanel::left("sidebar_explorer_panel")
        .resizable(true)
        .default_width(250.0)
        .width_range(190.0..=420.0)
        .frame(frame)
        .show(ctx, |ui| match state.active_activity {
            ActivityView::Connections => {
                render_connection_sidebar(ui, state, palette, events);
            }
            ActivityView::SavedQueries => {
                render_saved_queries_sidebar(ui, state, palette, events);
            }
            ActivityView::Aggregation => {
                render_aggregation_sidebar(ui, state, palette, events);
            }
            ActivityView::SchemaAnalyzer => {
                render_schema_sidebar(ui, state, palette, events);
            }
            ActivityView::Settings => {
                render_settings_sidebar(ui, state, palette, events);
            }
        });
}
