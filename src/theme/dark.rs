use egui::{epaint::Shadow, Color32, Context, Rounding, Stroke, Vec2, Visuals};

use super::ThemePalette;

#[must_use]
pub const fn dark_palette() -> ThemePalette {
    ThemePalette {
        activity_bar_bg: Color32::from_rgb(24, 24, 24), // #181818 VS Code Activity Bar
        activity_bar_fg_active: Color32::from_rgb(255, 255, 255),
        activity_bar_fg_inactive: Color32::from_rgb(140, 140, 140),
        sidebar_bg: Color32::from_rgb(37, 37, 38), // #252526 VS Code Sidebar
        sidebar_header_bg: Color32::from_rgb(37, 37, 38), // #252526
        editor_bg: Color32::from_rgb(30, 30, 30),  // #1e1e1e VS Code Editor
        panel_bg: Color32::from_rgb(24, 24, 24),   // #181818 VS Code Panel
        popup_bg: Color32::from_rgb(37, 37, 38),   // #252526 elevated surface
        popup_border: Color32::from_rgb(69, 69, 69), // #454545 crisp popup border
        tab_active_bg: Color32::from_rgb(30, 30, 30), // #1e1e1e matches editor
        tab_inactive_bg: Color32::from_rgb(45, 45, 45), // #2d2d2d
        status_bar_bg: Color32::from_rgb(0, 122, 204), // #007acc VS Code Blue
        status_bar_fg: Color32::from_rgb(255, 255, 255),
        border_color: Color32::from_rgb(60, 60, 60), // #3c3c3c crisp VS Code border
        accent_color: Color32::from_rgb(0, 122, 204), // #007acc
        success_color: Color32::from_rgb(78, 201, 176), // #4ec9b0 MongoDB/VS Code Teal-Green
        warning_color: Color32::from_rgb(204, 167, 0), // #cca700
        error_color: Color32::from_rgb(244, 71, 71), // #f44747
        text_primary: Color32::from_rgb(230, 230, 230),
        muted_text: Color32::from_rgb(140, 140, 140),
    }
}

#[must_use]
pub fn apply_dark_theme(ctx: &Context) -> ThemePalette {
    let p = dark_palette();
    let mut visuals = Visuals::dark();

    visuals.panel_fill = p.editor_bg;
    visuals.window_fill = p.popup_bg;
    visuals.extreme_bg_color = Color32::from_rgb(26, 26, 26);
    visuals.faint_bg_color = Color32::from_rgb(34, 34, 34);
    visuals.code_bg_color = Color32::from_rgb(24, 24, 24);

    visuals.selection.bg_fill = Color32::from_rgb(9, 71, 113); // #094771 VS Code selection
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(255, 255, 255));

    visuals.widgets.noninteractive.bg_fill = p.popup_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border_color);
    visuals.widgets.noninteractive.fg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(212, 212, 212));

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(50, 50, 54);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(64, 64, 70));
    visuals.widgets.inactive.rounding = Rounding::same(3.0);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 220, 220));

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(9, 71, 113); // Clear VS Code hover highlight
    visuals.widgets.hovered.rounding = Rounding::same(3.0);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

    visuals.widgets.active.bg_fill = p.accent_color;
    visuals.widgets.active.rounding = Rounding::same(3.0);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

    visuals.widgets.open.bg_fill = Color32::from_rgb(9, 71, 113);
    visuals.widgets.open.rounding = Rounding::same(3.0);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.accent_color);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

    visuals.window_rounding = Rounding::same(5.0);
    visuals.menu_rounding = Rounding::same(4.0);
    visuals.window_stroke = Stroke::new(1.0_f32, p.popup_border);
    visuals.popup_shadow = Shadow {
        offset: Vec2::new(0.0, 4.0),
        blur: 16.0,
        spread: 2.0,
        color: Color32::from_black_alpha(160),
    };
    visuals.window_shadow = visuals.popup_shadow;

    ctx.set_visuals(visuals);
    p
}
