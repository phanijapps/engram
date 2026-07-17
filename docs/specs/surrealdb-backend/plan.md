# Plan: SurrealDB backend (alternate storage engine)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Add a SurrealDB backend as a sibling recipe to SQLite, selectable by config.
The work runs gate → reorg → recipe → cells → select → conform → parity:

1. **Gate first (T0).** Extend `.codex/hooks/check-engine-neutrality.sh` to
   deny `surreal`/`surrealdb` crate imports and to scan `bindings/node` +
   `core/integration/src/config.rs`, so the neutrality claim is true before any
   Surreal code lands.
2. **De-risk the dependency (T1).** Spike-embed SurrealKV in a scratch crate to
   prove it builds + opens a namespace on the workspace's targets. The whole
   v1 embedded-only promise hangs on this.
3. **Reorg (T2 — ADR-0022 debt).** Extract the SQLite recipe out of
   `core/integration/src/sqlite/` + `adapters/integration/` into
   `backends/sqlite`, so `core/integration` becomes fully neutral and a second
   backend has a symmetrical home. Pure relocation — behavior unchanged.
4. **Surreal recipe (T3).** `backends/surreal` behind a `surreal` feature:
   embedded SurrealKV connection lifecycle, config validation, and
   `bootstrap_surreal(config) -> EngramProvider`.
5. **Cells (T4–T6).** Surreal adapter cells behind the existing core ports,
   translating the SAME DTOs to Surreal records + typed `RELATE` edges:
   memory + knowledge (+taxonomy/ontology) + recall-lane resolvers (T4),
   belief + hierarchy + consolidation adapters (T5), the Surreal-native vector
   index + vector lane (T6). Lexical/associative/community lanes reuse the
   engine-agnostic adapters over the Surreal knowledge store.
6. **Select + conform (T7).** Dispatch `engine: sqlite | surreal` by config;
   run the backend-parametric conformance suite against Surreal.
7. **Parity (T8).** Reach Surreal from the N-API binding + TS SDK; reflect it
   in `CapabilityReport`; run the binding's capability conformance over a
   Surreal-backed provider.

The riskiest parts are the SurrealKV embed build (T1 — gates everything), the
up-front reorg (T2), and the bi-temporal belief + hierarchy-aggregate mapping
(T5).

## Constraints

- **ADR-0022** (engine-grid-vs-backend-recipe; Status: Proposed — promote to
  Accepted before T2, per spec Ask first): engine = per-capability adapter
  cell on a capability × engine grid; backend = recipe crate composing adapters
  + owning connection lifecycle + config validation + conformance. A second
  engine triggers extracting `backends/sqlite` + normalizing the grid. Rules 1
  & 4 enforced (after T0) by `.codex/hooks/check-engine-neutrality.sh`.
- **ADR-0005** (storage adapter semantics).
- **AGENTS.md surface-parity**: every capability reachable via BOTH
  `engram-integration` (Rust facade) AND the N-API binding, reflected in
  `CapabilityReport`.
- **DTO parity**: Surreal cells reuse the existing domain DTOs verbatim — no
  new contract, no parallel type system.
- **No migration**: switching engines starts from an empty store.

## Construction tests

**Integration tests:**
- The existing backend-parametric conformance suite (`backend-conformance-
  coverage` S7 harness) runs against the Surreal backend and passes — same
  lifecycle / recall / ranking / policy ops as the SQLite fixtures. (T7.)
- The N-API binding's capability conformance runs against a Surreal-backed
  provider — every Surreal-backed capability invocable through the binding.
  (T8.)

**Manual verification:**
- A write → recall round-trip through the N-API binding on a Surreal-backed
  provider (full consumer journey). (T8.)

## Design (LLD)

### Design decisions

- **Surreal as a recipe, not a fork.** `backends/surreal` composes adapter
  cells behind neutral ports; `core/integration` holds only the neutral
  dispatch `match engine { "sqlite" | "surreal" }` (config strings only).
  Traces to: AC1, AC8.
- **DTO translation, not re-modeling.** Each Surreal cell maps the existing
  DTO ↔ Surreal record/edge; `Entity`/`Relationship` map to Surreal records +
  typed `RELATE` edges. Traces to: AC2.
