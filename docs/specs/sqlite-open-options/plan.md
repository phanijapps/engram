# Plan: SQLite Open Options

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

## Approach

Implement a common `SqliteOpenOptions` configuration pattern across all SQLite adapters so that AgentZero's `zbot-engram-adapter` (and other hosts) can explicitly control WAL mode, busy timeout, foreign keys, migrations, and directory creation. The strategy is:

1. **Add options types to `engram-runtime`** - create `SqliteOpenOptions`, `SqlitePath`, and `SqliteJournalMode` as adapter-local infrastructure
2. **Implement `open_with_options` in each adapter** - add the new constructor to memory, knowledge, belief, and hierarchy SQLite adapters
3. **Preserve existing constructors** - make `open_file` and `open_in_memory` call `open_with_options` internally
4. **Enable tutorial examples** - ensure the `open_with_options` pattern the tutorial uses actually compiles

This is a focused change that touches adapter initialization code only. No domain model changes, no breaking changes to existing behavior, and no changes to host application code.

## Constraints

- **RFC-0011** governs the port surface and genericity requirements
- **ADR-0005** (storage adapter semantics) requires adapters to remain storage-neutral at domain boundaries
- **AgentZero integration spec** requires explicit host-controlled SQLite configuration
- **Existing tests must pass** - all current adapter tests use `open_in_memory` and must continue working
- **Tutorial must become accurate** - the guide already uses `open_with_options` which doesn't exist yet

## Construction tests

- **Integration tests:** existing adapter tests pass with both old and new constructors
- **Manual verification:** tutorial example code compiles and runs successfully

## Design (LLD)

### Design decisions

- Keep `SqliteOpenOptions` in `engram-runtime` as adapter-local infrastructure, not domain types → satisfies genericity requirement
- Support both file and in-memory paths through `SqlitePath` enum → satisfies host configuration needs
- Apply WAL mode by default in file-based mode for production safety → satisfies performance/concurrency needs
- Make all constructors fallible with `CoreResult` → satisfies error handling requirements

### Data & schema

- `SqliteOpenOptions` struct: `path`, `create_parent_dirs`, `journal_mode`, `busy_timeout_ms`, `foreign_keys`, `run_migrations`
- `SqlitePath` enum: `File(PathBuf)`, `InMemory`
- `SqliteJournalMode` enum: `Wal`, `Delete`, `Truncate`, `Persist`, `Memory`, `Off`

### Interfaces & contracts

- `open_with_options(SqliteOpenOptions) -> CoreResult<Self>` added to all four SQLite adapters
- Existing `open_file(PathBuf)` and `open_in_memory()` preserved as compatibility wrappers

### Component / module decomposition

**New in `engram-runtime`:**
- `options.rs` module containing `SqliteOpenOptions`, `SqlitePath`, `SqliteJournalMode`

**Modified in each SQLite adapter:**
- Constructor method `open_with_options` using options callback pattern
- Internal `open_file` and `open_in_memory` refactored to call `open_with_options`

### Dependencies & integration

- Depends on `rusqlite` crate for SQLite pragmas
- Integrates with existing adapter schema initialization patterns
- No new external dependencies

## Tasks

### T1: Add options types to engram-runtime

**Depends on:** none

**Touches:** `core/runtime/src/lib.rs`, `core/runtime/src/options.rs` (new file)

**Tests:**
- `SqlitePath::File("/path/to/db")` constructs with path
- `SqlitePath::InMemory` constructs without path
- `SqliteJournalMode::Wal` serializes to "WAL"
- `SqliteOpenOptions` struct compiles with all fields

**Approach:**
- Create `core/runtime/src/options.rs` with the three types
- Add `pub use` re-exports to `core/runtime/src/lib.rs`
- Implement `fmt::Display` for `SqliteJournalMode` to get PRAGMA string values

**Done when:** options module compiles and all types are re-exported from `engram_runtime`

### T2: Implement open_with_options for SqlMemoryStore

**Depends on:** T1

**Touches:** `adapters/memory/sqlite/src/lib.rs`

