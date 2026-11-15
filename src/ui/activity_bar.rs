use egui::{
    Color32, Context, CursorIcon, FontId, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Ui,
    Vec2,
};
use egui_phosphor::regular;
use serde::{Deserialize, Serialize};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::theme::{ThemeMode, ThemePalette};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityView {
    Connections,
    SavedQueries,
    Aggregation,
    SchemaAnalyzer,
    Settings,
}

impl ActivityView {
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Connections => regular::PLUGS_CONNECTED,
            Self::SavedQueries => regular::MAGNIFYING_GLASS,
            Self::Aggregation => regular::LIGHTNING,
            Self::SchemaAnalyzer => regular::CHART_BAR,
            Self::Settings => regular::GEAR,
        }
    }

    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Connections => "Connections Explorer (Ctrl+B)",
            Self::SavedQueries => "Saved Queries & Snippets",
            Self::Aggregation => "Aggregation Pipelines",
            Self::SchemaAnalyzer => "Schema Analyzer",
            Self::Settings => "Settings & Shortcuts",
        }
    }
}

fn draw_activity_icon(
    ui: &mut Ui,
    icon: &'static str,
    tooltip: &str,
    is_active: bool,
    fg: Color32,
    accent: Color32,
) -> egui::Response {
    let desired_size = Vec2::new(36.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = resp.hovered();
        let icon_color = if is_active || hovered {
            Color32::WHITE
        } else {
            fg
        };

        if is_active {
            // Subtle selection pill background
            painter.rect_filled(
                rect,
                Rounding::same(5.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            );
            // Crisp VS Code active indicator on the far left edge of the panel
            let panel_left = ui.max_rect().left();
            let indicator = Rect::from_min_max(
                Pos2::new(panel_left, rect.top() + 4.0),
                Pos2::new(panel_left + 2.5, rect.bottom() - 4.0),
            );
            painter.rect_filled(
                indicator,
                Rounding {
                    nw: 0.0,
                    sw: 0.0,
                    ne: 1.5,
                    se: 1.5,
                },
                accent,
            );
        } else if hovered {
            painter.rect_filled(
                rect,
                Rounding::same(5.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, 14),
            );
        }

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(20.0),
            icon_color,
        );
    }

    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

fn draw_activity_action_button(
    ui: &mut Ui,
    icon: &'static str,
    tooltip: &str,
    fg: Color32,
) -> egui::Response {
    let desired_size = Vec2::new(36.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = resp.hovered();
        let icon_color = if hovered { Color32::WHITE } else { fg };

        if hovered {
            painter.rect_filled(
                rect,
                Rounding::same(5.0),
                Color32::from_rgba_unmultiplied(255, 255, 255, 14),
            );
        }

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(19.0),
            icon_color,
        );
    }

    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

pub fn render_activity_bar(
    ctx: &Context,
    state: &AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let frame = Frame::none()
        .fill(palette.activity_bar_bg)
        .inner_margin(Margin::symmetric(6.0, 8.0))
        .stroke(Stroke::new(1.0_f32, palette.border_color));

    egui::SidePanel::left("activity_bar_panel")
        .resizable(false)
        .exact_width(48.0)
        .frame(frame)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Top Primary Navigation Items
                let top_items = [
                    ActivityView::Connections,
                    ActivityView::SavedQueries,
                    ActivityView::Aggregation,
                    ActivityView::SchemaAnalyzer,
                ];

                for item in top_items {
                    let is_active = state.active_activity == item && state.sidebar_visible;
                    let fg = if is_active {
                        palette.activity_bar_fg_active
                    } else {
                        palette.activity_bar_fg_inactive
                    };

                    if draw_activity_icon(
                        ui,
                        item.icon(),
                        item.tooltip(),
                        is_active,
                        fg,
                        palette.accent_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::SelectActivity(item));
                    }
                    ui.add_space(6.0);
                }

                // Bottom VS Code Actions: Theme Toggle, Command Palette, Settings
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    // Settings at bottom per VS Code standard
                    let is_settings_active =
                        state.active_activity == ActivityView::Settings && state.sidebar_visible;
                    let settings_fg = if is_settings_active {
                        palette.activity_bar_fg_active
                    } else {
                        palette.activity_bar_fg_inactive
                    };
                    if draw_activity_icon(
                        ui,
                        ActivityView::Settings.icon(),
                        ActivityView::Settings.tooltip(),
                        is_settings_active,
                        settings_fg,
                        palette.accent_color,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::SelectActivity(ActivityView::Settings));
                    }

                    ui.add_space(6.0);

                    // Theme Toggle
                    let theme_icon = match state.theme_mode {
                        ThemeMode::Dark => regular::MOON,
                        ThemeMode::Light => regular::SUN,
                    };
                    if draw_activity_action_button(
                        ui,
                        theme_icon,
                        "Switch Dark / Light Theme",
                        palette.activity_bar_fg_inactive,
                    )
                    .clicked()
                    {
                        let next = match state.theme_mode {
                            ThemeMode::Dark => ThemeMode::Light,
                            ThemeMode::Light => ThemeMode::Dark,
                        };
                        events.push(AppEvent::SwitchTheme(next));
                    }

                    ui.add_space(6.0);

                    // Command Palette
                    if draw_activity_action_button(
                        ui,
                        regular::TERMINAL_WINDOW,
                        "Command Palette (Ctrl+P)",
                        palette.activity_bar_fg_inactive,
                    )
                    .clicked()
                    {
                        events.push(AppEvent::ToggleCommandPalette);
                    }
                });
            });
        });
}
