# Plan: associative-graph-retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done <!-- Drafting | Executing | Done -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Two additive, contract-free pieces, in dependency order:

1. A pure `personalized_pagerank` algorithm added to the zero-dependency
   `engram-graph-analytics` crate, mirroring the existing `pagerank` but with a
   seed personalization vector in place of the uniform teleport distribution.
2. A new engine-neutral adapter crate `engram-store-associative-graph` at
   `adapters/retrieval/associative-graph/` that implements `RetrievalIndex` for
   `RetrievalMode::Graph`. It resolves seed entities from the query by lexical
   name match, reads in-scope entities and relationships through an injected
   `GraphRelationshipSource` trait, maps relationships to a **bidirected** edge
   list (associative recall is direction-agnostic this slice), runs
   `personalized_pagerank` seeded at the query entities, and emits `Entity`
   `RetrievalResult`s ranked by PPR with a `FusionTrace`.

The riskiest part is the PPR walk's edge-boundedness: a graph traversal can only
reach nodes admitted by the edge set, so the walk is confined to the subgraph the
source returns by reading edges exclusively through the injected source (never
cached across requests). A dedicated edge-boundedness test fixes that property.
The adapter is unit-tested with a stub `GraphRelationshipSource` — SQL-free —
asserting on a pure `rank_associative` helper directly (the precedent
`rank_graph_candidates` test pattern) plus one async stub-source test through the
`RetrievalIndex` port. Live-pipeline wiring (N-API bindings / provider) is
deliberately deferred to a separate `associative-graph-wiring` spec; this slice
ships the adapter unit only, matching the `lexical-keyword-retrieval` precedent.

## Constraints

- RFC-0005 — backend-agnostic retrieval composition; a `RetrievalIndex` is
  defined by *what it returns*, never *how* it retrieves.
- `docs/domain-data-model.md` contract-freeze policy — after `Accepted`, only
  compatible additions; `RetrievalMode::Graph` is already frozen, so this slice
  adds no contract.
- ADR-0022 engine-neutrality — no engine type or SQL in `core/` or the SDK
  facade; the new adapter crate depends only on `engram-domain`,
  `engram-retrieval`, `engram-runtime`, and `engram-graph-analytics`.

## Construction tests

**Integration tests:** none beyond per-task tests — the adapter is a candidate
producer unit tested in isolation with a stub source; cross-component
integration belongs to the deferred wiring slice.
**Manual verification:** none — no user-invoked artifact ships in this slice.

## Design (LLD)

### Design decisions

- **Personalized PageRank (not seeded BFS).** Seeded multi-hop BFS over the
  existing `reachability` primitives returns unweighted, unranked sets; PPR
  gives a smooth proximity score for the same ~50-70 lines and is the
  HippoRAG-grounded mechanism. Traces to: AC1, AC3 · `engram-graph-analytics`.
  *Rejected:* approximating with `descendants`+decay — unweighted, awkward
  per-depth scoring.
- **Adapter-local `GraphRelationshipSource` trait, two-method (entities +
  relationships).** No existing port exposes all in-scope relationships, but
  adding a method to `KnowledgeGraphRepository` is a core-port touch beyond the
  lowest-risk slice. A two-method adapter-local trait faithfully mirrors the
  existing `GraphCandidateSource` pattern and is required regardless: lexical
  seed resolution and `RetrievalResult.content`/`provenance` need
  `KnowledgeEntity`, which is only available via the source. Traces to: AC2,
  AC5 · `adapters/retrieval/associative-graph`. *Rejected:* a new
  `KnowledgeGraphRepository::list_relationships` core-port method — core port
  change, deferred.
- **Crate location follows the `cross-encoder-rerank/` precedent.** ADR-0022
  names engine-suffixed cells (`tantivy-lexical`, `sqlite-vec`), but
  `adapters/retrieval/cross-encoder-rerank/` is the engine-neutral adapter cell
  behind an injected trait with no engine dependency — `associative-graph/` is
  that same shape, so the engine-naming rule is satisfied by analogy. Traces to:
  AC2, AC7 · ADR-0022.
- **Bidirected walk over all predicates.** Each relationship emits both
  `(subject, object)` and `(object, subject)` edges so symmetric predicates
  (`related_to`, `depends_on`) carry mass both ways and an entity reached only as
  an `object` is still reachable. Predicate-directionality policy is deferred to
  the Ask-first configurable-predicate item. Traces to: AC3 · `engram-retrieval`.
  *Rejected:* directed-only walk — misses reverse direction for non-directional
  predicates.
- **Entity-only results, lexical seed resolution, all predicates.** Smallest
  deterministic, model-free slice; `Relationship`/`Memory` targets and predicate
  allow-lists are explicit Ask-first deferrals. Traces to: AC2, AC3 ·
  `RetrievalResult`.

