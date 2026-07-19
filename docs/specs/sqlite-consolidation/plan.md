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

### T6: Move ALL `Sql*` glue + `SqliteOpenOptions` into the crate
**Depends on:** T5 · **Mode:** goal-based (green)
- Move every SQLite-specific module out of `core/integration/src/sqlite/` into
  `engram-store-sqlite`: `batch`, `export_import`, `observability`, `provenance`,
  `recall`, `recall_lanes`, `consolidation_adapters`, `migration_service`,
  `conformance`. `core/integration/src/sqlite/` keeps ONLY thin `bootstrap_sqlite`
  (constructs cells via the crate's constructors, returns `EngramProvider` — no
  raw sqlite call; Cargo cycle).
- Move `SqliteOpenOptions` (+ `SqlitePath`, `SqliteJournalMode`) from
  `engram-runtime/options.rs` into `engram-store-sqlite`; drop the
  `engram_runtime` re-exports. `engram-runtime` keeps only neutral primitives
  (`error.rs`, `redaction.rs`).
- **Done when:** all `Sql*` glue + connection config live in
  `engram-store-sqlite`; the facade wiring is thin; green.

### T7: Re-point every remaining consumer to `engram-store-sqlite`
**Depends on:** T6 · **Mode:** goal-based (green)
- Repoint `bindings/node` (~16 legacy modules import `Sql*` directly), `core/eval`
  tests, and `adapters/ingest` tests to import from `engram-store-sqlite`.
  (Coordinate with the `surrealdb-backend` plan's T8 — both touch
  `bindings/node`; do them together to avoid double-churn.)
- **Done when:** no consumer imports the deleted crate paths; all SQLite imports
  come from `engram-store-sqlite`.

### T8: Verify "nothing outside" + delete the old crates
**Depends on:** T7 · **Mode:** goal-based (grep + green)
- Delete the five source adapter-crate directories (code now folded in).
- Verify: `grep -rnE 'rusqlite|SqliteOpenOptions|SqliteJournalMode|SqlitePath'`
  outside `adapters/sqlite/` returns nothing; `grep -rnE 'engram_store_(sql|knowledge_sqlite|belief_sqlite|hierarchy_sqlite|vector)::'`
  returns nothing.
- Full workspace test suite + neutrality hook green; SQLite behavior unchanged.
- **Done when:** `engram-store-sqlite` is the complete self-contained SQLite
  backend (the template to mimic), and the grep gates are clean.

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
