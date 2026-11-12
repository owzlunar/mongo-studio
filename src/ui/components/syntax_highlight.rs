use std::sync::{Arc, OnceLock};

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Galley, Ui};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

fn append_plain_job(job: &mut LayoutJob, text: &str, font_id: FontId, color: Color32) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id,
            color,
            ..Default::default()
        },
    );
}

/// Highlights JSON / Extended JSON text into an [`egui::text::LayoutJob`] using cached `syntect`
/// syntax and theme sets, with transparent backgrounds so `TextEdit` looks native in Dark & Light
/// modes.
#[must_use]
pub fn highlight_json_job(ui: &Ui, text: &str, font_id: FontId, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    if text.is_empty() {
        return job;
    }

    let ps = syntax_set();
    let ts = theme_set();

    let preferred_theme = if ui.visuals().dark_mode {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    };

    let Some(theme) = ts
        .themes
        .get(preferred_theme)
        .or_else(|| ts.themes.values().next())
    else {
        append_plain_job(&mut job, text, font_id, ui.visuals().text_color());
        return job;
    };

    let syntax = ps
        .find_syntax_by_extension("json")
        .or_else(|| ps.find_syntax_by_name("JSON"))
        .unwrap_or_else(|| ps.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    let fallback_color = ui.visuals().text_color();

    for line in LinesWithEndings::from(text) {
        match highlighter.highlight_line(line, ps) {
            Ok(ranges) => {
                for (style, segment) in ranges {
                    let fg = style.foreground;
                    let color = Color32::from_rgba_unmultiplied(fg.r, fg.g, fg.b, fg.a);
                    job.append(
                        segment,
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color,
                            ..Default::default()
                        },
                    );
                }
            }
            Err(_) => {
                append_plain_job(&mut job, line, font_id.clone(), fallback_color);
            }
        }
    }

    job
}

/// Returns a reusable `TextEdit::layouter` closure for monospace JSON syntax highlighting.
pub fn json_layouter(font_size: f32) -> impl FnMut(&Ui, &str, f32) -> Arc<Galley> {
    move |ui, text, wrap_width| {
        let job = highlight_json_job(ui, text, FontId::monospace(font_size), wrap_width);
        ui.fonts(|fonts| fonts.layout_job(job))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntect_json_syntax_and_themes_initialize_cleanly() {
        let ps = syntax_set();
        let ts = theme_set();
        assert!(ps.find_syntax_by_extension("json").is_some());
        assert!(!ts.themes.is_empty());
    }
}
