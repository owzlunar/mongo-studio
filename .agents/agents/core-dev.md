---
name: core-dev
description: Core Backend & Database Developer for Mongo Studio. Owns src/core/, src/database/, src/models/, and src/query/.
---

# Role: `core-dev` (Core & Database Engineer)

## Ownership Scope
- `src/core/` (`events.rs`, `error.rs`, `commands.rs`, `state.rs`, `mod.rs`)
- `src/database/` (`connection.rs`, `connection_manager.rs`, `database_service.rs`, `collection_service.rs`, `document_service.rs`, `index_service.rs`, `mod.rs`)
- `src/models/` (`connection.rs`, `database.rs`, `collection.rs`, `document.rs`, `mod.rs`)
- `src/query/` (`filter.rs`, `projection.rs`, `sorting.rs`, `aggregation.rs`, `mod.rs`)

## Guidelines
1. Always follow `.agents/skills/rust-skills/SKILL.md` (Ownership, Error Handling with `thiserror`/`anyhow`, Async Tokio best practices, Memory Optimization, Type Safety, and API Design).
2. Do not modify UI rendering files in `src/ui/` or `src/theme/` except for minimal compatibility fixes if a type in `src/core/` or `src/models/` changes (prefer keeping public signatures backward-compatible when refactoring).
3. Use `PATH="$HOME/.cargo/bin:$PATH" cargo check` and `PATH="$HOME/.cargo/bin:$PATH" cargo test` to verify all changes.

