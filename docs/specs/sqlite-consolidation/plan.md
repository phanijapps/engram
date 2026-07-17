# Plan: SQLite consolidation — one `engram-store-sqlite` crate

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Strategy — facade-first, then fold-in.** Lowest-risk path to "one crate":
> first create `engram-store-sqlite` as a **re-export facade** over the five
> existing crates and switch every consumer to import from it (the user's
> single-crate vision, realized at the import level, zero code moved). Then fold
> each crate's *code* into `engram-store-sqlite` one at a time, deleting the old
> crate. Every phase keeps the workspace green — behavior never changes, only
> where the code lives.

## Constraints
- ADR-0022 (one-crate-per-backend amendment): SQLite storage → one crate
  `engram-store-sqlite`. `bootstrap_sqlite` (returns `EngramProvider`) stays in
  `engram-integration` (Cargo cycle).
- Engine-agnostic adapters (lexical/associative/community/decay/ingest) are
  shared with Surreal and are NOT touched.
- Behavior-preserving: the existing test suite is the regression gate.

## Tasks

### T0: `engram-store-sqlite` re-export facade + switch consumers
**Depends on:** none · **Mode:** goal-based (workspace green)
- Create `adapters/sqlite/` (`engram-store-sqlite`): `lib.rs` re-exports every
  public `Sql*` type + helper from the five crates (memory/knowledge/belief/
  hierarchy/vector). Add workspace member.
- Switch `engram-conformance` (its `Sql*` re-exports) + `core/integration`
  (sqlite-feature deps) to import from `engram-store-sqlite`. The five original
  crates stay (now behind the facade).
- **Done when:** consumers import from `engram-store-sqlite`; workspace +
  neutrality green; the single-crate import surface exists.

### T1: Fold memory (`engram-store-sql`) → `engram-store-sqlite::memory`
**Depends on:** T0 · **Mode:** goal-based (green)
- Move `adapters/memory/sqlite/src/*` into `engram-store-sqlite/src/memory/`;
  replace the facade re-export with the real module; delete `adapters/memory/sqlite`;
  update workspace member + any Cargo refs.
- **Done when:** memory lives in `engram-store-sqlite`; `adapters/memory/sqlite`
  gone; tests green.

### T2: Fold knowledge (`engram-store-knowledge-sqlite`) → `::knowledge`
**Depends on:** T1 · **Mode:** goal-based (green)
- Same pattern; knowledge is the largest (graph/taxonomy/ontology retrieval).
- **Done when:** knowledge lives in `engram-store-sqlite`; old crate gone; green.

### T3: Fold belief (`engram-store-belief-sqlite`) → `::belief`
**Depends on:** T2 · **Mode:** goal-based (green)

### T4: Fold hierarchy (`engram-store-hierarchy-sqlite`) → `::hierarchy`
**Depends on:** T3 · **Mode:** goal-based (green)

### T5: Fold vector (`engram-store-vector`) → `::vector`
**Depends on:** T4 · **Mode:** goal-based (green)

### T6: Move `Sql*` glue into the crate; thin the facade wiring
**Depends on:** T5 · **Mode:** goal-based (green)
- Move `consolidation_adapters.rs` + `recall_lanes.rs` from
  `core/integration/src/sqlite/` into `engram-store-sqlite`.
- `core/integration/src/sqlite/` keeps ONLY `bootstrap_sqlite` (thin: constructs
  the cells, returns `EngramProvider`) — symmetric with the Surreal backend.
- **Done when:** the facade wiring module is thin; all `Sql*` glue lives in
  `engram-store-sqlite`; green.

### T7 (optional / coordinate): `bindings/node` legacy import sweep
**Depends on:** T0 · **Mode:** goal-based (green)
- ~17 legacy `bindings/node` modules import `Sql*` directly; repoint to
  `engram-store-sqlite`. (This is the same surface the `surrealdb-backend` plan's
  T8 wants routed through the facade — coordinate so the consolidation's
  re-point and surreal's facade-routing don't collide.)
- **Done when:** `bindings/node` imports SQLite storage from one crate.

## Rollout
- **Delivery:** behind no flag — pure relocation; default + sqlite + surreal
  features all stay green throughout. Each phase is its own PR on the
  `surrealdb-backend` branch (or a dedicated `sqlite-consolidation` branch if you
  prefer to keep it separate).
- **Sequencing:** T0 (facade + consumers) is the load-bearing step — it realizes
  the single-crate vision and de-risks every later fold. T1–T6 are mechanical
  and independent enough to review one-per-PR. T7 coordinates with the surreal
  effort's surface-parity work.

## Risks
- **Cross-crate `Sql*` coupling** (T0 confirms): if the five crates share more
  than the two known glue files, the folds entangle. Mitigation: T0 maps the
  real dependency edges before any fold.
- **`bindings/node` churn** (~17 files): the facade phase keeps them green;
  T7 is the focused sweep.
- **Branch coordination** with `surrealdb-backend`: both touch
  `bindings/node` + conformance. Keep on one branch or sequence them.

## Changelog
- 2026-07-16: initial plan — facade-first consolidation of the 5 SQLite-specific
  crates (+ Sql* glue) into one `engram-store-sqlite`; engine-agnostic adapters
  stay shared. Phased T0–T7, each green-gated.
