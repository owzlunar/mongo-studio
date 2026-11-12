use egui::{Color32, Context, CursorIcon, FontId, Frame, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
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
            Self::Settings => "Settings & Theme",
        }
    }
}

fn draw_activity_icon(
    ui: &mut Ui,
    item: ActivityView,
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
            painter.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
            let indicator = Rect::from_min_max(
                Pos2::new(rect.left(), rect.top() + 6.0),
                Pos2::new(rect.left() + 2.5, rect.bottom() - 6.0),
            );
            painter.rect_filled(indicator, 1.0, accent);
        } else if hovered {
            painter.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
        }

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            item.icon(),
            FontId::proportional(20.0),
            icon_color,
        );
    }

    resp.on_hover_cursor(CursorIcon::PointingHand)
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
                let top_items = [
                    ActivityView::Connections,
                    ActivityView::SavedQueries,
                    ActivityView::Aggregation,
                    ActivityView::SchemaAnalyzer,
                    ActivityView::Settings,
                ];

                for item in top_items {
                    let is_active = state.active_activity == item && state.sidebar_visible;
                    let fg = if is_active {
                        palette.activity_bar_fg_active
                    } else {
                        palette.activity_bar_fg_inactive
                    };

                    if draw_activity_icon(ui, item, is_active, fg, palette.accent_color)
                        .on_hover_text(item.tooltip())
                        .clicked()
                    {
                        events.push(AppEvent::SelectActivity(item));
                    }
                    ui.add_space(6.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    let theme_icon = match state.theme_mode {
                        ThemeMode::Dark => regular::MOON,
                        ThemeMode::Light => regular::SUN,
                    };
                    let theme_btn = egui::Button::new(
                        RichText::new(theme_icon)
                            .size(18.0)
                            .color(palette.activity_bar_fg_inactive),
                    )
                    .frame(false)
                    .min_size(Vec2::new(36.0, 36.0));

                    if ui
                        .add(theme_btn)
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Switch Dark / Light Theme")
                        .clicked()
                    {
                        let next = match state.theme_mode {
                            ThemeMode::Dark => ThemeMode::Light,
                            ThemeMode::Light => ThemeMode::Dark,
                        };
                        events.push(AppEvent::SwitchTheme(next));
                    }

                    ui.add_space(4.0);

                    let cmd_btn = egui::Button::new(
                        RichText::new(regular::TERMINAL_WINDOW)
                            .size(18.0)
                            .color(palette.activity_bar_fg_inactive),
                    )
                    .frame(false)
                    .min_size(Vec2::new(36.0, 36.0));

                    if ui
                        .add(cmd_btn)
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Command Palette (Ctrl+P)")
                        .clicked()
                    {
                        events.push(AppEvent::ToggleCommandPalette);
                    }
                });
            });
        });
}