- **Storage cells vs. reused compositions.** New Surreal work is confined to
  the storage cells (memory/knowledge/taxonomy/ontology/belief/hierarchy/
  vectors/consolidation) + the consolidation adapters + recall-lane resolvers.
  The backend-neutral compositions (unified_recall, hybrid_search,
  atomic_batch, export_import, observability, maintenance, episodes_evidence,
  contradiction) are reused over the Surreal stores. Traces to: AC2.
- **Lexical stays Tantivy.** The lexical lane is engine-agnostic; only its
  knowledge-store resolver is Surreal-backed. Traces to: AC2.
- **Embedded only in v1.** In-process SurrealKV mirrors SQLite's single-binary
  contract; server mode deferred. Traces to: AC1, AC9.

### Data & schema

- One Surreal namespace/database per engram scope; tables mirror the domain
  (`memory_record`, `entity`, `relationship`, `belief`, `hierarchy_node`, …).
  Typed relationships are Surreal graph edges; vectors are inline embedding
  columns with a Surreal vector index; bi-temporal belief rows use Surreal's
  native time-travel for as-of queries. No schema is a new contract — it is
  the persistence shape of the existing DTOs. Traces to: AC2, AC7.

### Interfaces & contracts

- No new contract. Surreal cells implement the existing core ports
  (`MemoryRepository`/`MemoryService`, `KnowledgeRepository`/
  `KnowledgeGraphRepository`/`TaxonomyRepository`/`OntologyRepository`,
  belief + hierarchy ports, `RetrievalIndex` lanes) and the consolidation
  executor traits (`BeliefSink`, `ActiveMemorySource`, `DecayMemorySource`).
  The only new surface is the config string `engine: sqlite | surreal` + the
  `surreal` Cargo feature. Traces to: AC1, AC2.

### Component / module decomposition

- `backends/sqlite` (new, extracted in T2) — SQLite recipe + `bootstrap_sqlite`.
- `backends/surreal` (new, T3) — Surreal recipe + `bootstrap_surreal` + the
  Surreal consolidation adapters + recall-lane resolver newtypes.
- Surreal adapter cells (new, T4–T6), one per storage capability, behind
  existing ports.
- `core/integration` — neutralized: ports + facade + dispatch only.
- `bindings/node` + TS SDK — gain a config branch only (no new types beyond
  the engine literal union). Traces to: AC4, AC8.

### Failure, edge cases & resilience

- Engine mismatch — config points at `surreal` without the `surreal` feature
  compiled, or a path that already holds the other engine's store — returns a
  typed config error; never silently cross-reads a foreign store. Traces to:
  AC7.
- Fresh store on engine switch is the documented contract — no partial/legacy
  read. Traces to: AC7.

### Dependencies & integration

- `surreal` / `surrealdb` embedded Rust SDK + SurrealKV in-process store (new
  dependency, confined to `backends/surreal` + the Surreal cells — never a
  neutral-layer dependency; T1 proves it builds). FastEmbed/Ollama embedding
  providers are reused unchanged; only the vector *index* host changes.
  Traces to: AC2, AC4, AC9.

## Tasks

### T0: Extend the engine-neutrality gate

**Depends on:** none
**Mode:** goal-based (hook self-test)

**Tests:**
- A planted `use surreal::...` / `use surrealdb::...` in a gated neutral layer
  fails the hook; removing it passes.
- Existing workspace stays green (`cargo check --workspace`).
- NOTE: gating `bindings/node` and `core/integration/src/config.rs` is DEFERRED
  — `bindings/node` holds existing `Sql*`/`engram_store_*` wiring (cleared by
  the T2 reorg), and `config.rs` doc-comments spell "SurrealDB" (matches the
  `\bSurreal[A-Z]` pattern; cleared when T3 reconciles the embedded config).

**Approach:**
- DONE 2026-07-16: in `.codex/hooks/check-engine-neutrality.sh`, added `surreal`
  and `surrealdb` to the crate-import deny alternation (both the
  `use`/`extern crate` form and the bare `surreal::`/`surrealdb::` path-ref
  form). Header comment updated.
