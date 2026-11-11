use egui::{Context, Frame, Margin, RichText, Stroke, Vec2};
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
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Connections => "🗄",
            Self::SavedQueries => "🔍",
            Self::Aggregation => "⚡",
            Self::SchemaAnalyzer => "📊",
            Self::Settings => "⚙",
        }
    }

    pub fn tooltip(&self) -> &'static str {
        match self {
            Self::Connections => "Connections Explorer (Ctrl+B)",
            Self::SavedQueries => "Saved Queries & Snippets",
            Self::Aggregation => "Aggregation Pipelines",
            Self::SchemaAnalyzer => "Schema Analyzer",
            Self::Settings => "Settings & Theme",
        }
    }
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
        .stroke(Stroke::new(1.0, palette.border_color));

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
                ];

                for item in top_items {
                    let is_active = state.active_activity == item && state.sidebar_visible;
                    let fg = if is_active {
                        palette.activity_bar_fg_active
                    } else {
                        palette.activity_bar_fg_inactive
                    };

                    let btn = egui::Button::new(RichText::new(item.icon()).size(20.0).color(fg))
                        .frame(is_active)
                        .min_size(Vec2::new(36.0, 36.0));

                    if ui.add(btn).on_hover_text(item.tooltip()).clicked() {
                        events.push(AppEvent::SelectActivity(item));
                    }
                    ui.add_space(6.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    let theme_icon = match state.theme_mode {
                        ThemeMode::Dark => "🌙",
                        ThemeMode::Light => "☀",
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
                        RichText::new("⌘")
                            .size(18.0)
                            .color(palette.activity_bar_fg_inactive),
                    )
                    .frame(false)
                    .min_size(Vec2::new(36.0, 36.0));

                    if ui
                        .add(cmd_btn)
                        .on_hover_text("Command Palette (Ctrl+Shift+P)")
                        .clicked()
                    {
                        events.push(AppEvent::ToggleCommandPalette);
                    }
                });
            });
        });
}
