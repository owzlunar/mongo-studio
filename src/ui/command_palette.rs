use egui::{Context, Key, RichText, ScrollArea, TextEdit, Window};

use crate::core::commands::AppCommand;
use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::theme::ThemePalette;

pub fn render_command_palette(
    ctx: &Context,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    if !state.command_palette_open {
        return;
    }

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        state.command_palette_open = false;
        return;
    }

    Window::new("Command Palette")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size([520.0, 320.0])
        .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("›").strong().color(palette.accent_color).size(16.0));
                let resp = ui.add(
                    TextEdit::singleline(&mut state.command_palette_query)
                        .hint_text("Type the name of a command to run...")
                        .desired_width(450.0),
                );
                resp.request_focus();
                if ui.small_button("✕").clicked() {
                    state.command_palette_open = false;
                }
            });

            ui.separator();

            let q = state.command_palette_query.trim().to_lowercase();
            let items = AppCommand::all_items();

            ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                for item in items {
                    if !q.is_empty()
                        && !item.label.to_lowercase().contains(&q)
                        && !item.category.to_lowercase().contains(&q)
                    {
                        continue;
                    }

                    ui.horizontal(|ui| {
                        if ui.selectable_label(false, item.label).clicked() {
                            events.push(item.command.to_event());
                            state.command_palette_open = false;
                            state.command_palette_query.clear();
                        }

                        if !item.shortcut.is_empty() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(item.shortcut)
                                            .monospace()
                                            .size(11.0)
                                            .color(palette.muted_text),
                                    );
                                },
                            );
                        }
                    });
                }
            });
        });
}
