# Plan: SQLite knowledge-graph + taxonomy adapter (demo Slice 1)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing

## Approach

Bottom-up. (1) Add the `TaxonomyRepository` port to `engram-knowledge`. (2) Build
`engram-store-knowledge-sqlite` mirroring `engram-store-sql` (schema-as-JSON-blob
+ scope/lookup columns; `open_in_memory`/`open_in_file`; `scope_allows`),
implementing `KnowledgeRepository` + `KnowledgeGraphRepository` +
`TaxonomyRepository`. (3) Forbidden-import gate. (4) Binding + demo wiring
(NativeKnowledgeEngine, backend routes, UI) — later tasks.

## Constraints

RFC-0003 Slice 1; ADR-0003 (Rust library); ADR-0006 (SQLite adapter precedent);
ADR-0007 (binding surface); `memory-knowledge-boundaries` and
`sql-service-conformance` (focused modules, no god-module, no cross-adapter
coupling). Lighter adversarial review.

## Tasks

### T1: `TaxonomyRepository` port in `engram-knowledge`
**Depends on:** none
**Tests:** `engram-knowledge` compiles with the new trait; existing knowledge tests unaffected.
**Done when:** `cargo check -p engram-knowledge` passes.

### T2: `engram-store-knowledge-sqlite` crate
**Depends on:** T1
**Tests:** `tests/repository.rs` round-trips graph/entity/relationship/neighbors (scope+limit), chunk (inherited scope), taxonomy scheme/concept/relation/list (scope).
**Done when:** `cargo test -p engram-store-knowledge-sqlite` green (3 tests).

### T3: Forbidden-import gate
**Depends on:** T2
**Tests:** `.codex/hooks/check-knowledge-sqlite-isolation.sh` passes.
**Done when:** gate prints "ok" and would fail if a forbidden dep were added.

### T4: Workspace gates
**Depends on:** T3
**Tests:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `check-contracts.sh`, `check-docs.sh`.
**Done when:** all green, no drift.

### T5 (later): Binding `NativeKnowledgeEngine` + demo wiring
**Depends on:** T4
**Scope:** expose knowledge + taxonomy over `engram-node` (ADR-0007); add backend
`/knowledge/*` and `/taxonomy/*` routes; frontend knowledge-browse + taxonomy UI.
Split into its own tasks when started.

## Risks

- Scope semantics drift from the in-memory adapter → mitigated by mirroring its
  `scope_allows` and chunk→source chain exactly, with parallel tests.
- Future shared-connection work (OQ2) tempting cross-crate coupling → the gate
  makes it a build failure.

## Changelog

- 2026-06-30: initial plan (Slice 1 Rust foundation). Binding + UI deferred to T5.