### Interfaces & contracts

- Implements `engram_retrieval::RetrievalIndex` (`core/retrieval/src/ports.rs:22`)
  — an already-accepted port; no contract authored or changed.
- Consumes `engram_domain::{RetrievalRequest, RetrievalResult, RetrievalTargetType,
  RetrievalScore, FusionTrace, KnowledgeEntity, KnowledgeRelationship, Scope,
  Policy, Provenance}` and `engram_graph_analytics::personalized_pagerank`.
- Defines the adapter-local two-method `GraphRelationshipSource` trait.

### Component / module decomposition

- `core/graph-analytics/src/pagerank.rs` — gains `personalized_pagerank` (+ test
  module); `lib.rs` exports it.
- `adapters/retrieval/associative-graph/` (new crate `engram-store-associative-graph`):
  - `src/lib.rs` — facade: module declarations + narrow re-exports.
  - `src/source.rs` — `GraphRelationshipSource` trait (`entities(scope)` +
    `relationships(scope)`).
  - `src/seeds.rs` — deterministic lexical seed resolution (query tokens ↔
    in-scope entity names).
  - `src/ranking.rs` — pure `rank_associative(...)` helper (entities +
    relationships + seeds + ppr config → scored entity list, bidirected edges,
    node set = edge endpoints ∪ seeds); unit-tested directly.
  - `src/index.rs` — `AssociativeGraphIndex: RetrievalIndex` glue: resolve seeds,
    read entities+relationships via the source, call `rank_associative`, build
    `RetrievalResult`s (content/provenance from `KnowledgeEntity`, policy from
    the graph default) with `FusionTrace`.

### State & control flow

Stateless per request. Flow: parse `RetrievalRequest` → read in-scope entities +
relationships from the injected source → resolve seed entity IDs lexically → if
no seeds, return `vec![]` → build bidirected edge list → `personalized_pagerank`
→ map scores to `RetrievalResult` → sort + truncate by `request.limit`. No state
carried between requests.

### Failure, edge cases & resilience

- No seeds resolved → empty candidates (no traversal without an anchor).
- Seed entity absent from every edge → still added to the PPR node set, so its
  teleport mass appears in the output (pinned in T1).
- Empty graph / empty edge set → empty candidates (isolated seeds excepted).
- Source returns a `CoreResult` error → propagated as a retrieval source failure
  (the composer reports source failures rather than failing the whole request).
- Non-UTF-8 / malformed predicates → skipped; PPR operates on `String` node keys.

### Quality attributes (NFRs)

- **Determinism:** no clock, no RNG, no HashMap iteration affecting output order
  (results sorted by score desc then id asc before truncation).
- **Edge-boundedness:** the walk invents no edges; out-of-supply entities are
  unreachable by construction (AC5).
- **Engine-neutrality:** zero SQL / engine types in the crate (AC7).
- **Contract stability:** zero v1 drift (AC6).

## Tasks

### T1: Add `personalized_pagerank` to `engram-graph-analytics` (TDD)

**Depends on:** none

**Tests:**
- Convergence on a known fixture graph to expected scores within tolerance.
- Seed bias: with seeds `{A}`, `A` and its direct neighbors score strictly higher
  than a disconnected/equal-structure distant node (AC1).
- Scores sum to approximately one (`(sum-1.0).abs() < 1e-6`).
- Empty edges OR empty seeds → empty map.
- Deterministic across repeated calls (`assert_eq!`).
- Isolated seed: a seed with no edges still appears in the output (the seed is
  part of the node set; its teleport mass is non-zero). This pins the dangling /
  absent-seed behavior the rest of the slice depends on.
- Dangling (sink) seed mass redistributed via the personalization vector, not
  uniformly.

**Approach:**
- Copy the structure of `pagerank.rs`: build `out_neighbors` / `in_neighbors`,
  collect dangling nodes.
- Build the node set as the union of edge endpoints and the supplied seeds (so
  isolated seeds are represented).
- Replace the uniform init (`1/n`) with a personalization vector `p(n)` =
  `1/|seeds|` for seed nodes, `0` otherwise; reject empty seeds (return empty).
- Teleport term becomes `(1-damping) * p[n]`; dangling mass redistributes by
  `p` instead of uniformly; incoming contributions unchanged
  (`damping * Σ scores[m]/outdeg(m)`).
- Iterate to `max_delta < tol` or `iterations`.
- Export from `lib.rs`: `pub use pagerank::personalized_pagerank;`.

**Done when:** `cargo test -p engram-graph-analytics` is green with the new
tests and `personalized_pagerank` is exported.

### T2: Scaffold the `engram-store-associative-graph` crate + two-method `GraphRelationshipSource` trait

