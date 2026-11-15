use egui::{Context, CursorIcon, Frame, Margin, RichText};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::models::ConnectionStatus;
use crate::theme::{ThemeMode, ThemePalette};

pub fn render_status_bar(
    ctx: &Context,
    state: &AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let frame = Frame::none()
        .fill(palette.status_bar_bg)
        .inner_margin(Margin::symmetric(10.0, 2.0));

    egui::TopBottomPanel::bottom("status_bar_panel")
        .resizable(false)
        .exact_height(24.0)
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(conn) = state.connections.get(state.active_connection_idx) {
                    let status_text = match conn.status {
                        ConnectionStatus::Connected => format!("{} Connected", regular::RECORD),
                        ConnectionStatus::Connecting => {
                            format!("{} Connecting", regular::CIRCLE_HALF)
                        }
                        ConnectionStatus::Disconnected => {
                            format!("{} Disconnected", regular::CIRCLE)
                        }
                        ConnectionStatus::Error => format!("{} Error", regular::X_CIRCLE),
                    };

                    ui.label(
                        RichText::new(status_text)
                            .color(palette.status_bar_fg)
                            .strong()
                            .size(12.0),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} {}", regular::PLUGS_CONNECTED, conn.host))
                            .color(palette.status_bar_fg)
                            .monospace()
                            .size(12.0),
                    );
                }

                ui.separator();
                if ui
                    .small_button(
                        RichText::new(if state.bottom_panel.visible {
                            format!("{} Panel", regular::CARET_DOWN)
                        } else {
                            format!("{} Panel", regular::CARET_UP)
                        })
                        .color(palette.status_bar_fg)
                        .size(11.5),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Toggle Terminal / Output Panel (Ctrl+J)")
                    .clicked()
                {
                    events.push(AppEvent::ToggleBottomPanel);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} MongoDB", regular::DATABASE))
                            .color(palette.status_bar_fg)
                            .strong()
                            .size(12.0),
                    );
                    ui.separator();
                    let mode_str = match state.theme_mode {
                        ThemeMode::Dark => "Dark",
                        ThemeMode::Light => "Light",
                    };
                    ui.label(
                        RichText::new(format!("Theme: {mode_str}"))
                            .color(palette.status_bar_fg)
                            .size(11.5),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("Phase 2 Live Driver (Tokio Async)")
                            .color(palette.status_bar_fg)
                            .size(11.5),
                    );
                });
            });
        });
}
