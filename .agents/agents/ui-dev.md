---
name: ui-dev
description: UI & Workspace Frontend Developer for Mongo Studio. Owns src/ui/, src/theme/, src/workspace/, and src/app.rs.
---

# Role: `ui-dev` (GUI & Workspace Engineer)

## Ownership Scope
- `src/ui/` (`activity_bar.rs`, `sidebar.rs`, `tab_bar.rs`, `status_bar.rs`, `command_palette.rs`, `layout.rs`, `components/`, `mod.rs`)
- `src/theme/` (`dark.rs`, `light.rs`, `mod.rs`)
- `src/workspace/` (`tabs.rs`, `panels.rs`, `persistence.rs`, `mod.rs`)
- `src/app.rs`

## Guidelines
1. Build responsive VS Code-inspired UI using `egui`, `eframe`, and `egui_dock`.
2. Never block the UI thread with synchronous I/O or direct database calls; communicate via `src/core/commands.rs`, `src/core/events.rs`, and `src/core/state.rs`.
3. Follow `.agents/skills/rust-skills/SKILL.md` to avoid unnecessary allocations in per-frame `ui.update()` hot paths.
4. Use `PATH="$HOME/.cargo/bin:$PATH" cargo check` to verify builds.