**Depends on:** T1

**Tests:**
- Goal-based: `cargo check -p engram-store-associative-graph` compiles; the
  crate is in the workspace; the trait is `async`, two-method, and
  engine-neutral (no SQL / engine type present — verified by grep).

**Approach:**
- Create `adapters/retrieval/associative-graph/Cargo.toml`. **Manifest**
  precedent: `adapters/retrieval/sqlite-vec/Cargo.toml` (it pairs `async-trait`
  + `engram-retrieval`, which the new async `GraphRelationshipSource` trait
  needs); the engine-neutral **shape** precedent is `cross-encoder-rerank/`
  (Design Decisions above), but it is a *sync* adapter with no async deps — do
  not copy its manifest verbatim. Set `name =
  "engram-store-associative-graph"`, `version/edition/license/rust-version`
  workspace-inherited; deps `async-trait`, `engram-domain`, `engram-retrieval`,
  `engram-runtime`, `engram-graph-analytics` (paths `../../../core/<crate>`);
  dev-dep `tokio` with `rt-multi-thread` + `macros` features for async tests.
- Add `"adapters/retrieval/associative-graph"` to root `Cargo.toml` workspace
  members.
- `src/lib.rs` facade + `src/source.rs` defining:
  `#[async_trait] pub trait GraphRelationshipSource: Send + Sync { async fn
  entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>; async fn
  relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;
  }` — faithfully mirroring `GraphCandidateSource`'s two-method shape, because
  seed resolution matches `KnowledgeEntity` names and `RetrievalResult` content /
  provenance are cloned from `KnowledgeEntity`.
- Add the crate to the AGENTS.md `adapters/` target-shape entry.

**Done when:** `cargo check -p engram-store-associative-graph` is green, the
workspace builds, and AGENTS.md lists the new adapter.

### T3a: Pure `rank_associative` + lexical seed resolution (TDD)

**Depends on:** T2

**Tests (pure-helper, asserted directly):**
- Golden ordering: with bidirected relationships `{A→B, B→C, C→D}` and seeds
  `{A}`, `rank_associative` returns `[(A,_),(B,_),(C,_),(D,_)]` by descending PPR
  (exact ordered ids).
- Bidirectional reachability: with a single relationship `A→B` and seed `{B}`,
  `A` appears (reverse direction carries mass).
- Isolated seed: seed `{Z}` with no edges yields `[(Z, _)]` (non-empty).
- No seeds → empty.
- Edge-boundedness: only entity ids that are seed or edge-endpoint appear; an id
  not supplied by the source never appears.
- Determinism: identical inputs → identical output ordering.
- Seed resolution: `resolve_seeds(query, entities)` matches query tokens to
  `KnowledgeEntity` names (case-insensitive; exact then token-containment) and
  returns the right seed ids.

**Approach:**
- `src/seeds.rs`: `resolve_seeds(query: &str, entities: &[KnowledgeEntity]) ->
  Vec<String>` — lowercase query tokens, match against entity name/label; return
  seed node keys (id else name, computed locally).
- `src/ranking.rs`: `rank_associative(entities, relationships, seeds, cfg) ->
  Vec<(String, f64)>` — build the node key map (id else name, computed **locally
  in this module**; do NOT add `engram-codegraph-queries` as a dependency — the
  dependency direction is codegraph → engram, never the reverse), build a
  **bidirected** edge list (push both `(s,o)` and `(o,s)` per relationship over
  all predicates), call `personalized_pagerank(edges, seeds, ...)`, sort by score
  desc then id asc.

**Done when:** `cargo test -p engram-store-associative-graph` (pure-helper
tests) is green.

### T3b: `AssociativeGraphIndex: RetrievalIndex` glue + `FusionTrace` + async stub-source test (TDD)

**Depends on:** T3a

**Tests:**
- Through a stub `GraphRelationshipSource` (owned entities + relationships), a
  `RetrievalRequest` whose query names seed entity `A` returns `Entity`
  `RetrievalResult`s ranked by PPR, with `FusionTrace.source ==
  "associative_graph"` and the resolved seed ids recorded.
- `request.limit` truncates the result set after ranking.
- `RetrievalTargetType::Entity` on every emitted result; `content`/`provenance`
  cloned from the matched `KnowledgeEntity`; `policy` is the graph default.
- Query resolving no seeds → empty `Vec<RetrievalResult>`.

**Approach:**
- `src/index.rs`: `AssociativeGraphIndex { source: Arc<dyn
  GraphRelationshipSource>, config }` implementing `RetrievalIndex`:
  `retrieve_candidates` reads `entities` + `relationships` via the source
  (scope-filtered by the source), calls `resolve_seeds` then `rank_associative`,
  and builds `RetrievalResult { target_type: Entity, score:
  RetrievalScore{total: ppr}, content: entity.summary/name, provenance:
  entity.provenance.clone(), policy: graph_default_policy(), explanation/
  fusion_trace }` with `FusionTrace.source = "associative_graph"`; sort +
  truncate by `request.limit`.
