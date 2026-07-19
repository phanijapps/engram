# Spec: SurrealDB backend (alternate storage engine)

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine-grid-vs-backend-recipe; **Status: Proposed** — see Ask first), ADR-0005 (storage adapter semantics)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md)
- **Contract:** none — implements existing core storage/retrieval ports; no new contract surface (see new-spec step 4b)
- **Shape:** mixed <!-- data (Surreal schema per capability) + integration (backend recipe wiring) -->

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram ships two selectable storage backends — SQLite and SurrealDB — chosen
by a single config string at provider construction. Both back the same
storage-backed capabilities behind the same core ports and exchange the same
domain DTOs; SurrealDB is a persistence translation, not a second domain model
and not a new contract. A Surreal-backed provider constructs in-process
(embedded SurrealKV) and passes the same backend-parametric conformance suite
the SQLite backend passes. Selecting SurrealDB starts from an empty store —
data does not carry over from a SQLite-backed deployment, by design — and
cross-engine migration is out of scope. Consumers switch engines by editing
configuration, never by rewriting application code.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Implement SurrealDB exclusively behind the existing core ports, reusing the
  existing domain DTOs verbatim — Surreal cells are a persistence translation
  (DTO ↔ Surreal record / typed edge), never a new contract or a parallel
  type system.
- Keep the neutral layers engine-neutral: no `Surreal*` types, no SurrealQL,
  and no `surreal` / `surrealdb` crate import in `core/*` (the 8 port-trait
  crates + the curated `core/integration` port files), `bindings/node`, or
  `core/integration/src/config.rs`. Engine identity appears only as a config
  string. Today `.codex/hooks/check-engine-neutrality.sh` catches `Surreal*`
  type names but **not** `surreal`/`surrealdb` crate imports, and it does not
  yet scan `bindings/node` or `config.rs`; T0 closes both gaps before any
  Surreal code lands.
- Make SurrealDB pass the same backend-parametric conformance suite the SQLite
  backend passes (lifecycle, recall, ranking, policy ops) — passing it is what
  makes Surreal selectable (ADR-0022 rule 4).
- Reach every Surreal-backed capability through BOTH surfaces — the
  `engram-integration` Rust facade and the N-API `bindings/node` — and reflect
  the active engine in `CapabilityReport` (AGENTS.md surface parity).
- Keep lexical retrieval on the engine-agnostic Tantivy adapter
  (`engram-store-lexical`); it indexes the active knowledge store regardless
  of engine. Surreal FTS is a possible future swap, not a v1 dependency.

### Ask first

- Promote **ADR-0022 from `Proposed` → `Accepted`** before the reorg (T2)
  lands — this spec is its named "second engine" trigger event, and the reorg
  executes the ADR's structural moves. Sign-off is recorded in the ADR-0022
  header (status/date flip) + the T2 PR description.
- Confirm "full coverage" means the **storage-backed** capabilities
  (memory, knowledge incl. graph/taxonomy/ontology, beliefs, hierarchy,
  vectors, consolidation) plus the backend-neutral compositions over them —
  not flipping all 19 `CapabilityReport` keys, several of which are
  backend-orthogonal feature slices (hybrid_search, episodes_evidence,
  contradiction, atomic_batch, export_import).
- Before introducing any Surreal-native capability (graph traversal,
  time-travel queries) the current port cannot express — decide whether to
  extend the neutral port or keep the DTO surface identical.
- Before deferring any storage-backed capability out of v1 (full coverage is
  in scope; any deferral needs sign-off).

### Never do

- Never hold SurrealQL, `Surreal*` types, or `surreal`/`surrealdb` crate
  references in any neutral layer (`core/*`, `core/integration` port files,
  `bindings/node`, `config.rs`) — ADR-0022 rule 1, enforced by the neutrality
  lint. [structural]
- Never add a SQLite↔Surreal data-migration path — switching engines starts
  from an empty store by design (explicitly out of scope). [structural]
- Never duplicate domain truth or DTOs — Surreal cells translate the same
  domain types; no second type system in Rust or TypeScript. [structural]
- Never make the two backends a type-level fork of the facade — selection is
  runtime config, and `EngramProvider::open` stays the single neutral entry
  point. [structural]

## Testing Strategy

- **Translation fidelity (TDD):** per-capability Surreal adapter unit tests
  assert round-trip DTO ↔ Surreal record/edge fidelity — typed graph edges,
  bi-temporal belief rows, hierarchy aggregates, vectors, and the
  consolidation-source / recall-lane-resolver seams.
- **Backend conformance (goal-based, integration tests):** the Surreal backend
  passes the existing backend-parametric conformance suite (the same one
  SQLite passes) across lifecycle / recall / ranking / policy ops.
- **Engine-neutrality (goal-based):** `.codex/hooks/check-engine-neutrality.sh`
  is green and, after T0, covers `bindings/node` + `config.rs` and denies
  `surreal`/`surrealdb` crate imports.
