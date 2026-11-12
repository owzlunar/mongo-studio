pub mod dark;
pub mod light;

use egui::{Color32, Context, Rounding, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub activity_bar_bg: Color32,
    pub activity_bar_fg_active: Color32,
    pub activity_bar_fg_inactive: Color32,
    pub sidebar_bg: Color32,
    pub sidebar_header_bg: Color32,
    pub editor_bg: Color32,
    pub panel_bg: Color32,
    pub popup_bg: Color32,
    pub popup_border: Color32,
    pub tab_active_bg: Color32,
    pub tab_inactive_bg: Color32,
    pub status_bar_bg: Color32,
    pub status_bar_fg: Color32,
    pub border_color: Color32,
    pub accent_color: Color32,
    pub success_color: Color32,
    pub warning_color: Color32,
    pub error_color: Color32,
    pub muted_text: Color32,
}

pub(crate) fn configure_common_style(ctx: &Context) {
    ctx.style_mut(|style| {
        // Ensure uniform height (24px) and alignment across buttons, singleline inputs, and combo boxes
        style.spacing.interact_size = Vec2::new(40.0, 24.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.item_spacing = Vec2::new(6.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(10.0);
        style.spacing.menu_margin = egui::Margin::same(6.0);
        style.visuals.menu_rounding = Rounding::same(4.0);
    });
}

#[must_use]
pub fn apply_theme(ctx: &Context, mode: ThemeMode) -> ThemePalette {
    let palette = match mode {
        ThemeMode::Dark => dark::apply_dark_theme(ctx),
        ThemeMode::Light => light::apply_light_theme(ctx),
    };
    configure_common_style(ctx);
    palette
}

#[must_use]
pub const fn palette(mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::Dark => dark::dark_palette(),
        ThemeMode::Light => light::light_palette(),
    }
}
