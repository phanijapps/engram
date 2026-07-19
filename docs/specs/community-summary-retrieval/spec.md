# Spec: community-summary-retrieval

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0005, ADR-0022, [`associative-graph-retrieval/spec.md`](../associative-graph-retrieval/spec.md) (sibling pattern)
- **Contract:** none — implements the already-frozen `RetrievalMode::Graph`; no v1 change
- **Reuses:** `engram-store-associative-graph::GraphRelationshipSource`, `engram-graph-analytics::communities`
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

A `RetrievalIndex` adapter that detects communities over the knowledge graph,
builds a deterministic text summary per community, and ranks communities by
lexical relevance to the query — returning the top community's member entities as
candidates. This gives agents **global/thematic recall** (whole-corpus questions
like "what clusters of entities exist?") that point-retrieval cannot produce, à la
GraphRAG (Edge et al. 2024, arXiv:2404.16130). Success: a retrieval request
returns entities from the top-matching community, deterministically, with provenance
and no cross-scope leakage, zero v1 contract change.

## Boundaries

### Always do
- Reuse `GraphRelationshipSource` from `engram-store-associative-graph` (no trait duplication).
- Use `communities()` from `engram-graph-analytics` over all-predicate edges (drop codegraph's `calls` filter).
- Build deterministic per-community summaries (member names + intra-community predicates).
- Rank communities by lexical token-overlap with the query; return the top community's entities.
- Keep the crate engine-neutral: no SQL/engine/LLM; data enters only through the injected source.
- Tag results `FusionTrace.source = "community_summary"`.

### Never do
- Change any v1 contract (`RetrievalMode::Graph` is frozen).
- Add SQL, an engine type, or an LLM to `core/` or this crate.
- Persist community summaries as `HierarchyNode` or any domain type (they are transient retrieval artifacts).
- Cache communities across requests or share state between scopes.

## Testing Strategy
- **Pure ranking + community detection: TDD** with a stub `GraphRelationshipSource` — assert deterministic community detection, summary ranking by query relevance, top-community entity return, scope-isolation-by-construction.
- **Engine-neutrality + no-contract-change: goal-based** — `Cargo.toml` deps are port crates only; `contracts:check-generated` zero drift.

## Acceptance Criteria
- [x] `engram-store-community-summary` at `adapters/retrieval/community-summary/` implements `RetrievalIndex` for `RetrievalMode::Graph`, returning `Entity` candidates from the top query-matching community.
- [x] Given a seeded KG with 2+ communities, the query returns entities from the lexically-best-matching community, deterministically (a test asserts the exact entity set on a fixture).
- [x] Communities are detected via `engram_graph_analytics::communities()` over all-predicate edges; summaries are deterministic text (member names + predicates).
- [x] The walk invents no edges — only the source's scope-filtered relationships are used (scope-safe by construction).
- [x] Zero v1 contract change (`contracts:check-generated` clean); engine-neutral (no engine type/SQL in the crate).
- [x] All repository gates green.

## Assumptions
- `communities(edges, max_passes) -> HashMap<N, usize>` is deterministic, undirected, single-level Louvain (`core/graph-analytics/src/communities.rs:20`). (source: recon)
- `GraphRelationshipSource` (entities + relationships by scope) is `pub` in `engram-store-associative-graph` and reusable. (source: recon)
- The codegraph `entity_key` + `call_edges` pattern (minus the `calls` filter) is the edge-mapping template. (source: recon)
- The `resolve_seeds` lexical-match logic from `associative-graph/src/seeds.rs` is reusable for summary ranking. (source: recon)
- Community summaries are transient (not persisted); no overlap with the deferred `HierarchyNode.summary` model-assisted summaries. (source: recon)
