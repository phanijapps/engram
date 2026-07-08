# Spec: graph-analytics

- **Status:** Shipped (PageRank + betweenness; Louvain pending as B5)
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (items B3/B4/B5), `docs/domain-data-model.md`
- **Brief:** none
- **Contract:** none — pure algorithms; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

**Scope.** A focused pure-algorithm crate (`engram-graph-analytics`) with graph
centrality / community primitives over a generic directed edge list, decoupled
from domain types. Callers map `KnowledgeRelationship` (or any edge source) to
`(source, target)` id pairs at the call site. No dependencies.

**Shipped:** PageRank centrality (B3), betweenness centrality (Brandes, B4).
**Pending:** Louvain community detection → hierarchy clusters (B5).

## Objective

Graph-analytics returns node centrality scores and (later) communities for a
directed graph, enabling popularity-prior retrieval, bridge-symbol detection,
community/cluster detection, and code-graph queries (blast-radius, dead-code)
downstream. General-purpose: any edge list benefits, not code alone.

## Boundaries

### Always do
- Operate on a generic edge list, decoupled from `KnowledgeRelationship`/
  `EntityRef` identity ambiguity.
- Keep the crate dependency-free (std-only).

### Ask first
- Coupling the algorithms to `engram-domain` types directly (currently declined).

### Never do
- Add a dependency, or put graph algorithms in `engram-domain`/`engram-retrieval`
  core as a god-module.

## Testing Strategy

- **TDD** — each algorithm on known small graphs (deterministic ordering /
  values); per-crate gates (fmt/clippy `-D warnings`/test).

## Acceptance Criteria

- [x] `pagerank(edges, damping, iterations, tol)` returns scores summing to ~1.0;
  cycle → near-uniform; two-source → sink ranks sink highest; empty → empty.
- [x] `betweenness(edges)` returns Brandes betweenness; a bridge node carries all
  through-traffic; parallel shortest paths split credit (0.5 each); empty → empty.
- [ ] Louvain community detection → `HierarchyNode(kind=cluster)` (B5).
- [x] No dependencies; per-crate gates green.
- [x] No public contract change.

## Assumptions

- Technical: graph analytics was absent from the repo (A1 audit). The algorithms
  are pure and need no contract acquisition.
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
