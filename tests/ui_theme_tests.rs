use egui::Color32;
use mongo_studio::theme::{dark_palette, light_palette, ThemeMode};
use mongo_studio::ui::components::buttons::{
    BUTTON_FONT_SIZE, BUTTON_ROUNDING_RADIUS, PRIMARY_BUTTON_MIN_SIZE, SECONDARY_BUTTON_MIN_SIZE,
};
use mongo_studio::ui::layout::{
    ACTIVITY_BAR_WIDTH, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MIN_WINDOW_HEIGHT,
    MIN_WINDOW_WIDTH, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
};
use mongo_studio::ui::tab_bar::bson_type_color;

/// Computes the relative luminance of a color according to W3C WCAG 2.1 specs:
/// https://www.w3.org/TR/WCAG21/#dfn-relative-luminance
fn channel_srgb_to_linear(c: u8) -> f64 {
    let s = c as f64 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn relative_luminance(c: Color32) -> f64 {
    0.2126 * channel_srgb_to_linear(c.r())
        + 0.7152 * channel_srgb_to_linear(c.g())
        + 0.0722 * channel_srgb_to_linear(c.b())
}

/// Computes the WCAG contrast ratio between two colors:
/// ratio = (L1 + 0.05) / (L2 + 0.05) where L1 is the lighter color.
fn contrast_ratio(c1: Color32, c2: Color32) -> f64 {
    let l1 = relative_luminance(c1);
    let l2 = relative_luminance(c2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

#[test]
fn test_light_mode_wcag_text_contrast() {
    let p = light_palette();

    // In Light Mode, text_primary on all main light backgrounds must meet WCAG AAA (>= 7.0:1)
    let ratio_editor = contrast_ratio(p.text_primary, p.editor_bg);
    assert!(
        ratio_editor >= 7.0,
        "Light mode text_primary on editor_bg contrast {ratio_editor:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_panel = contrast_ratio(p.text_primary, p.panel_bg);
    assert!(
        ratio_panel >= 7.0,
        "Light mode text_primary on panel_bg contrast {ratio_panel:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_popup = contrast_ratio(p.text_primary, p.popup_bg);
    assert!(
        ratio_popup >= 7.0,
        "Light mode text_primary on popup_bg contrast {ratio_popup:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_sidebar = contrast_ratio(p.text_primary, p.sidebar_bg);
    assert!(
        ratio_sidebar >= 7.0,
        "Light mode text_primary on sidebar_bg contrast {ratio_sidebar:.2} must be >= 7.0:1 (AAA)"
    );

    // Muted text on editor_bg must meet at least WCAG AA (>= 4.5:1)
    let ratio_muted = contrast_ratio(p.muted_text, p.editor_bg);
    assert!(
        ratio_muted >= 4.5,
        "Light mode muted_text on editor_bg contrast {ratio_muted:.2} must be >= 4.5:1 (AA)"
    );

    // Status bar white text on blue status bar must meet WCAG AA (>= 4.5:1)
    let ratio_status = contrast_ratio(p.status_bar_fg, p.status_bar_bg);
    assert!(
        ratio_status >= 4.5,
        "Light mode status bar fg/bg contrast {ratio_status:.2} must be >= 4.5:1 (AA)"
    );
}

#[test]
fn test_dark_mode_wcag_text_contrast() {
    let p = dark_palette();

    // In Dark Mode, text_primary on all main dark backgrounds must meet WCAG AAA (>= 7.0:1)
    let ratio_editor = contrast_ratio(p.text_primary, p.editor_bg);
    assert!(
        ratio_editor >= 7.0,
        "Dark mode text_primary on editor_bg contrast {ratio_editor:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_panel = contrast_ratio(p.text_primary, p.panel_bg);
    assert!(
        ratio_panel >= 7.0,
        "Dark mode text_primary on panel_bg contrast {ratio_panel:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_popup = contrast_ratio(p.text_primary, p.popup_bg);
    assert!(
        ratio_popup >= 7.0,
        "Dark mode text_primary on popup_bg contrast {ratio_popup:.2} must be >= 7.0:1 (AAA)"
    );

    let ratio_sidebar = contrast_ratio(p.text_primary, p.sidebar_bg);
    assert!(
        ratio_sidebar >= 7.0,
        "Dark mode text_primary on sidebar_bg contrast {ratio_sidebar:.2} must be >= 7.0:1 (AAA)"
    );

    // Muted text on editor_bg in dark mode must meet at least WCAG AA (>= 4.5:1)
    let ratio_muted = contrast_ratio(p.muted_text, p.editor_bg);
    assert!(
        ratio_muted >= 4.5,
        "Dark mode muted_text on editor_bg contrast {ratio_muted:.2} must be >= 4.5:1 (AA)"
    );

    // Status bar white text on blue status bar
    let ratio_status = contrast_ratio(p.status_bar_fg, p.status_bar_bg);
    assert!(
        ratio_status >= 4.5,
        "Dark mode status bar fg/bg contrast {ratio_status:.2} must be >= 4.5:1 (AA)"
    );
}

#[test]
fn test_bson_type_badges_contrast_in_both_modes() {
    let light = light_palette();
    let dark = dark_palette();

    let sample_types = [
        "String",
        "Int32",
        "Int64",
        "Double",
        "ObjectId",
        "Boolean",
        "DateTime",
        "Document",
        "Array",
        "Null",
        "CustomType",
    ];

    for ty in sample_types {
        let color_light = bson_type_color(ty, &light);
        let color_dark = bson_type_color(ty, &dark);

        // In Light Mode, badges must be easily readable on white editor_bg (contrast >= 3.0:1)
        let ratio_light = contrast_ratio(color_light, light.editor_bg);
        assert!(
            ratio_light >= 3.0,
            "BSON type '{ty}' in Light Mode contrast {ratio_light:.2} against white must be >= 3.0:1"
        );

        // In Dark Mode, badges must be easily readable on dark editor_bg (contrast >= 3.5:1)
        let ratio_dark = contrast_ratio(color_dark, dark.editor_bg);
        assert!(
            ratio_dark >= 3.5,
            "BSON type '{ty}' in Dark Mode contrast {ratio_dark:.2} against dark bg must be >= 3.5:1"
        );
    }
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_component_size_and_theme_consistency() {
    // Buttons must have identical height (24.0px) for visual alignment
    assert_eq!(
        PRIMARY_BUTTON_MIN_SIZE.y, SECONDARY_BUTTON_MIN_SIZE.y,
        "Primary and secondary buttons must have identical height"
    );
    assert_eq!(
        PRIMARY_BUTTON_MIN_SIZE.y, 24.0,
        "Standard button height must be 24.0px"
    );

    // Button rounding radius must be consistent (3.0px) across components
    assert_eq!(
        BUTTON_ROUNDING_RADIUS, 3.0,
        "Standard button rounding must be 3.0px"
    );

    // Button font size must be consistent (12.0px)
    assert_eq!(
        BUTTON_FONT_SIZE, 12.0,
        "Standard button font size must be 12.0px"
    );

    // Minimum button widths must accommodate icons and labels without overflowing
    assert!(
        PRIMARY_BUTTON_MIN_SIZE.x >= 64.0,
        "Primary button minimum width must be >= 64.0px"
    );
    assert!(
        SECONDARY_BUTTON_MIN_SIZE.x >= 60.0,
        "Secondary button minimum width must be >= 60.0px"
    );
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_window_minimum_constraints_and_zero_overlap() {
    // Minimum window dimensions
    assert!(
        MIN_WINDOW_WIDTH >= 1024.0,
        "Minimum window width {MIN_WINDOW_WIDTH} must be >= 1024.0px to avoid UI clipping"
    );
    assert!(
        MIN_WINDOW_HEIGHT >= 640.0,
        "Minimum window height {MIN_WINDOW_HEIGHT} must be >= 640.0px for adequate vertical workspace"
    );
    assert!(
        DEFAULT_WINDOW_WIDTH >= MIN_WINDOW_WIDTH,
        "Default window width must be >= minimum window width"
    );
    assert!(
        DEFAULT_WINDOW_HEIGHT >= MIN_WINDOW_HEIGHT,
        "Default window height must be >= minimum window height"
    );

    // Sidebar and activity bar constraints
    assert!(
        SIDEBAR_MIN_WIDTH <= SIDEBAR_DEFAULT_WIDTH,
        "Sidebar min width must be <= default width"
    );
    assert!(
        SIDEBAR_DEFAULT_WIDTH <= SIDEBAR_MAX_WIDTH,
        "Sidebar default width must be <= max width"
    );

    // Even when the sidebar is expanded to its maximum allowable width (420px),
    // the remaining central workspace width at MIN_WINDOW_WIDTH must be at least 600px!
    let min_workspace_width = MIN_WINDOW_WIDTH - ACTIVITY_BAR_WIDTH - SIDEBAR_MAX_WIDTH;
    assert!(
        min_workspace_width >= 600.0,
        "Central workspace width ({min_workspace_width:.1}px) must be >= 600.0px at minimum window size even with max sidebar"
    );

    // At default sidebar width (250px), remaining workspace must be at least 750px
    let default_workspace_width = MIN_WINDOW_WIDTH - ACTIVITY_BAR_WIDTH - SIDEBAR_DEFAULT_WIDTH;
    assert!(
        default_workspace_width >= 750.0,
        "Central workspace width ({default_workspace_width:.1}px) must be >= 750.0px at minimum window size with default sidebar"
    );
}

#[test]
fn test_modal_dialog_bounds_fit_within_minimum_window() {
    // Define the sizes of all modals in Mongo Studio:
    // (name, width, height)
    let modal_sizes = [
        ("New/Edit Connection (SSH)", 480.0, 490.0),
        ("New/Edit Connection (No SSH)", 460.0, 345.0),
        ("Add Database", 410.0, 210.0),
        ("Create Collection", 400.0, 175.0),
        ("Confirm Drop Database", 410.0, 160.0),
        ("Confirm Drop Collection", 410.0, 160.0),
        ("Edit Document", 560.0, 460.0),
        ("Confirm Delete Document", 430.0, 170.0),
        ("Create Index", 460.0, 360.0),
        ("Confirm Drop Index", 420.0, 165.0),
        ("Save Query", 420.0, 220.0),
        ("Create/Duplicate User", 460.0, 380.0),
        ("Change Password", 410.0, 245.0),
        ("Confirm Delete User", 400.0, 155.0),
        ("About Modal", 390.0, 185.0),
        ("Command Palette", 520.0, 320.0),
    ];

    for (name, w, h) in modal_sizes {
        // Every modal must fit within MIN_WINDOW_WIDTH with at least 50px horizontal margin
        assert!(
            w <= MIN_WINDOW_WIDTH - 100.0,
            "Modal '{name}' width ({w:.1}px) must leave at least 100px total margin inside MIN_WINDOW_WIDTH ({MIN_WINDOW_WIDTH}px)"
        );

        // Every modal must fit within MIN_WINDOW_HEIGHT with at least 50px vertical margin
        assert!(
            h <= MIN_WINDOW_HEIGHT - 100.0,
            "Modal '{name}' height ({h:.1}px) must leave at least 100px total margin inside MIN_WINDOW_HEIGHT ({MIN_WINDOW_HEIGHT}px)"
        );
    }
}

#[test]
fn test_theme_mode_palette_symmetry_and_completeness() {
    let dark = dark_palette();
    let light = light_palette();

    // Verify all colors are opaque (a == 255)
    for (name, c) in [
        ("dark.activity_bar_bg", dark.activity_bar_bg),
        ("dark.sidebar_bg", dark.sidebar_bg),
        ("dark.editor_bg", dark.editor_bg),
        ("dark.panel_bg", dark.panel_bg),
        ("dark.popup_bg", dark.popup_bg),
        ("dark.status_bar_bg", dark.status_bar_bg),
        ("dark.text_primary", dark.text_primary),
        ("dark.muted_text", dark.muted_text),
        ("light.activity_bar_bg", light.activity_bar_bg),
        ("light.sidebar_bg", light.sidebar_bg),
        ("light.editor_bg", light.editor_bg),
        ("light.panel_bg", light.panel_bg),
        ("light.popup_bg", light.popup_bg),
        ("light.status_bar_bg", light.status_bar_bg),
        ("light.text_primary", light.text_primary),
        ("light.muted_text", light.muted_text),
    ] {
        assert_eq!(
            c.a(),
            255,
            "Palette color '{name}' must be fully opaque for consistent rendering"
        );
    }

    // Light editor must be brighter than dark editor
    assert!(
        relative_luminance(light.editor_bg) > relative_luminance(dark.editor_bg),
        "Light mode editor_bg must be brighter than Dark mode editor_bg"
    );

    // Dark text must be brighter than light text
    assert!(
        relative_luminance(dark.text_primary) > relative_luminance(light.text_primary),
        "Dark mode text_primary must be brighter than Light mode text_primary"
    );

    // Theme mode enum switching
    let mode = ThemeMode::Dark;
    let switched = match mode {
        ThemeMode::Dark => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
    };
    assert_eq!(switched, ThemeMode::Light);
}
