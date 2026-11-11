pub mod dark;
pub mod light;

use egui::{Color32, Context};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
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

pub fn apply_theme(ctx: &Context, mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::Dark => dark::apply_dark_theme(ctx),
        ThemeMode::Light => light::apply_light_theme(ctx),
    }
}

pub fn palette(mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::Dark => dark::dark_palette(),
        ThemeMode::Light => light::light_palette(),
    }
}
