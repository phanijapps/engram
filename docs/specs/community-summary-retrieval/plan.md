# Plan: community-summary-retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

A new engine-neutral adapter crate `engram-store-community-summary` at
`adapters/retrieval/community-summary/`, mirroring the associative-graph adapter
1:1: same `GraphRelationshipSource` trait (reused via dep), same `RetrievalIndex`
for `RetrievalMode::Graph`, same build-result shape. The difference: instead of
PPR, it uses `communities()` from `engram-graph-analytics` to detect communities,
builds a deterministic text summary per community (member names + predicates), and
ranks communities by lexical token-overlap with the query. The top community's
entities are returned as candidates. Zero contract change; wiring deferred.

## Tasks

### T1: Scaffold + `CommunitySummaryIndex` (TDD)
**Depends on:** none

**Tests:** over a stub `GraphRelationshipSource` with 2 communities (e.g.,
`{a,b,c}` + `{d,e}`), a query matching community 0's member names returns a,b,c;
a query matching community 1 returns d,e; deterministic; scope-isolation.

**Approach:**
- `adapters/retrieval/community-summary/Cargo.toml` (deps: engram-domain,
  engram-retrieval, engram-runtime, engram-graph-analytics,
  engram-store-associative-graph (for GraphRelationshipSource), async-trait,
  futures, serde_json; dev chrono).
- `src/lib.rs` — facade re-exporting `CommunitySummaryIndex`.
- `src/index.rs` — `CommunitySummaryIndex: RetrievalIndex`: read entities +
  relationships via source → map edges (all predicates, entity_key pattern) →
  `communities(&edges, 20)` → invert to community→members → build text summary
  per community → rank by lexical overlap with query → return top community's
  entities as `RetrievalResult`s tagged `source = "community_summary"`.

**Done when:** `cargo test -p engram-store-community-summary` green.

### T2: Full gates + ship
**Depends on:** T1

`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace`; `contracts:check-generated` (zero drift);
`check-engine-neutrality.sh`; `check-docs.sh`; grep (no engine/store/LLM in the
crate).
