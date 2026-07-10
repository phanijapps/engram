# Spec: Export / import API (S5)

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality), [`rust-crate-integration`](../rust-crate-integration/spec.md) (the Implementing facade S5 extends), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — owns the `export_import` capability key S5 flips)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S5, capabilities #13/#18)
- **Contract:** none — a Rust port trait (`ExportImport`) + provider handle, reusing existing migration types.
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

A host **exports** the semantic state of a scope — knowledge sources, documents, chunks, entities, relationships, concept schemes, concepts, and memory records — into one `ImportData` payload through a backend-neutral `ExportImport` provider handle. Export reads from the existing concrete store methods (`list_entities`, `list_sources`, `list_documents`, `list_chunks`, `list_memories`, etc.) in the adapter layer — no new storage, no schema change. Import remains on the existing `MigrationService` handle (`dry_run_import` + `apply_import`); the `export_import` capability flips to `Supported` when **both** the export handle and the migration handle are wired and the round-trip conformance fixture passes.

The exported `ImportData` reuses the existing import record types, so export → `dry_run_import` → `apply_import` round-trips for single-backend scope-to-scope movement. v1 covers the families whose concrete stores expose listing methods (knowledge + taxonomy + memory + belief + hierarchy); the belief + hierarchy stores are attached optionally to the export handle (an unwired family exports empty). Vectors remain deferred (no concrete scope-wide list method). Contradictions are not in the round-trip (ImportData has no contradictions vec).

## Boundaries

### Always do
- Export by reading from the existing **concrete store methods** on the wired `Sql*` stores in the adapter layer (`list_entities`, `list_sources`, `list_documents`, `list_chunks`, `list_memories` — the impl is engine-specific and may call them directly).
- Reuse `ImportData` + the existing `*ImportRecord` types for the exported payload (round-trip parity with `MigrationService::dry_run_import`/`apply_import`).
- Import remains on the existing `MigrationService` handle (sync trait) — do **not** reimplement import or wrap it in a new trait.
- Keep the `ExportImport` port in `core/integration` (engine-neutral); put the SQLite impl in `adapters/integration`.
- Flip `export_import` to `Supported` only when the conformance fixture passes AND the migration handle is wired; attach the export handle only then.

### Ask first
- Add listing methods to MemoryService/BeliefRepository/HierarchyRepository ports (so export can cover those families via the port abstraction, not just the concrete store).
- Add parity validation (record-count + content-hash comparison post-import) as a first-class operation.
- Add a streaming/chunked export for very large scopes.

### Never do
- Name an engine type or hold SQL in the `ExportImport` port or `core/integration`. *(structural, ADR-0022)*
- Reimplement import (it stays on `MigrationService`). *(structural)*
- Invent a new export format — reuse `ImportData`.
- Reimplement SQL queries for export — use the existing concrete store methods.

## Testing Strategy
- **ExportImport port — goal-based.** The trait compiles, is re-exported, the neutrality gate covers `export_import.rs`.
- **SqlExportImport impl — TDD.** Export from a seeded scope → `ImportData` with the right record types + counts for the knowledge + memory families; round-trip (export → `MigrationService::dry_run_import` → `apply_import` into a fresh scope) → records recoverable with matching counts.
- **Provider handle + capability flip — TDD.** `EngramProvider.export_import()` handle present when fixture passes; `export_import` capability `Supported` only when both export + migration handles wired + fixture passes.
- **Conformance fixture — goal-based.** Export from scope A → `ImportData` → import (via `MigrationService`) into scope B → recover records from B → parity.
- **Engine neutrality — goal-based check.** Gate covers `core/integration/src/export_import.rs`.
- **No regression — goal-based check.**

## Acceptance Criteria
- [x] The `ExportImport` port in `core/integration/src/export_import.rs` exposes `async fn export(&self, scope: &Scope) -> CoreResult<ImportData>`, reusing the existing `ImportData` type. Import is NOT on this trait — it stays on the existing `MigrationService` handle (`dry_run_import` + `apply_import`, sync).
- [x] A SQLite `ExportImport` implementation reads from the existing concrete store methods (`list_entities`, `list_sources`, `list_documents`, `list_chunks`, `list_memories`) on the wired `Sql*` stores in the adapter layer — no new SQL, no schema change. v1 covers knowledge (sources/documents/chunks/entities/relationships) + memory.
- [x] `EngramProvider` exposes an `export_import()` handle; the `export_import` capability flips to `Supported` only when the conformance fixture passes AND the migration handle is wired; the export handle is attached only then.
- [x] A conformance fixture exports from a seeded scope A → `ImportData` → `MigrationService::dry_run_import` → `apply_import` returns `Ok` with the validation row counts matching the exported counts (parity through the validation-only pipeline; v1 `apply_import` does not write records).
- [x] `.codex/hooks/check-engine-neutrality.sh` covers `core/integration/src/export_import.rs` (added to `GATED_PATHS`); the gated files in `core/integration/src/` are engine-symbol-free.
- [x] v1 export covers knowledge (sources/documents/chunks/entities/relationships), taxonomy (concept schemes + their concepts), memory, belief, and hierarchy — the families whose concrete stores expose scope-wide listing methods. Beliefs are read via `SqlBeliefStore::list_beliefs`; hierarchy nodes via `SqlHierarchyStore::list_nodes`; concept schemes via `SqlKnowledgeStore::list_concept_schemes` (concepts listed per scheme via the existing `TaxonomyRepository::list_concepts`). The belief + hierarchy stores are attached optionally (`SqlExportImport::with_belief` / `with_hierarchy`); an unwired family exports empty rather than erroring. Vectors remain deferred (no concrete scope-wide list method). Contradictions are not in the round-trip (`ImportData` has no contradictions vec). Export is a non-atomic best-effort read (concurrent writes between list calls can yield an inconsistent snapshot).
- [x] SQLite behavior for existing operations is unchanged; existing workspace tests green.

## Assumptions
- Technical: `ImportData` + `*ImportRecord` structs + `MigrationService` (`dry_run_import` sync → `ValidationReport`; `apply_import` sync → `()`) exist and the `MigrationService` is already an `EngramProvider` handle (source: `core/integration/src/migration.rs`).
- Technical: no export function exists today; `export_import` capability key is `Unsupported { FeatureDisabled }` with no handle (source: grep; `core/integration/src/provider.rs`).
- Technical: the concrete stores expose scope-wide listing for knowledge (`list_entities`/`list_sources`/`list_documents`/`list_chunks`/`list_concept_schemes` at `adapters/knowledge/sqlite/src/service.rs`), memory (`list_memories` at `adapters/memory/sqlite/src/service.rs`), belief (`SqlBeliefStore::list_beliefs` at `adapters/orchestration/belief-sqlite/src/service.rs`), and hierarchy (`SqlHierarchyStore::list_nodes` at `adapters/hierarchy/sqlite/src/service.rs`); concepts are listed per scheme via the existing `TaxonomyRepository::list_concepts`. Vectors have no scope-wide list method today (source: grep).
- Technical: `MigrationService` methods are sync; `ExportImport::export` is async — the traits are independent (export doesn't delegate to migration, so no sync/async impedance).
- Design: `ExportImport` is export-only; import stays on `MigrationService`. The `export_import` capability = both wired. Exported `ImportData` reuses existing types. Port in `core/integration`, impl in `adapters/integration`. (source: user confirmation 2026-07-10 + spec review 2026-07-10)
- Process: SQLite only; the port stays engine-neutral (ADR-0022); additive only; reuse existing concrete store reads; the brief's "extends migration module" is superseded by the separate-port decision (user-fixed).
