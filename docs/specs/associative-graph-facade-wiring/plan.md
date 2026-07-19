# Plan: associative-graph-facade-wiring

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** the implementation strategy. May change as you learn; note
> substantive changes in the changelog.

## Approach

Add associative retrieval as a unified-recall lane in the Rust SDK facade — the
zero-contract-change path (the recon confirms `RetrievalMode::Graph` is already
frozen and lanes are composed in code, not a domain enum). Two pieces, both in
the ADR-0022 exempt zone `core/integration/src/sqlite/`:

1. A `KnowledgeRelationshipSource` newtype in `recall_lanes.rs` (verbatim copy of
   the binding's wrapper at `bindings/node/src/knowledge_fusion.rs:43-53`), an
   orphan-rule-safe `impl GraphRelationshipSource` delegating to
   `SqlKnowledgeStore::list_entities` / `list_relationships`.
2. Construct `AssociativeGraphIndex::new(...)` and push it into `retrieval_lanes`
   in `bootstrap.rs`, immediately after the lexical `GraphRetrievalIndex` lane,
   inside the existing `if let Some(knowledge_handle)` block.

Plus the `engram-store-associative-graph` dep on `core/integration` behind its
`sqlite` feature. A Rust embedder's `provider.recall(request)` then fans
associative candidates into the existing RRF + `compose_context` automatically.
The riskiest part is keeping the engine-naming newtype + imports out of gated
files (they stay in `src/sqlite/`); the engine-neutrality lint gates that.

## Constraints

- AGENTS.md surface-parity rule — this slice is what makes associative compliant
  (reachable via `engram-integration`, not only the binding).
- RFC-0005 — retrieval composition; associative is one more `RetrievalIndex` lane.
- ADR-0022 — the `SqlKnowledgeStore`-naming newtype + engine imports live in
  `core/integration/src/sqlite/` (exempt), never in gated `*.rs` files.
- `associative-graph-retrieval/spec.md` — the adapter unit is unchanged.

## Construction tests

**Integration test:** one `recall` E2E over a seeded sqlite `EngramProvider`
asserting `associative_graph`-tagged candidates appear + scope isolation.

## Design (LLD)

### Design decisions

- **Unified-recall lane only** (not a standalone op). Zero contract change
  (`RetrievalMode::Graph` frozen; associative is another `RetrievalIndex` in the
  existing Vec) and it mirrors the lexical lane exactly. A standalone op would be
  heavier (new engine-neutral port trait = contract change). Traces to: all ACs.
- **Newtype in `recall_lanes.rs`** (not `bootstrap.rs`). Mirrors
  `KnowledgeLexicalResolver` (`recall_lanes.rs:39`); keeps `bootstrap.rs` to
  construction only. Traces to: AC2, AC5.

### Component / module decomposition

- `core/integration/src/sqlite/recall_lanes.rs` — `KnowledgeRelationshipSource`
  newtype + `impl GraphRelationshipSource`.
- `core/integration/src/sqlite/bootstrap.rs` — `AssociativeGraphIndex::new(...)`
  push into `retrieval_lanes` (after the lexical graph lane, ~line 267).
- `core/integration/Cargo.toml` — `engram-store-associative-graph` optional dep +
  added to the `sqlite` feature array.

### Failure, edge cases & resilience

- No seed resolves → the lane returns empty candidates; recall still returns the
  other lanes' results (associative just contributes nothing).
- Out-of-scope edges → never walked (the store scope-filters at the read
  boundary, inherited by the newtype).
- Empty knowledge store → associative lane contributes nothing; recall unchanged.
- Lane overlap — an entity returned by both the lexical and associative graph
  lanes is merged by the fusion's `(target_type, target_id)` dedup (appears once,
  RRF-boosted by cross-source consensus); T2 pins the observed outcome.

## Tasks

### T1: `KnowledgeRelationshipSource` newtype + Cargo dep

**Depends on:** none

**Tests:**
- Goal-based: `cargo check -p engram-integration --features sqlite` compiles with
  the new dep; the newtype is in `recall_lanes.rs` (exempt zone).

**Approach:**
- `core/integration/Cargo.toml`: add
  `engram-store-associative-graph = { path = "../../adapters/retrieval/associative-graph", optional = true }`
  and append `"dep:engram-store-associative-graph"` to the `sqlite` feature.
