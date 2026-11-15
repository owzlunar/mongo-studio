use egui::{epaint::Shadow, Color32, Context, Rounding, Stroke, Vec2, Visuals};

use super::ThemePalette;

#[must_use]
pub const fn light_palette() -> ThemePalette {
    ThemePalette {
        activity_bar_bg: Color32::from_rgb(44, 44, 44), // #2c2c2c VS Code Light Activity Bar
        activity_bar_fg_active: Color32::from_rgb(255, 255, 255),
        activity_bar_fg_inactive: Color32::from_rgb(160, 160, 160),
        sidebar_bg: Color32::from_rgb(243, 243, 243), // #f3f3f3
        sidebar_header_bg: Color32::from_rgb(235, 235, 235), // #ebebeb
        editor_bg: Color32::from_rgb(255, 255, 255),  // #ffffff
        panel_bg: Color32::from_rgb(248, 248, 248),   // #f8f8f8
        popup_bg: Color32::from_rgb(255, 255, 255),
        popup_border: Color32::from_rgb(180, 185, 195),
        tab_active_bg: Color32::from_rgb(255, 255, 255),
        tab_inactive_bg: Color32::from_rgb(236, 236, 236),
        status_bar_bg: Color32::from_rgb(0, 122, 204), // #007acc
        status_bar_fg: Color32::from_rgb(255, 255, 255),
        border_color: Color32::from_rgb(218, 218, 218), // #dadada
        accent_color: Color32::from_rgb(0, 122, 204),   // #007acc
        success_color: Color32::from_rgb(22, 130, 93),
        warning_color: Color32::from_rgb(180, 120, 0),
        error_color: Color32::from_rgb(211, 47, 47),
        text_primary: Color32::from_rgb(28, 28, 28),
        muted_text: Color32::from_rgb(100, 100, 100),
    }
}

#[must_use]
pub fn apply_light_theme(ctx: &Context) -> ThemePalette {
    let p = light_palette();
    let mut visuals = Visuals::light();

    visuals.panel_fill = p.editor_bg;
    visuals.window_fill = p.popup_bg;
    visuals.extreme_bg_color = Color32::from_rgb(240, 240, 243);
    visuals.faint_bg_color = Color32::from_rgb(245, 245, 245);
    visuals.selection.bg_fill = Color32::from_rgb(173, 214, 255);
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 90, 158));

    visuals.override_text_color = Some(p.text_primary);

    visuals.widgets.noninteractive.bg_fill = p.popup_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border_color);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text_primary);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(230, 232, 236);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(210, 214, 220));
    visuals.widgets.inactive.rounding = Rounding::same(3.0);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text_primary);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(214, 232, 255);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.hovered.rounding = Rounding::same(3.0);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(10, 10, 10));

    visuals.widgets.active.bg_fill = Color32::from_rgb(180, 214, 255);
    visuals.widgets.active.rounding = Rounding::same(3.0);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::from_rgb(15, 15, 15));

    visuals.widgets.open.bg_fill = Color32::from_rgb(214, 232, 255);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.open.rounding = Rounding::same(3.0);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(10, 10, 10));

    visuals.window_rounding = Rounding::same(5.0);
    visuals.menu_rounding = Rounding::same(4.0);
    visuals.window_stroke = Stroke::new(1.0_f32, p.popup_border);
    visuals.popup_shadow = Shadow {
        offset: Vec2::new(0.0, 3.0),
        blur: 12.0,
        spread: 0.0,
        color: Color32::from_black_alpha(45),
    };
    visuals.window_shadow = visuals.popup_shadow;

    ctx.set_visuals(visuals);
    p
}