**Tests:**
- Constructor with WAL mode creates database with `journal_mode=WAL`
- Constructor with `create_parent_dirs=true` creates parent directories
- Constructor with `foreign_keys=true` enables foreign keys
- Constructor with `run_migrations=false` skips migrations

**Approach:**
- Add `open_with_options` method that accepts `SqliteOpenOptions`
- Extract PRAGMA execution into a callback pattern
- Apply pragmas in order: journal_mode, synchronous, foreign_keys, busy_timeout, cache_size
- Preserve existing `schema.rs` initialization after pragmas

**Done when:** `open_with_options` works and existing tests still pass

### T3: Implement open_with_options for SqlKnowledgeStore

**Depends on:** T1, T2

**Touches:** `adapters/knowledge/sqlite/src/lib.rs`

**Tests:**
- Constructor with WAL mode creates knowledge database with correct pragmas
- Existing knowledge repository tests pass with new constructor

**Approach:**
- Copy the pattern from SqlMemoryStore implementation
- Adapt for knowledge store's specific schema initialization
- Preserve existing `open_file` and `open_in_memory` as wrappers

**Done when:** knowledge adapter uses same options pattern as memory adapter

### T4: Implement open_with_options for SqlBeliefStore

**Depends on:** T1, T2

**Touches:** `adapters/orchestration/belief-sqlite/src/lib.rs`

**Tests:**
- Constructor with WAL mode creates belief database with correct pragmas
- Existing belief repository tests pass with new constructor

**Approach:**
- Apply the same pattern as memory and knowledge adapters
- Ensure belief schema initialization remains compatible

**Done when:** belief adapter supports `open_with_options`

### T5: Implement open_with_options for SqlHierarchyStore

**Depends on:** T1, T2

**Touches:** `adapters/hierarchy/sqlite/src/lib.rs`

**Tests:**
- Constructor with WAL mode creates hierarchy database with correct pragmas
- Existing hierarchy repository tests pass with new constructor

**Approach:**
- Apply the same pattern as other adapters
- Ensure hierarchy schema initialization remains compatible

**Done when:** all four SQLite adapters support `open_with_options`

### T6: Update existing constructors to use open_with_options

**Depends on:** T2, T3, T4, T5

**Touches:** all four SQLite adapters

**Tests:**
- `open_file("/path")` creates file-based database with WAL mode
- `open_in_memory()` creates in-memory database with default pragmas
- All existing adapter tests continue to pass

**Approach:**
- Refactor `open_file` to call `open_with_options` with WAL mode enabled
- Refactor `open_in_memory` to call `open_with_options` with MEMORY journal mode
- Ensure backward compatibility - existing code using these constructors sees no behavior change

**Done when:** old constructors work and all tests pass

### T7: Verify tutorial examples work

**Depends on:** T6

**Touches:** `docs/guides/tutorials/integrating-engram-rust-library.md`

**Tests:**
- Tutorial example code compiles with `open_with_options` usage
- Tutorial example code runs and creates SQLite database with WAL mode

**Approach:**
- The tutorial already uses `open_with_options` - this was premature but now correct
- Verify the example matches the actual API surface
- Update if any mismatches found

**Done when:** tutorial example compiles and demonstrates WAL mode initialization

## Rollout

- **Delivery:** as a regular library release - no flags, big bang, but backward compatible
- **Infrastructure:** none - pure library change
- **External-system integration:** none - AgentZero adapter picks up the new API at next compile
- **Deployment sequencing:** library ships first, then AgentZero adapter can adopt `open_with_options` when convenient

## Risks

- **Tutorial drift risk:** The tutorial already uses `open_with_options` which doesn't exist yet → Mitigated: this implementation makes the tutorial accurate
- **Backward compatibility:** Existing code might depend on current constructor behavior → Mitigated: preserve existing constructors as wrappers
- **AgentZero adoption:** AgentZero adapter might need updates to use new options → Mitigated: new API is additive, old constructors still work

## Changelog

- 2026-07-05: initial plan focused on P0 (sqlite-open-options) for AgentZero integration ASAP
