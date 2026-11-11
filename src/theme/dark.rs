use egui::{Color32, Context, Rounding, Stroke, Visuals};

use super::ThemePalette;

pub fn dark_palette() -> ThemePalette {
    ThemePalette {
        activity_bar_bg: Color32::from_rgb(51, 51, 51),       // #333333
        activity_bar_fg_active: Color32::from_rgb(255, 255, 255),
        activity_bar_fg_inactive: Color32::from_rgb(150, 150, 150),
        sidebar_bg: Color32::from_rgb(37, 37, 38),            // #252526
        sidebar_header_bg: Color32::from_rgb(45, 45, 45),     // #2d2d2d
        editor_bg: Color32::from_rgb(30, 30, 30),             // #1e1e1e
        panel_bg: Color32::from_rgb(24, 24, 24),              // #181818
        tab_active_bg: Color32::from_rgb(30, 30, 30),         // #1e1e1e
        tab_inactive_bg: Color32::from_rgb(45, 45, 45),       // #2d2d2d
        status_bar_bg: Color32::from_rgb(0, 122, 204),        // #007acc (VS Code Blue)
        status_bar_fg: Color32::from_rgb(255, 255, 255),
        border_color: Color32::from_rgb(60, 60, 60),          // #3c3c3c
        accent_color: Color32::from_rgb(0, 122, 204),         // #007acc
        success_color: Color32::from_rgb(78, 201, 176),       // #4ec9b0 (MongoDB/VS Code Teal-Green)
        warning_color: Color32::from_rgb(204, 167, 0),
        error_color: Color32::from_rgb(244, 71, 71),
        muted_text: Color32::from_rgb(140, 140, 140),
    }
}

pub fn apply_dark_theme(ctx: &Context) -> ThemePalette {
    let p = dark_palette();
    let mut visuals = Visuals::dark();

    visuals.panel_fill = p.editor_bg;
    visuals.window_fill = p.sidebar_bg;
    visuals.extreme_bg_color = Color32::from_rgb(20, 20, 20);
    visuals.faint_bg_color = Color32::from_rgb(36, 36, 37);
    visuals.code_bg_color = Color32::from_rgb(25, 25, 26);

    visuals.selection.bg_fill = Color32::from_rgb(9, 71, 113); // #094771 VS Code selection
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(255, 255, 255));

    visuals.widgets.noninteractive.bg_fill = p.editor_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border_color);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(204, 204, 204));

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(50, 50, 52);
    visuals.widgets.inactive.rounding = Rounding::same(3.0);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(212, 212, 212));

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(62, 62, 66);
    visuals.widgets.hovered.rounding = Rounding::same(3.0);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent_color);

    visuals.widgets.active.bg_fill = p.accent_color;
    visuals.widgets.active.rounding = Rounding::same(3.0);

    visuals.window_rounding = Rounding::same(4.0);
    visuals.window_stroke = Stroke::new(1.0, p.border_color);

    ctx.set_visuals(visuals);
    p
}