- DEFERRED: adding `bindings/node/src` and `core/integration/src/config.rs` to
  `GATED_PATHS` moves to T2 (bindings/node neutralization) and T3 (config.rs
  reconciliation) — gating them now fires on existing legitimate engine refs.

**Done when:** DONE 2026-07-16 — the hook denies surreal/surrealdb crate imports
across the gated surface (self-test fires), existing layers pass; the coverage
widening to `bindings/node` (T2) + `config.rs` (T3) is tracked there.

### T1: SurrealKV embed spike (embed build + the load-bearing Surreal capabilities)

**Depends on:** none
**Mode:** goal-based (build + capability probe)

**Tests:**
- A scratch crate embedding SurrealKV builds on the workspace's targets
  (matching the CI matrix) and opens a namespace/database in-process.
- It round-trips a record, creates + queries a typed `RELATE` edge (de-risks
  T4's graph model), runs a vector similarity search over an inline embedding
  column (de-risks T6), and reads a record as-of a past timestamp (de-risks
  T5's bi-temporal time-travel).

**Approach:**
- Stand up a throwaway crate (or `examples/`) pulling the `surreal`/`surrealdb`
  embedded SDK + SurrealKV; open an in-process connection and probe the four
  load-bearing Surreal capabilities above (record, RELATE edge, vector search,
  as-of query). No production wiring.
- Record the working crate version + any target-specific build flags in the
  changelog; this becomes T3's pinned dependency. If any probe fails, surface
  it before T3–T6 commit to Surreal.

**Status: DONE 2026-07-16** (spike at `~/surreal-spike`, surrealdb 2.6.5, stable
Rust 1.94.1). Results: connect ✅, record ✅, relate ✅, vector ✅, **as_of ✅**,
changefeed observed. CORRECTION: an earlier run wrongly concluded "no
time-travel" — it used the wrong syntax (`VERSION TIME`) without the `.versioned()`
connect opt-in. The correct `SELECT ... VERSION <dt>` with `.versioned()` connect
PASSES (record present at `time::now()`, empty at `time::now() - 1d`). ⇒ T5 KEEPS
explicit validity-interval fields (valid-time is a domain concept; engine-neutral
parity with SQLite); native VERSION is reserved as a future transaction-time
optimization, not a v1 dependency.

**Done when:** embed builds in-process AND record/RELATE/vector/as_of probes
pass. ✅ MET.

### T2: Extract `backends/sqlite` recipe (ADR-0022 reorg)

**Depends on:** T0 (+ ADR-0022 Proposed→Accepted sign-off, per spec Ask first)
**Mode:** goal-based (workspace test suite)

**Tests:**
- `cargo check --workspace` + `cargo test --workspace` green after the move
  (behavior unchanged — relocation only).
- `.codex/hooks/check-engine-neutrality.sh` green, with `core/integration`
  holding zero engine symbols (the `src/sqlite/` module has moved out).
- A provider still constructs and round-trips against SQLite exactly as before.

**Approach:**
- Create `backends/sqlite`; relocate `core/integration/src/sqlite/*`
  (`bootstrap_sqlite`, layout resolution, consolidation adapters, recall-lane
  resolvers, adapter composition) and the recipe portion of
  `adapters/integration/src/wiring.rs` into it.
- `core/integration` keeps the engine-neutral ports + `EngramProvider` facade +
  a neutral dispatch entry; it depends on `backends/sqlite` under the existing
  `sqlite` feature.
- Per-store relocation is staged behind the unchanged test suite; cosmetic
  grid renames (`retrieval/sqlite-vec`, `retrieval/tantivy-lexical`,
  `orchestration/belief-sqlite`) are deferred unless they block a Surreal cell.

**Done when:** `core/integration` is engine-symbol-free, `backends/sqlite`
owns the SQLite recipe, and the full workspace test suite is green with no
behavior change. (If blast radius proves too large for one PR, split T2 into a
preceding `backends-sqlite-extraction` spec — the spec's `Constrained by:
ADR-0022` already supports that.)

### T3: `backends/surreal` recipe skeleton + connection lifecycle

**Depends on:** T1, T2
**Mode:** TDD (config validation) + goal-based (open)

**Tests:**
- Constructing a provider with `engine: surreal` opens an embedded SurrealKV
  connection (namespace/database resolved from config) without error.
- Invalid Surreal config (missing path) returns a typed config error. (v1 has
  a single embedded layout — no Surreal layout taxonomy to validate.)
- Pointing at a path holding a different engine's store returns a typed config
  error (no cross-read).
- No `Surreal*` symbol or SurrealQL leaks outside `backends/surreal`
  (neutrality lint green).

**Approach:**
- Add crate `backends/surreal` depending on the T1-pinned surrealdb 2.6.5
  embedded SDK (`kv-surrealkv` feature) + `engram-integration` (facade/builder
  types) + the core port crates.
- **Reconcile the existing config plumbing** (already partly built in
  `core/integration/src/config.rs`): `BackendProfile::Surreal { endpoint }` is
  shaped for a REMOTE endpoint (`ws://...`) — change it to embedded
  `Surreal { data_root }` to match the v1 embedded-only choice (mirrors the
  `Sqlite { data_root }` shape). This also clears config.rs for the T0-deferred
  gating (the "SurrealDB" doc refs get reworded).
- Selection follows the **existing Cargo-feature + `EngramProvider::open`**
  model (`provider.rs:176` dispatches `#[cfg(feature = "sqlite")]`): add a
  `surreal` feature + a `bootstrap_surreal` dispatch branch. The profile's
  `[backend] kind` must match the compiled feature.
- Implement `bootstrap_surreal(config: &EngramConfig) -> CoreResult<EngramProvider>`
  that opens the embedded SurrealKV connection + namespace/database and returns
  engine-neutral handles (empty adapter set for now — cells land in T4–T6).

**Done when:** a Surreal-backed `EngramProvider` is constructible from config in
embedded mode with a validated, isolated connection; foreign-store detection
errors.

### T4: Surreal memory + knowledge + taxonomy + ontology cells + recall-lane resolvers

**Depends on:** T3
**Mode:** TDD

**Tests:**
- Round-trip DTO fidelity for memory records (write/read/forget, lifecycle
  events, idempotency) and knowledge entities/relationships (typed `RELATE`
  edges survive a round-trip); taxonomy + ontology ports return identical DTOs
  to the SQLite cells on shared fixtures.
- Unified-recall returns lexical + associative + community results on a
  Surreal-backed provider (the lanes read the Surreal knowledge store).

**Approach:**
- Add Surreal cells implementing `MemoryRepository`/`MemoryService` and the
  knowledge/taxonomy/ontology repository ports, mapping each DTO to Surreal
  records; model relationships as typed `RELATE` edges.
- Add the Surreal recall-lane seams mirroring `recall_lanes.rs`: a
  `KnowledgeRelationshipSource(SurrealKnowledgeStore)` orphan newtype for the
  associative + community lanes, and a `KnowledgeLexicalResolver` over the
  Surreal knowledge store (Tantivy stays engine-agnostic). Wire
  `associative_recall_lane` / `community_summary_recall_lane` builders.
- Wire all into `bootstrap_surreal`.

**Done when:** memory + knowledge graph + taxonomy + ontology work end-to-end
on Surreal with DTO parity, and the lexical/associative/community recall lanes
return results over the Surreal knowledge store.

### T5: Surreal belief + hierarchy + consolidation adapter cells

**Depends on:** T3, T4
**Mode:** TDD

**Tests:**
- Bi-temporal belief as-of queries return correct results via explicit
  valid_from/valid_to fields + filter (valid-time is a domain concept; v1 mirrors
  SQLite for engine-neutral parity — native Surreal `VERSION` time-travel is a
  documented future optimization for the transaction-time axis); contradiction
  detection + hierarchy aggregate/navigation parity vs SQLite on shared fixtures.
- Consolidation (reflection + decay) runs on a Surreal-backed provider: the
  Surreal `BeliefSink` / `ActiveMemorySource` / `DecayMemorySource` adapters
  bridge the Surreal belief + memory stores to the executors.

**Approach:**
- Add Surreal cells for the belief (synthesis, contradiction, bi-temporal) and
  hierarchy (aggregates, navigation) ports; store bi-temporal validity intervals
  (valid_from/valid_to + transaction time) as explicit fields and filter on them
  — required for valid-time (a domain concept) and engine-neutral parity with
  SQLite. (SurrealKV DOES support `VERSION` time-travel with `.versioned()` —
  T1 spike-verified; reserved as a future transaction-time optimization, not v1.)
- Add the Surreal consolidation adapters mirroring `consolidation_adapters.rs`:
  `BeliefSinkAdapter(SurrealBeliefStore)`, `ActiveMemorySourceAdapter(...)`,
  `DecayMemorySourceAdapter(...)` over the Surreal memory + belief stores.
  (`ExecutorConsolidationService` is backend-neutral — reused as-is.)
- Wire into `bootstrap_surreal`.

**Done when:** belief + hierarchy + consolidation pass on Surreal with DTO and
behavioral parity to SQLite.

### T6: Surreal vector index cell + vector recall lane

**Depends on:** T4
**Mode:** TDD + goal-based (parity)

**Tests:**
- Vector recall fidelity vs the SQLite/sqlite-vec path on shared fixtures.
- The vector recall lane returns results on a Surreal-backed provider (vector
  resolver rehydrates from the Surreal knowledge store).

**Approach:**
- Add a Surreal-native vector index cell behind the vector retrieval port
  (SurrealDB vector search), replacing the sqlite-vec semantics for the
  Surreal backend.
- Add the Surreal vector-lane resolver over the Surreal knowledge store
  (mirroring `KnowledgeVectorResolver`); wire the vector recall lane.
- Reuse the existing RRF fusion + `RerankScorer` injection unchanged.

**Done when:** vector retrieval returns results on the Surreal backend with
parity to the SQLite/sqlite-vec path on shared fixtures.

### T7: Engine selection dispatch + conformance suite green

**Depends on:** T4, T5, T6
**Mode:** goal-based (conformance suite)

**Tests:**
- `EngramProvider::open` with `engine: surreal` yields a Surreal-backed
  provider; `engine: sqlite` yields the SQLite one (selection test).
- The backend-parametric conformance suite (S7 harness) passes against the
  Surreal backend with the same lifecycle / recall / ranking / policy ops as
  SQLite.

**Approach:**
- Wire the neutral dispatch in `core/integration`/`provider.rs::open` to select
  `backends_sqlite` vs `backends_surreal` by compiled Cargo feature (the existing
  `#[cfg(feature = ...)]` model), validating that the profile's `[backend] kind`
  matches the compiled feature.
- Point the conformance harness at the Surreal backend and fix translation gaps
  until green. The existing harness (`adapters/integration/tests/`) already
  exercises recall, batch_ingest, export_import, observability, provenance, and
  associative_recall; **definitely** extend it to also exercise any
  storage-backed capability op not yet covered — at minimum memory lifecycle
  (write/forget), knowledge-graph entities/relationships, taxonomy/ontology,
  beliefs, hierarchy, vectors, and consolidation — sizing that extension as
  part of T7.

**Done when:** engine selection works by config string and the full conformance
suite is green against Surreal.

### T8: Surface parity — N-API binding + TS SDK + CapabilityReport

**Depends on:** T7
**Mode:** goal-based (binding conformance) + E2E + typecheck

**Tests:**
- The existing per-capability binding integration tests
  (`bindings/node/tests/*.rs` — belief, consolidation, hierarchy, ingest,
  integration, …) and the TS transport tests (`packages/node/test/*.ts`) run
  against a Surreal-backed provider — every Surreal-backed capability is
  invocable through the binding, not merely constructible. (There is no single
  unified binding-conformance harness today; these per-capability tests ARE the
  binding surface. Building a unified harness is optional and deferred.)
- A write → recall round-trip succeeds through the binding (E2E).
- `pnpm run typecheck` green; the engine config literal union in
  `packages/contracts` widens to `sqlite | surreal`.
- `CapabilityReport` reports engine `surreal` when Surreal is selected.

**Approach:**
- Add the config branch in `bindings/node`; thread engine selection through
  `packages/node` + `packages/client`; widen the engine literal in
  `packages/contracts`.
- Repoint the existing binding integration tests at a Surreal-backed provider
  fixture; surface the active engine in `CapabilityReport`.

**Done when:** Surreal is reachable from both the Rust facade and the N-API/TS
surface (binding conformance green), reflected in `CapabilityReport`, with an
E2E round-trip green.

### T9: Route `codegraph/mcp-server` through the facade (surface parity)

**Depends on:** T7
**Mode:** goal-based (build + smoke) + Ask first (facade-surface gap, if any)

**Tests:**
- `codegraph/mcp-server` starts against a Surreal-backed provider
  (`engine: surreal`) and answers a codegraph query (e.g. dead-code) end-to-end,
  the same way it does against SQLite.
- No `Sql*` / `engram_store_*` direct construction in `codegraph/mcp-server` —
  it depends on the facade, not the engine adapter crates (engine-neutral).

**Approach:**
- `codegraph/mcp-server` currently constructs `SqlKnowledgeStore` + `LexicalIndex`
  **directly** (bypasses the facade — discovered while diagramming the surfaces).
  Re-point it at `EngramProvider::open` + the knowledge/graph handles + retrieval
  lanes, mirroring `memory/mcp-server`.
- The codegraph queries (dead-code, blast-radius, dependency-path, communities)
  may need operations the facade doesn't expose. If so, surface that as a
  facade-surface gap and either (a) extend the neutral facade, or (b) record a
  documented exception (Ask first) for the queries that can't be facade-routed.

**Done when:** `codegraph/mcp-server` runs against both SQLite and Surreal via
the facade with zero direct engine-store construction, OR a documented exception
is recorded for whichever queries can't be facade-routed.

## Rollout

- **Delivery:** behind the new `surreal` Cargo feature + a config string;
  default remains `sqlite`. Additive — no existing SQLite consumer changes.
  Reversible by config; the only irreversible aspect (a cross-engine data
  migration) is explicitly out of scope.
- **Infrastructure:** none beyond the embedded SurrealKV native dependency
  (in-process, like SQLite) — no server, no network, no secrets in v1.
- **External-system integration:** the `surreal`/`surrealdb` Rust crate must be
  version-matched and buildable on the workspace's targets — proven by the T1
  spike before any cell depends on it.
- **Deployment sequencing:** T0 (gate) → T1 (spike) → T2 (reorg, must not
  change SQLite behavior) → T3 (recipe) → T4–T6 (cells) → T7 (conformance)
  → T8 (parity). Conformance + parity gate selectability.

## Risks

- **SurrealKV embed build (T1)** — the whole v1 promise depends on it building
  in-process on the workspace's targets; mitigated by the up-front spike.
- **T2 reorg size** — extracting `backends/sqlite` touches every store's
  wiring; may itself warrant a preceding `backends-sqlite-extraction` spec.
  Mitigation: pure relocation, guarded by the unchanged test suite.
- **ADR-0022 is Proposed, not Accepted** — T2 executes its structural moves;
  promote to Accepted first (spec Ask first).
- **Bi-temporal belief + hierarchy-aggregate** mapping onto Surreal (T5) —
  DE-RISKED 2026-07-16: T1 spike confirms SurrealKV supports `VERSION`
  time-travel (+ changefeeds); v1 uses explicit validity fields for engine-neutral
  parity (valid-time is domain-owned regardless). Hierarchy aggregates remain the
  translation-risk surface.
- **Embedded Surreal SDK maturity/ergonomics** vs the well-trodden SQLite path
  (idempotency, concurrent-access, bi-temporal parity).

## Changelog

- 2026-07-16: added T9 — route `codegraph/mcp-server` through the facade. Surface-
  parity gap discovered while diagramming the integration / N-API / MCP paths:
  `memory/mcp-server` is already facade-routed (✅); the `bindings/node`
  integration path is facade-routed (✅) but its legacy modules (memory/belief/
  ingest/knowledge/codegraph) + `codegraph/mcp-server` construct `Sql*` stores
  DIRECTLY (❌ bypass). T8 closes the N-API legacy surface; T9 closes
  codegraph/mcp. Both are required for "consumer changes backend, everything
  works." T3 (surreal selection) is implemented + green on branch `surrealdb-backend`.
- 2026-07-16: ARCHITECTURE REVISION (T2 cycle) — extracting `backends/sqlite`
  as a separate crate is impossible: `bootstrap_sqlite` returns an
  `EngramProvider` (owned by `core/integration`) so the backend must depend on
  the facade, while `open()` calls it → Cargo cycle. Decision (user 2026-07-16:
  "all DB ops in one place, easy to switch"): recipes are feature-gated engine
  SUBMODULES of `core/integration` (`src/sqlite/` already; add `src/surreal/`),
  each an exempt engine zone. **T2 collapses** (sqlite recipe is already a
  submodule; no extraction needed). **T3 becomes**: add `src/surreal/` submodule
  + `surreal` feature + `bootstrap_surreal` + `BackendProfile::Surreal { data_root }`
  reconciliation + `open()` dispatch. ADR-0022 amended (recipe = submodule).
  Neutrality + swap-by-config unchanged. The literal `backends/<name>` crate
  remains a future option if a facade/`EngramProvider` split is ever done.
- 2026-07-16: CORRECTION (re-analysis) — the T1 "as_of FAIL / no time-travel"
  finding was WRONG. It used incorrect syntax (`VERSION TIME`) without the
  `.versioned()` connect opt-in. With correct `SELECT ... VERSION <dt>` + a
  versioned connect, time-travel PASSES (record present at `time::now()`, empty at
  `-1d`); changefeeds also work. T5 KEEPS explicit validity fields (valid-time is
  domain-owned; engine-neutral parity) but the false "no MVCC" claim is retracted;
  native VERSION is a documented future transaction-time optimization.
- 2026-07-16: IMPLEMENTATION — T0 + T1 executed. T0: `surreal`/`surrealdb`
  deny-list shipped in `.codex/hooks/check-engine-neutrality.sh` (self-test
  fires, gate green); `bindings/node` + `config.rs` coverage deferred to T2/T3.
  T1 spike (`~/surreal-spike`, surrealdb 2.6.5): connect/record/relate/vector
  PASS, as_of FAIL — **SurrealDB 2.x has no MVCC time-travel**, so T5 uses
  explicit validity-interval fields. Also discovered: `BackendProfile::Surreal
  { endpoint }` is remote-shaped → T3 reconciles to embedded `{ data_root }`;
  selection is Cargo-feature-gated (`provider.rs::open`), not a runtime config
  string → AC1/T3/T7 reframed accordingly.
- 2026-07-16: initial plan scaffolded; engine switched LanceDB → SurrealDB
  (SurrealDB is a better fit: graph-native multi-model maps to engram's
  knowledge/belief/hierarchy; LanceDB is vector-only).
- 2026-07-16: scope confirmed — full storage-backed coverage, embedded
  in-process only, DTO-translation-only.
- 2026-07-16: spec-mode adversarial review pass 2 — expanded T1 spike to also
  probe RELATE/vector/as-of (de-risks T4–T6, not just embed build); made the
  T7 harness extension definite + sized; reframed T8 to repoint the existing
  per-capability binding tests (no hidden build-a-harness scope); dropped the
  undefined "unknown layout" Surreal test; recorded ADR-0022 promotion venue;
  consistent vendor-URL qualifiers.
- 2026-07-16: spec-mode adversarial review pass 1 — restructured to T0–T8:
  added T0 (extend neutrality hook to deny surreal/surrealdb + scan
  bindings/node + config.rs), T1 (SurrealKV embed spike), Surreal
  consolidation adapters (T5) + recall-lane resolvers (T4/T6) the review
  caught as missing, `Mode:` labels per task, and strengthened parity (T8) to
  binding-level conformance; reconciled AC7 fresh-start vs typed-error; noted
  lexical stays Tantivy (engine-agnostic) and ADR-0022 is Proposed.
