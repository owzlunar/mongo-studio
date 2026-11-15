use std::path::Path;
use std::sync::Arc;

use egui::IconData;

/// Embedded SVG vector asset of the Mongo Studio application logo.
pub const LOGO_SVG: &[u8] = include_bytes!("../../assets/logo.svg");

/// Loads the application icon at 256x256 resolution as [`egui::IconData`]
/// suitable for [`egui::ViewportBuilder::with_icon`].
///
/// This runtime icon is automatically picked up by `eframe`:
/// - On macOS: sets the application Dock icon via Cocoa's `[NSApp setApplicationIconImage:]`
/// - On Windows: sets `WM_SETICON` for taskbar and titlebar
/// - On Linux: sets the X11/Wayland window & taskbar icon
#[must_use]
pub fn load_app_icon_data() -> Option<IconData> {
    render_svg_icon(256)
}

/// Loads the application icon wrapped in an `Arc`.
#[must_use]
pub fn load_app_icon_arc() -> Option<Arc<IconData>> {
    load_app_icon_data().map(Arc::new)
}

/// Renders the embedded SVG logo at the requested square `size` (in pixels)
/// and returns an [`IconData`] structure containing raw RGBA8 pixels.
#[must_use]
pub fn render_svg_icon(size: u32) -> Option<IconData> {
    let color_image = egui_extras::image::load_svg_bytes_with_size(
        LOGO_SVG,
        Some(egui::SizeHint::Size(size, size)),
    )
    .ok()?;

    Some(IconData {
        rgba: color_image.as_raw().to_vec(),
        width: color_image.width() as u32,
        height: color_image.height() as u32,
    })
}

/// Encodes raw RGBA8 pixels into PNG bytes.
pub fn encode_png_bytes(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut png_bytes = Vec::new();
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "Failed to construct RgbaImage from raw pixels".to_string())?;

    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageOutputFormat::Png,
    )
    .map_err(|err| format!("PNG encoding failed: {err}"))?;

    Ok(png_bytes)
}

/// Creates a multi-resolution Windows `.ico` byte buffer embedding standard PNG streams.
#[must_use]
pub fn create_ico_from_pngs(png_images: &[(u32, u32, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    // ICONDIR header
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved (must be 0)
    out.extend_from_slice(&1u16.to_le_bytes()); // Image type: 1 = ICO
    out.extend_from_slice(&(png_images.len() as u16).to_le_bytes()); // Number of images

    let mut offset = 6 + (png_images.len() * 16) as u32;
    for &(w, h, data) in png_images {
        let width_byte = if w >= 256 { 0u8 } else { w as u8 };
        let height_byte = if h >= 256 { 0u8 } else { h as u8 };
        out.push(width_byte);
        out.push(height_byte);
        out.push(0); // Palette color count (0 = no palette / 256+ colors)
        out.push(0); // Reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // Color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // Bits per pixel (32 = RGBA)
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // Data length
        out.extend_from_slice(&offset.to_le_bytes()); // Data offset
        offset += data.len() as u32;
    }

    for &(_, _, data) in png_images {
        out.extend_from_slice(data);
    }
    out
}

/// Generates all OS-specific icon assets (PNGs, Windows `.ico`, and Linux `.desktop`)
/// into the specified target directory.
pub fn generate_icon_assets(target_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target_dir).map_err(|e| {
        format!(
            "Failed to create icon directory {}: {e}",
            target_dir.display()
        )
    })?;

    let sizes = [16u32, 32, 48, 64, 128, 256, 512];
    let mut png_layers: Vec<(u32, u32, Vec<u8>)> = Vec::new();

    for &sz in &sizes {
        let icon_data =
            render_svg_icon(sz).ok_or_else(|| format!("Failed to render SVG at size {sz}x{sz}"))?;
        let png = encode_png_bytes(sz, sz, &icon_data.rgba)?;

        let filename = format!("icon_{sz}x{sz}.png");
        std::fs::write(target_dir.join(&filename), &png)
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;

        if sz == 512 {
            std::fs::write(target_dir.join("app_icon.png"), &png)
                .map_err(|e| format!("Failed to write app_icon.png: {e}"))?;
        }

        png_layers.push((sz, sz, png));
    }

    // Windows .ico
    let png_refs: Vec<(u32, u32, &[u8])> = png_layers
        .iter()
        .map(|(w, h, data)| (*w, *h, data.as_slice()))
        .collect();
    let ico_bytes = create_ico_from_pngs(&png_refs);
    std::fs::write(target_dir.join("app_icon.ico"), &ico_bytes)
        .map_err(|e| format!("Failed to write app_icon.ico: {e}"))?;

    // macOS .iconset and .icns
    let iconset_dir = target_dir.join("app_icon.iconset");
    std::fs::create_dir_all(&iconset_dir)
        .map_err(|e| format!("Failed to create iconset dir: {e}"))?;

    let iconset_specs = [
        ("icon_16x16.png", 16u32),
        ("icon_16x16@2x.png", 32u32),
        ("icon_32x32.png", 32u32),
        ("icon_32x32@2x.png", 64u32),
        ("icon_128x128.png", 128u32),
        ("icon_128x128@2x.png", 256u32),
        ("icon_256x256.png", 256u32),
        ("icon_256x256@2x.png", 512u32),
        ("icon_512x512.png", 512u32),
        ("icon_512x512@2x.png", 1024u32),
    ];

    for (name, sz) in iconset_specs {
        let icon_data = render_svg_icon(sz)
            .ok_or_else(|| format!("Failed to render SVG at size {sz}x{sz} for iconset"))?;
        let png = encode_png_bytes(sz, sz, &icon_data.rgba)?;
        std::fs::write(iconset_dir.join(name), &png)
            .map_err(|e| format!("Failed to write {name}: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        let icns_path = target_dir.join("app_icon.icns");
        let _ = std::process::Command::new("/usr/bin/iconutil")
            .arg("-c")
            .arg("icns")
            .arg(&iconset_dir)
            .arg("-o")
            .arg(&icns_path)
            .status();
    }

    // Linux .desktop specification
    let desktop_entry = r#"[Desktop Entry]
Name=Mongo Studio
Comment=Modern MongoDB Admin and Query Tool
Exec=mongo-studio
Icon=mongo-studio
Terminal=false
Type=Application
Categories=Development;Database;
StartupNotify=true
"#;
    std::fs::write(target_dir.join("mongo-studio.desktop"), desktop_entry)
        .map_err(|e| format!("Failed to write mongo-studio.desktop: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_app_icon_data_renders_valid_rgba() {
        let icon = load_app_icon_data().expect("should render embedded SVG icon");
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn test_encode_png_and_create_ico() {
        let icon = render_svg_icon(32).expect("render 32x32");
        let png = encode_png_bytes(32, 32, &icon.rgba).expect("encode png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        let ico = create_ico_from_pngs(&[(32, 32, &png)]);
        assert_eq!(ico.len(), 6 + 16 + png.len());
        assert_eq!(&ico[0..4], &[0, 0, 1, 0]); // Reserved 0, Type 1
    }

    #[test]
    fn test_generate_all_os_icons() {
        let out_dir = Path::new("assets/icons");
        generate_icon_assets(out_dir).expect("generate icon assets");
        assert!(out_dir.join("app_icon.png").exists());
        assert!(out_dir.join("app_icon.ico").exists());
        assert!(out_dir.join("mongo-studio.desktop").exists());
        assert!(out_dir.join("icon_256x256.png").exists());
        #[cfg(target_os = "macos")]
        assert!(out_dir.join("app_icon.icns").exists());
    }
}
