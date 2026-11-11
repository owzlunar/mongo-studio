use egui::{Context, Frame, Margin, RichText};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::models::ConnectionStatus;
use crate::theme::ThemePalette;

pub fn render_status_bar(
    ctx: &Context,
    state: &AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let frame = Frame::none()
        .fill(palette.status_bar_bg)
        .inner_margin(Margin::symmetric(10.0, 3.0));

    egui::TopBottomPanel::bottom("status_bar_panel")
        .resizable(false)
        .exact_height(24.0)
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(conn) = state.connections.get(state.active_connection_idx) {
                    let dot = match conn.status {
                        ConnectionStatus::Connected => "● Connected",
                        ConnectionStatus::Connecting => "◌ Connecting",
                        ConnectionStatus::Disconnected => "○ Disconnected",
                        ConnectionStatus::Error => "✖ Error",
                    };

                    ui.label(
                        RichText::new(dot)
                            .color(palette.status_bar_fg)
                            .strong()
                            .size(12.0),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(&conn.host)
                            .color(palette.status_bar_fg)
                            .monospace()
                            .size(12.0),
                    );
                }

                ui.separator();
                if ui
                    .small_button(
                        RichText::new(if state.bottom_panel.visible {
                            "▾ Panel"
                        } else {
                            "▴ Panel"
                        })
                        .color(palette.status_bar_fg)
                        .size(11.5),
                    )
                    .on_hover_text("Toggle Terminal / Output Panel (Ctrl+J)")
                    .clicked()
                {
                    events.push(AppEvent::ToggleBottomPanel);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("MongoDB")
                            .color(palette.status_bar_fg)
                            .strong()
                            .size(12.0),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Theme: {:?}", state.theme_mode))
                            .color(palette.status_bar_fg)
                            .size(11.5),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("Phase 1 Shell (Mock Data)")
                            .color(palette.status_bar_fg)
                            .size(11.5),
                    );
                });
            });
        });
}
