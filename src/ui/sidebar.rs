use egui::{Context, CursorIcon, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit, Vec2};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::theme::ThemePalette;
use crate::ui::activity_bar::ActivityView;
use crate::ui::components::buttons::primary_button;
use crate::ui::components::tree_view::render_connection_tree;

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
        .show(ctx, |ui| {
            match state.active_activity {
                ActivityView::Connections => {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("CONNECTIONS")
                                .size(11.5)
                                .strong()
                                .color(palette.muted_text),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let refresh_btn =
                                egui::Button::new(RichText::new(regular::ARROWS_CLOCKWISE).size(13.0))
                                    .min_size(Vec2::new(22.0, 22.0));
                            if ui
                                .add(refresh_btn)
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("Refresh Explorer")
                                .clicked()
                            {
                                events.push(AppEvent::RefreshConnections);
                            }

                            let add_btn =
                                egui::Button::new(RichText::new(regular::PLUS).strong().size(13.0))
                                    .min_size(Vec2::new(22.0, 22.0));
                            if ui
                                .add(add_btn)
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("Add Connection")
                                .clicked()
                            {
                                events.push(AppEvent::OpenNewConnectionModal);
                            }
                        });
                    });

                    ui.add_space(6.0);

                    ui.add(
                        TextEdit::singleline(&mut state.sidebar_search)
                            .hint_text("Filter databases / collections...")
                            .margin(Margin::symmetric(6.0, 4.0))
                            .min_size(Vec2::new(120.0, 24.0))
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            render_connection_tree(
                                ui,
                                &mut state.connections,
                                &state.sidebar_search,
                                palette,
                                events,
                            );

                            ui.add_space(12.0);
                            if primary_button(
                                ui,
                                &format!("{} Add Connection", regular::PLUS),
                                palette.accent_color,
                            )
                            .clicked()
                            {
                                events.push(AppEvent::OpenNewConnectionModal);
                            }
                        });
                }
                ActivityView::SavedQueries => {
                    ui.label(
                        RichText::new("SAVED QUERIES")
                            .size(11.5)
                            .strong()
                            .color(palette.muted_text),
                    );
                    ui.separator();
                    ui.add_space(6.0);

                    let presets = [
                        ("Active Users", "admin", "users", "{\"status\": \"active\"}"),
                        ("All Products", "admin", "products", "{}"),
                        ("Pending Orders", "admin", "orders", "{\"status\": \"active\"}"),
                        ("VIP Customers", "shop_db", "customers", "{\"role\": \"admin\"}"),
                    ];

                    for (title, db, coll, filter) in presets {
                        if ui
                            .selectable_label(
                                false,
                                format!("{} {title} ({db}.{coll})", regular::FILE_TEXT),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text(filter)
                            .clicked()
                        {
                            events.push(AppEvent::OpenCollection {
                                connection_name: "MongoDB Local".to_string(),
                                database: db.to_string(),
                                collection: coll.to_string(),
                            });
                            events.push(AppEvent::ExecuteFilterQuery {
                                database: db.to_string(),
                                collection: coll.to_string(),
                                filter: filter.to_string(),
                            });
                        }
                    }
                }
                ActivityView::Aggregation => {
                    ui.label(
                        RichText::new("AGGREGATION PIPELINES")
                            .size(11.5)
                            .strong()
                            .color(palette.muted_text),
                    );
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label("Select a collection tab and switch to the 'Aggregation' sub-tab to build stages.");
                }
                ActivityView::SchemaAnalyzer => {
                    ui.label(
                        RichText::new("SCHEMA ANALYZER")
                            .size(11.5)
                            .strong()
                            .color(palette.muted_text),
                    );
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label("Inspect BSON field types and frequency distribution across sampled documents.");
                }
                ActivityView::Settings => {
                    ui.label(
                        RichText::new("SETTINGS")
                            .size(11.5)
                            .strong()
                            .color(palette.muted_text),
                    );
                    ui.separator();
                }
            }
        });
}