- `recall_lanes.rs`: copy the binding's
  `KnowledgeRelationshipSource(pub(crate) Arc<SqlKnowledgeStore>)` + its
  `#[async_trait] impl GraphRelationshipSource` (delegating to `list_entities` /
  `list_relationships`) verbatim, with the needed imports (`async_trait`,
  `engram_domain::{KnowledgeEntity, KnowledgeRelationship, Scope}`,
  `engram_runtime::CoreResult`, `engram_store_associative_graph::GraphRelationshipSource`,
  `engram_store_knowledge_sqlite::SqlKnowledgeStore`).

**Done when:** `cargo check -p engram-integration --features sqlite` is green.

### T2: Construct the associative lane + `recall` E2E test (TDD)

**Depends on:** T1

**Tests:**
- `recall` over a seeded sqlite `SqlUnifiedRecall` (with the associative lane)
  returns `associative_graph`-tagged `Entity` candidates when the query names a
  seed entity; an out-of-scope tenant entity does not appear (scope isolation).
- An entity returned by both the lexical and associative graph lanes appears at
  most once in the `ContextPayload` (pin the observed dedup outcome).

**Approach:**
- `bootstrap.rs`: inside `if let Some(knowledge_handle) = &knowledge_store { ... }`,
  after the lexical graph lane push, push
  `Arc::new(AssociativeGraphIndex::new(Arc::new(KnowledgeRelationshipSource(knowledge_handle.clone()))))`
  into `retrieval_lanes`.
- Test harness — `engram-conformance` (`adapters/integration/tests/`, e.g. a new
  `associative_recall.rs` or an extension of `recall.rs`), which owns the sqlite
  test infra (`core/integration` has none by design). Read `recall.rs` to see how
  `SqlUnifiedRecall` is constructed there; if it goes through `bootstrap` the lane
  is auto-included (seed an entity graph + assert); if constructed directly, add
  the associative lane (`AssociativeGraphIndex::new(Arc::new(
  KnowledgeRelationshipSource(store.clone())))`) to that construction. Seed via
  `KnowledgeRepository::put_entity` / `put_relationship`
  (`core/knowledge/src/repository.rs:29,37`); assert
  `associative_graph`-tagged `Entity` items in the `ContextPayload` after
  `recall(request)`, plus the scope-isolation and dedup assertions above.

**Done when:** the recall E2E test passes and `cargo test -p engram-conformance`
is green.

### T3: Full gates + no-drift + engine-neutrality verification

**Depends on:** T2

**Tests:** goal-based — `cargo fmt --all --check`; `cargo check --workspace`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
`pnpm run typecheck`; `pnpm run build`; `pnpm run contracts:check-generated`
(zero drift); `.codex/hooks/check-contracts.sh`; `.codex/hooks/check-docs.sh`;
`.codex/hooks/check-engine-neutrality.sh`; AND a recursive grep over the files
`check-engine-neutrality.sh` gates under `core/integration/src/` (`provider.rs`,
`capability.rs`, `recall.rs`, `provenance.rs`, `batch.rs`, `export_import.rs`,
`observability.rs`) finding no `Sql*`/engine type (the exempt zone is `src/sqlite/`
only).

**Done when:** every gate is green and the spec's ACs are all checked.

## Rollout

- **Delivery:** additive — one new lane in the facade's recall fan-out. No
  existing lane changes; reversible by removing the push + the newtype + the dep.
  Ships unconditionally (associative candidates fuse in automatically for any
  recall over a populated knowledge graph).
- **Infrastructure:** none.
- **External-system integration:** none.

## Risks

- **Engine-neutrality** — the newtype names `SqlKnowledgeStore`. Mitigated by
  keeping it in `src/sqlite/` (exempt) + the T3 grep over gated files.
- **Recall E2E test construction** — building a sqlite `EngramProvider` in a test.
  Mitigated by modeling on the existing recall conformance tests.
- **Accidental contract touch** — mitigated by `contracts:check-generated` (zero
  drift is an AC).

## Changelog

- 2026-07-15: initial plan — unified-recall lane only; newtype in `recall_lanes.rs`;
  construction in `bootstrap.rs`; dep behind the `sqlite` feature.
- 2026-07-15: SHIPPED — adversarial + quality-engineer passes: the E2E test now
  composes BOTH the lexical and associative graph lanes and pins the RRF dedup
  (entity in both lanes appears once); source filter fixed to `contains`
  (RRF merges cross-source candidates into `"graph+associative_graph"`). Exposed a
  `pub associative_recall_lane` constructor (one newtype, used by bootstrap +
  tests). Facade testing scoped to the `SqlUnifiedRecall` level by design
  (`core/integration` has no sqlite test infra); the bootstrap lane push is
  compile-verified + mirrors the lexical lane. Closes the AGENTS.md surface-parity
  gap for associative.
