# Spec: SQLite Open Options

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0005 (storage adapter semantics)
- **Brief:** none
- **Contract:** none (adapter-local infrastructure, not a domain contract)
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Host adapters such as AgentZero's `zbot-engram-adapter` configure SQLite storage with explicit options (WAL mode, busy timeout, foreign keys, migrations, directory creation) through a common `SqliteOpenOptions` struct and `open_with_options` constructor. This replaces hardcoded PRAGMA values with explicit, host-controlled configuration while keeping existing `open_file` and `open_in_memory` constructors working as compatibility wrappers.

## Boundaries

### Always do

- Use `SqliteOpenOptions` for all new SQLite adapter initialization in host code.
- Preserve existing `open_file` and `open_in_memory` constructors as compatibility wrappers.
- Apply the same options pattern across all SQLite adapters (memory, knowledge, belief, hierarchy).
- Keep options type inside `engram-runtime` as adapter-local infrastructure.
- Make all SQLite configuration explicit through the options struct.

### Ask first

- Add SQLite-specific options that belong in host configuration instead of adapter code.
- Change the behavior of existing `open_file` or `open_in_memory` constructors.
- Add new SQLite pragmas beyond WAL mode, busy timeout, foreign keys, and cache size.

### Never do

- Leak SQLite implementation details into domain contracts or public API.
- Make options types part of the generated portable domain model.
- Assume host environments have write permission without `create_parent_dirs` flag.
- Skip migrations in production without explicit `run_migrations` flag.

## Testing Strategy

- **TDD:** options struct validation, path conversion, and PRAGMA execution order.
- **Goal-based:** adapter opens successfully with WAL mode and produces a valid database handle.
- **Integration:** existing tests pass after replacing constructors with `open_with_options`.

## Acceptance Criteria

- [x] `SqliteOpenOptions` struct exists in `engram-runtime` with fields for path, directory creation, journal mode, busy timeout, foreign keys, and migrations.
- [x] `SqlitePath` enum supports file paths and in-memory with consistent naming.
- [x] `SqliteJournalMode` enum supports WAL, DELETE, TRUNCATE, PERSIST, MEMORY, and OFF.
- [x] All four SQLite adapters (`SqlMemoryStore`, `SqlKnowledgeStore`, `SqlBeliefStore`, `SqlHierarchyStore`) expose `open_with_options`.
- [x] `open_with_options` creates parent directories when `create_parent_dirs` is true.
- [x] `open_with_options` applies `journal_mode`, `busy_timeout`, `foreign_keys`, and cache_size pragmas.
- [x] Existing `open_file` constructors continue to work and call `open_with_options` internally.
- [x] Existing `open_in_memory` constructors continue to work and call `open_with_options` internally.
- [x] Tutorial examples using `open_with_options` compile and run successfully.

## Assumptions

- Technical: Engram uses SQLite via rusqlite crate (source: Cargo.toml)
- Technical: Four SQLite adapters exist with hardcoded WAL/synchronous/busy_timeout pragmas (source: adapter schema.rs files)
- Technical: No existing `open_with_options` or `SqliteOpenOptions` pattern (source: grep verification returned empty)
- Technical: Tutorial already uses `open_with_options` but API doesn't exist yet (source: tutorial verification)
- Process: SQLite open options is P0 for AgentZero integration (source: user confirmation 2026-07-05)
- Product: AgentZero adapter needs explicit SQLite configuration (source: AgentZero integration spec)
- Product: Hosts need file-based SQLite with WAL mode for production (source: user requirement)
