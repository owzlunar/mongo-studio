use egui::{Color32, Context, Rounding, Stroke, Visuals};

use super::ThemePalette;

pub fn light_palette() -> ThemePalette {
    ThemePalette {
        activity_bar_bg: Color32::from_rgb(44, 44, 44),
        activity_bar_fg_active: Color32::from_rgb(255, 255, 255),
        activity_bar_fg_inactive: Color32::from_rgb(160, 160, 160),
        sidebar_bg: Color32::from_rgb(243, 243, 243),
        sidebar_header_bg: Color32::from_rgb(230, 230, 230),
        editor_bg: Color32::from_rgb(255, 255, 255),
        panel_bg: Color32::from_rgb(248, 248, 248),
        tab_active_bg: Color32::from_rgb(255, 255, 255),
        tab_inactive_bg: Color32::from_rgb(236, 236, 236),
        status_bar_bg: Color32::from_rgb(0, 122, 204),
        status_bar_fg: Color32::from_rgb(255, 255, 255),
        border_color: Color32::from_rgb(218, 218, 218),
        accent_color: Color32::from_rgb(0, 122, 204),
        success_color: Color32::from_rgb(22, 130, 93),
        warning_color: Color32::from_rgb(180, 120, 0),
        error_color: Color32::from_rgb(211, 47, 47),
        muted_text: Color32::from_rgb(110, 110, 110),
    }
}

pub fn apply_light_theme(ctx: &Context) -> ThemePalette {
    let p = light_palette();
    let mut visuals = Visuals::light();

    visuals.panel_fill = p.editor_bg;
    visuals.window_fill = p.sidebar_bg;
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
    visuals.faint_bg_color = Color32::from_rgb(245, 245, 245);
    visuals.selection.bg_fill = Color32::from_rgb(173, 214, 255);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border_color);
    visuals.widgets.inactive.rounding = Rounding::same(3.0);
    visuals.widgets.hovered.rounding = Rounding::same(3.0);
    visuals.window_rounding = Rounding::same(4.0);

    ctx.set_visuals(visuals);
    p
}
