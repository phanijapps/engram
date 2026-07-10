# Plan: Export / import API (S5)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

S5 adds the **export** half (import already exists via the `MigrationService` handle). Four moves:

1. **Port.** `ExportImport` in `core/integration/src/export_import.rs` — `async fn export(&self, scope: &Scope) -> CoreResult<ImportData>`. Export-only; import stays on `MigrationService` (sync, independent — no impedance).
2. **SQLite impl.** `SqlExportImport` in `adapters/integration/src/export_import.rs`, composing the wired `Sql*` stores. Export reads via the existing **concrete store methods** (`list_entities`, `list_sources`, `list_documents`, `list_chunks`, `list_memories`) and serializes into `ImportData` (reusing the existing `*ImportRecord` types). v1 covers knowledge + memory families; hierarchy/belief deferred (no list method).
3. **Provider + capability.** `EngramProvider` gains an `export_import: Option<Arc<dyn ExportImport>>` handle; `bootstrap_provider` constructs `SqlExportImport`, runs the fixture, flips `export_import` to `Supported` on pass (only when the migration handle is also wired).
4. **Conformance fixture + gate.** Export from a seeded scope → `ImportData` → import via `MigrationService` into a fresh scope → parity.

## Constraints
- ADR-0022: port in `core/integration/src/export_import.rs` (engine-neutral); impl in `adapters/integration`.
- Reuse `ImportData` + `*ImportRecord` (round-trip parity); reuse `MigrationService` (don't reimplement import).
- Concrete store methods (not port traits) for export reads — the impl is engine-specific.

## Tasks

### T1: ExportImport port + gate
**Depends on:** none · **Mode:** goal-based
Port compiles, re-exported from `lib.rs`; add `$ROOT/core/integration/src/export_import.rs` to `.codex/hooks/check-engine-neutrality.sh` `GATED_PATHS`.

### T2: SqlExportImport impl
**Depends on:** T1 · **Mode:** TDD
Tests: export from a seeded scope → `ImportData` with correct record types + counts (knowledge + memory); round-trip via `MigrationService` (dry_run + apply into fresh scope) → records recoverable; parity. Impl: read from concrete store methods; serialize into `ImportData`.

### T3: Provider handle + capability flip + conformance fixture
**Depends on:** T2 · **Mode:** TDD
Handle + flip gated on fixture (export→import→parity) + migration handle wired. Self-sufficient (fixture in T3).

### T4: Gate verify + deferred-lanes backlog
**Depends on:** T3 · **Mode:** goal-based
Gate green; `docs/backlog.md` → `## export-import-hierarchy-belief` (hierarchy/belief export deferred — no concrete list method; contradictions not in ImportData).

## Changelog
- 2026-07-10: initial plan (S5 export-only port; import on MigrationService; concrete store reads; spec review resolved Blockers 1-4).
- 2026-07-10: implementation discovery — `MigrationService::apply_import` is validation-only (no store handles; cannot write records). AC4 reworded from "recovers records" to "validation-only pipeline parity" (dry_run row counts match exported counts + apply_import Ok). AC6 narrowed: concept_schemes/concepts moved from covered to deferred (no scope-wide list method). Concern 7: export is non-atomic best-effort read (documented in AC6). Concern 6 (source_for_chunk refactor) + Nit 8/9 deferred as non-blocking quality items.
