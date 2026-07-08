# Spec: graph-analytics

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (item B3), `docs/domain-data-model.md`
- **Brief:** none
- **Contract:** none — pure algorithms; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

**Scope.** A focused pure-algorithm crate with **PageRank** centrality over a
generic directed edge list (betweenness and Louvain are follow-on micro-specs
B4/B5). Decoupled from domain types — callers map `KnowledgeRelationship` →
edge pairs at the call site — so the algorithms are reusable and testable in
isolation.

## Objective

Graph-analytics returns node centrality scores (PageRank first) for a directed
graph, enabling popularity-prior retrieval, community/cluster detection, and
code-graph queries (blast-radius, dead-code) downstream. It is general-purpose:
any edge list benefits, not code alone.

## Boundaries

### Always do
- Operate on a generic edge list (`&[(String, String)]`), decoupled from
  `KnowledgeRelationship`/`EntityRef` identity ambiguity.
- Handle dangling nodes (no out-edges) by distributing their mass uniformly.
- Keep the crate dependency-free (std-only).

### Ask first
- Coupling the algorithms to `engram-domain` types directly (currently declined
  — the edge-list boundary keeps it reusable).

### Never do
- Add a dependency, or put graph algorithms in `engram-domain`/`engram-retrieval`
  core as a god-module.
- Block on betweenness/Louvain — they are B4/B5.

## Testing Strategy

- **TDD** — PageRank on known small graphs: cycle → uniform; a two-source sink →
  sink ranks highest; empty → empty; deterministic.

## Acceptance Criteria

- [ ] `pagerank(edges, damping, iterations, tol)` returns scores summing to ~1.0.
- [ ] A 3-node cycle yields near-uniform scores (within tol of 1/3).
- [ ] A two-source → sink graph ranks the sink strictly highest.
- [ ] An empty edge list returns an empty map.
- [ ] No dependencies; per-crate gates green (fmt/clippy `-D warnings`/test).
- [ ] No public contract change.

## Assumptions

- Technical: graph analytics is absent from the repo (A1 audit). The algorithms
  are pure and need no contract acquisition.
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