- `Policy` mirrors the precedent's `graph_default_policy()`
  (`adapters/knowledge/sqlite/src/retrieval.rs:79-80,214-223`); `Provenance` is
  cloned from the matched `KnowledgeEntity` (now available via the source's
  `entities()`).
- Test through a small async stub source.

**Done when:** the async stub-source test passes and `cargo test -p
engram-store-associative-graph` is fully green.

### T4: Run workspace gates, engine-neutrality, and contract-freeze verification

**Depends on:** T3b

**Tests:**
- Goal-based: `cargo fmt --all --check`; `cargo check --workspace`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace`; `pnpm run typecheck`; `pnpm run test`;
  `pnpm run contracts:check-generated` (zero drift);
  `.codex/hooks/check-contracts.sh`; `.codex/hooks/check-docs.sh`;
  `.codex/hooks/check-engine-neutrality.sh` (clean on `core/`);
  AND a recursive grep over `adapters/retrieval/associative-graph/` finding no
  `Sql*` / `Pg*` / `Tantivy*` / SQL keyword (the hook does not scan adapter
  crates, so the grep is the new-crate gate).

**Approach:**
- Run the full gate sweep; fix lint/clippy nits in the new code only (no
  bundled fixes outside the touched directories).
- Confirm `contracts/v1` regenerates with no diff (Graph mode is unchanged).
- Confirm the engine-neutrality lint finds no engine type/SQL in `core/`, and
  the grep finds none in the new adapter crate.

**Done when:** every gate above is green and the spec's Acceptance Criteria are
all checked.

## Rollout

- **Delivery:** pure additive Rust — a new zero-dependency algorithm in an
  existing crate and a new adapter crate that no live path imports yet. Ships
  unconditionally (not behind a flag) because it has zero runtime impact until
  the deferred wiring slice composes it. Fully reversible: remove the crate,
  the workspace member, the `lib.rs` export, and the AGENTS.md line.
- **Infrastructure:** none.
- **External-system integration:** none in this slice.
- **Deployment sequencing:** none — the externally visible cutover (composing
  the index into the retrieval pipeline) is the separate
  `associative-graph-wiring` follow-up spec (registered in `docs/backlog.md`).

## Risks

- **PPR scope leakage** — a graph walk reaching across tenants. Mitigated by
  reading edges only through the scope-filtered source and the edge-boundedness
  test (AC5); edges are never cached across requests. The source's own
  `scope_allows` filtering is gated in the deferred wiring slice.
- **PPR semantics correctness** — mitigated by mirroring the proven `pagerank.rs`
  structure and testing convergence / mass conservation / seed bias / isolated
  seeds.
- **Convention drift (crate name / manifest / shape)** — mitigated by mirroring
  the `cross-encoder-rerank` engine-neutral adapter cell and the spec/plan
  adversarial review.
- **Accidental contract touch** — mitigated by `contracts:check-generated`
  (zero drift is an AC) and the contract-freeze policy citation.

## Changelog

- 2026-07-14: initial plan — adapter-unit slice (algorithm + `RetrievalIndex`
  adapter + trait + unit tests); SQL/provider wiring deferred to a follow-up
  `associative-graph-wiring` spec per the `lexical-keyword-retrieval` precedent.
- 2026-07-14: spec/plan adversarial review pass 1 applied — `GraphRelationshipSource`
  made two-method (entities + relationships) so seed resolution and
  `RetrievalResult` content/provenance are well-defined; walk made bidirected
  over all predicates; T3 split into T3a (pure helper) + T3b (index glue);
  dangling/absent-seed behavior pinned in T1; `Policy`/`Provenance` sourcing
  stated; `Constrained by:` narrowed to RFC-0005 + contract-freeze + ADR-0022;
  AC5/AC7 restated; manifest mirror re-anchored to `cross-encoder-rerank`;
  `entity_key` implemented locally (no `engram-codegraph-queries` dependency).
- 2026-07-14: post-EXECUTE review applied and SHIPPED — adversarial pass 2 +
  quality-engineer clean; `entity_key` made id-only (seed/edge key
  consistency), index passes directed edges (no double-bidirection), self-loop
  reverse skipped, manifest mirror corrected to `sqlite-vec`, resolved seed ids
  stamped into result `metadata`, `with_config`/`PprConfig` exposed, error
  propagation from the source tested (17 crate tests, workspace green). Wiring
  deferred to `associative-graph-wiring`.