- **Engine selection (goal-based):** a test asserts `EngramProvider::open`
  with `engine: surreal` constructs a Surreal-backed provider (and `sqlite`
  the SQLite one), and that a path holding a foreign engine's store yields a
  typed config error.
- **Surface parity (goal-based, E2E):** the N-API binding's capability
  conformance runs against a Surreal-backed provider — every Surreal-backed
  capability is invocable through the binding, not merely constructible.
- **Embed build gate (goal-based):** the SurrealKV embedded dependency builds
  and opens a namespace in-process on the workspace's targets (T1 spike).

## Acceptance Criteria

- [ ] A Surreal-backed provider constructs in-process (embedded SurrealKV),
  selected via the new `surreal` Cargo feature alongside the existing `sqlite`
  feature — `EngramProvider::open` dispatches to the compiled backend, and the
  `[backend] kind = "surreal"` profile (reconciled to embedded `data_root` in
  T3) selects it. (Follows the existing feature-gated `open()` model.)
- [ ] Every **storage-backed** capability — memory, knowledge (graph +
  taxonomy + ontology), beliefs, hierarchy, vectors, and consolidation — is
  backed by Surreal behind its existing core port, exchanging the same DTOs
  as SQLite. The Surreal consolidation adapters (`BeliefSink` /
  `ActiveMemorySource` / `DecayMemorySource`) and the recall-lane resolvers
  (lexical + associative + community over the Surreal knowledge store; vector
  over a Surreal vector index) are wired into `bootstrap_surreal`. The
  backend-neutral compositions (`unified_recall`, `hybrid_search`,
  `atomic_batch`, `export_import`, `observability`, `maintenance`,
  `episodes_evidence`, `contradiction`) are reused and verified functional
  over the Surreal stores. (Scope: storage-backed subset of the 19
  `CapabilityReport` keys; backend-orthogonal feature slices are unchanged.)
- [ ] The backend-parametric conformance suite passes against the Surreal
  backend with the same lifecycle / recall / ranking / policy ops as the
  SQLite fixtures.
- [ ] `.codex/hooks/check-engine-neutrality.sh` is green and, after T0,
  scans `bindings/node` + `core/integration/src/config.rs` and denies
  `surreal`/`surrealdb` crate imports — no Surreal symbol in any neutral
  layer.
- [ ] Every Surreal-backed capability is invocable through BOTH the Rust
  facade and the N-API binding (the existing per-capability binding tests in
  `bindings/node/tests/*.rs` + `packages/node/test/*.ts`, pointed at a
  Surreal-backed provider) — not merely constructible.
- [ ] `CapabilityReport` reports the active engine as `surreal` when
  SurrealDB is selected (and `sqlite` otherwise).
- [ ] Selecting `surreal` on a fresh/empty path starts an empty Surreal store
  (no migration, no carry-over); pointing the provider at a path that already
  holds a different engine's store returns a typed config error rather than
  cross-reading it.
- [ ] The ADR-0022 structural reorg lands: `backends/sqlite` is extracted as a
  sibling recipe crate and `backends/surreal` composes the Surreal adapter
  cells; the engine-neutral facade holds no engine-specific wiring.
- [ ] SurrealKV embeds in-process and builds on the workspace's targets (the
  T1 spike gate) — the whole v1 embedded-only promise depends on it.

## Assumptions

- Technical: engram stack is a Rust Cargo workspace; engine selection today is
  Cargo-feature + config (`bootstrap_sqlite`, `sqlite_storage_layout`); SQLite
  wiring lives in `core/integration/src/sqlite/` + `adapters/integration/src/`.
  (source: repo grep of those modules)
- Technical: no `backends/` crate exists yet — ADR-0022 says it is created when
  a second engine is adopted. (source: repo `ls backends/` → none; ADR-0022
  lines 50, 101)
- Technical: the port seam is already proven backend-parametric —
  `backend-conformance-coverage` (S7) shipped a non-SQLite stub backend
  (HashMap `MemoryService`) passing the same lifecycle ops as SQLite.
  (source: docs/specs/README.md S7 entry)
- Technical: SurrealDB is Rust, multi-model (document + graph + vector +
  time-series), full ACID, embedded + server modes, native graph (tables =
  entity types, typed RELATE edges), native vector + full-text search — maps
  to engram's memory / knowledge / belief / hierarchy / vectors. (source:
  surrealdb.com/platform/surrealdb, /use-cases/knowledge-graphs,
  /why/vs-vector-databases, github.com/surrealdb/surrealdb — vendor-claimed,
  to be spike-verified in T1)
- Technical: SurrealDB beats LanceDB for engram because LanceDB is a
  vector/columnar store with no native graph model — it cannot natively hold
  the knowledge graph / belief / hierarchy capabilities, whereas SurrealDB
  maps them directly. (source: lancedb.com/lp/vector-db-guide +
  surrealdb.com/why/vs-vector-databases — vendor marketing on both sides,
  comparative, not independently benchmarked; the engine choice is made, this
  is rationale, not a load-bearing claim)
