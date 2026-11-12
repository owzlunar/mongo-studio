use egui::{epaint::Shadow, Color32, Context, Rounding, Stroke, Vec2, Visuals};

use super::ThemePalette;

#[must_use]
pub const fn light_palette() -> ThemePalette {
    ThemePalette {
        activity_bar_bg: Color32::from_rgb(44, 44, 44),
        activity_bar_fg_active: Color32::from_rgb(255, 255, 255),
        activity_bar_fg_inactive: Color32::from_rgb(160, 160, 160),
        sidebar_bg: Color32::from_rgb(243, 243, 243),
        sidebar_header_bg: Color32::from_rgb(230, 230, 230),
        editor_bg: Color32::from_rgb(255, 255, 255),
        panel_bg: Color32::from_rgb(248, 248, 248),
        popup_bg: Color32::from_rgb(255, 255, 255),
        popup_border: Color32::from_rgb(175, 180, 190),
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

#[must_use]
pub fn apply_light_theme(ctx: &Context) -> ThemePalette {
    let p = light_palette();
    let mut visuals = Visuals::light();

    visuals.panel_fill = p.editor_bg;
    visuals.window_fill = p.popup_bg;
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
    visuals.faint_bg_color = Color32::from_rgb(245, 245, 245);
    visuals.selection.bg_fill = Color32::from_rgb(173, 214, 255);
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 90, 158));

    visuals.widgets.noninteractive.bg_fill = p.popup_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border_color);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(236, 238, 242);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(200, 204, 212));
    visuals.widgets.inactive.rounding = Rounding::same(3.0);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(214, 232, 255);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.hovered.rounding = Rounding::same(3.0);

    visuals.widgets.open.bg_fill = Color32::from_rgb(214, 232, 255);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.open.rounding = Rounding::same(3.0);

    visuals.window_rounding = Rounding::same(5.0);
    visuals.menu_rounding = Rounding::same(4.0);
    visuals.window_stroke = Stroke::new(1.0_f32, p.popup_border);
    visuals.popup_shadow = Shadow {
        offset: Vec2::new(0.0, 3.0),
        blur: 10.0,
        spread: 0.0,
        color: Color32::from_black_alpha(45),
    };
    visuals.window_shadow = visuals.popup_shadow;

    ctx.set_visuals(visuals);
    p
}
