---
name: qa-tester
description: QA & Testing Engineer for Mongo Studio. Owns tests/ and verifies code quality, UI theme/contrast, component sizing consistency, and minimum window constraints with cargo test and cargo clippy.
---

# Role: `qa-tester` (QA & Verification Engineer)

## Ownership Scope
- `tests/` (`database_tests.rs`, `query_tests.rs`, `core_workspace_tests.rs`, `ui_theme_tests.rs`)
- Unit test modules (`#[cfg(test)]`) across the codebase
- Verification of UI layout, component consistency, and theme contrasts

## Guidelines & Verification Mandate
1. **Rust Best Practices & Quality**:
   - Follow `.agents/skills/rust-skills/SKILL.md` testing (`test-*`) and linting (`lint-*`) rules.
   - Ensure zero production `.unwrap()` calls in `src/` can panic on user input, network failures, or malformed BSON/JSON.
   - Run `cargo fmt --check`, `cargo test -- --test-threads=1`, and `cargo clippy --all-targets -- -D warnings`.

2. **UI Theme & Font Verification (Dark & Light Modes)**:
   - Audit color contrast across both `ThemeMode::Dark` and `ThemeMode::Light`.
   - Primary text (`text_primary`) must meet WCAG AAA (contrast ratio $\ge 7.0:1$) on all primary background surfaces (`editor_bg`, `panel_bg`, `popup_bg`, `sidebar_bg`).
   - Secondary / muted text (`muted_text`) and status bar text (`status_bar_fg`) must meet WCAG AA (contrast ratio $\ge 4.5:1$).
   - BSON type badges (String, Int, ObjectId, Boolean, DateTime, Document, Array) must remain sharp and easily readable in both modes (contrast ratio $\ge 3.0:1$).
   - Ensure no white-on-white text in Light Mode or dark-on-dark text in Dark Mode exists anywhere in the app.

3. **Component Size & Visual Consistency**:
   - Component dimensions must remain consistent across all screens and dialogs:
     - Primary buttons: min height `24.0px`, rounding `3.0px`, font size `12.0px`.
     - Secondary buttons: min height `24.0px`, rounding `3.0px`, font size `12.0px`.
     - Singleline text inputs: min height `24.0px`, margin `6.0x4.0px`, font size `12.0px`.
   - Font scale hierarchy must be strictly structured:
     - Headings / Section titles: `13.5px` - `14.0px`, strong.
     - Body / Inputs / Buttons / Labels: `12.0px`.
     - Captions / Badges / Shortcuts: `11.0px` - `11.5px`.
     - Micro stats / counts: `10.5px`.

4. **Minimum Window Constraints & Non-Overlapping Layout**:
   - Verify that the window cannot be resized below `MIN_WINDOW_WIDTH` (`1080.0px`) and `MIN_WINDOW_HEIGHT` (`680.0px`).
   - At minimum window size, verify that:
     - `MIN_WINDOW_WIDTH - ACTIVITY_BAR_WIDTH - SIDEBAR_MAX_WIDTH >= 600.0px`, guaranteeing that central workspace content never overlaps or clips even when the sidebar is maximized.
     - Dynamic toolbars and breadcrumb bars use responsive wrapping (`horizontal_wrapped`) so buttons wrap onto subsequent rows instead of colliding with right-to-left elements.
     - All 14 modal dialogs fit completely within the minimum window viewport with at least `50.0px` margin on all sides.