- Technical: the engine-neutrality hook today scans the 8 port-trait crates +
  a curated subset of `core/integration` (provider/capability/provenance/
  batch/recall/export_import/observability.rs); it exempts `bindings/node`,
  `config.rs`, `core/eval`, and its crate-import deny-list omits
  `surreal`/`surrealdb`. T0 closes these gaps. (source:
  `.codex/hooks/check-engine-neutrality.sh` lines 13–15, 27–43, 55–57)
- Technical: consolidation has engine-specific adapters today
  (`BeliefSinkAdapter`, `ActiveMemorySourceAdapter`, `DecayMemorySourceAdapter`
  in `core/integration/src/sqlite/consolidation_adapters.rs`) bridging
  `SqlBeliefStore`/`SqlMemoryService` to the reflection + decay executors;
  Surreal needs analogs. (source: consolidation_adapters.rs)
- Technical: unified-recall's lexical + vector + associative + community lanes
  are built over engine-specific resolvers wrapping `SqlKnowledgeStore`
  (`core/integration/src/sqlite/recall_lanes.rs`); Surreal needs analogs over
  the Surreal knowledge store or those lanes silently drop. (source:
  recall_lanes.rs)
- Technical: `CapabilityReport` has 19 keys; only a subset are storage cells
  (memory, knowledge, graph, ontology, taxonomy, beliefs, hierarchy, vectors,
  consolidation). The rest (hybrid_search, episodes_evidence, contradiction,
  atomic_batch, unified_recall, export_import, maintenance, observability,
  migration) are backend-neutral compositions/ops that work over whatever
  stores are wired. (source: core/integration/src/capability.rs)
- Technical: lexical retrieval is already engine-agnostic —
  `engram-store-lexical` (Tantivy) indexes the active knowledge store via a
  resolver, regardless of engine; Surreal FTS is not required for v1.
  (source: recall_lanes.rs `KnowledgeLexicalResolver`)
- Technical: T1 spike (2026-07-16, surrealdb 2.6.5, stable Rust 1.94.1) verified
  embedded SurrealKV builds in-process and supports record round-trip, typed
  RELATE edges, MTREE vector + KNN search, AND MVCC time-travel via
  `SELECT ... VERSION <dt>` (requires `.versioned()` connect opt-in) + changefeeds
  (`SHOW CHANGES ... SINCE`). Bi-temporal still uses explicit valid_from/valid_to
  fields (valid-time is a domain concept no DB infers); native VERSION is reserved
  as a future transaction-time optimization, with v1 mirroring SQLite's explicit-
  field approach for engine-neutral parity. (source: `~/surreal-spike` probe run
  + surrealdb 2.6.5 docs/source)
- Technical: backend selection already has groundwork —
  `core/integration/src/config.rs` defines
  `BackendProfile::{Sqlite, Postgres, Surreal}` (the Surreal variant is
  remote-`endpoint`-shaped, reconciled to embedded `data_root` in T3), and
  `EngramProvider::open` (`provider.rs:176`) dispatches by compiled Cargo
  feature (`#[cfg(feature = "sqlite")]`). Selection is feature-gated, not a
  runtime config-string match. (source: config.rs, provider.rs)
- Process: ADR-0022 is `Status: Proposed`, not `Accepted`; T2 executes its
  structural reorg, so promoting it to `Accepted` is an Ask-first before T2.
  (source: docs/adr/0022 line 3)
- Process: ADR-0022 + the host-sdk brief name SurrealDB as an anticipated
  engine; adopting it triggers extracting `backends/sqlite` + normalizing the
  adapter grid + promoting conventions 2 & 3 to enforced structure.
  (source: ADR-0022 lines 28, 101, 115–121)
- Process: the engine-neutrality lint `.codex/hooks/check-engine-neutrality.sh`
  enforces ADR-0022 rule 1 + 4 over its gated surface (to be extended in T0).
  (source: AGENTS.md + the hook file)
- Process: AGENTS.md surface-parity rule — every capability reachable via BOTH
  `engram-integration` (Rust facade) AND the N-API binding, reflected in
  `CapabilityReport`. (source: AGENTS.md "Surface parity")
- Product: this is a full-backend swap — SurrealDB holds all storage-backed
  capabilities, selectable by config; DTOs are identical across engines, so
  the work is persistence translation only. (source: user confirmation
  2026-07-16)
- Product: v1 ships full storage-backed coverage (no deferrals without
  sign-off). (source: user confirmation 2026-07-16)
- Product: v1 supports embedded in-process mode only (SurrealKV), mirroring
  SQLite; remote server mode is deferred (YAGNI). (source: user confirmation
  2026-07-16)
- Product: motivations are graph-native expressivity, scale performance,
  consumer choice / non-SQL, and unifying records + relationships + vectors in
  one ACID engine. (source: user confirmation 2026-07-16)
