# Spec: graph-analytics

- **Status:** Shipped (PageRank + betweenness + communities)
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

**Shipped:** PageRank centrality (B3), betweenness centrality — Brandes (B4),
community detection — single-level modularity-greedy Louvain local-moving (B5).
**Follow-ups (out of scope here):** Louvain multi-level aggregation; wiring the
outputs to retrieval (popularity prior, bridge detection) and to
`HierarchyNode(kind=cluster)`.

## Objective

Graph-analytics returns node centrality scores and communities for a directed
graph, enabling popularity-prior retrieval, bridge-symbol detection,
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
  values / partitions); per-crate gates (fmt/clippy `-D warnings`/test).

## Acceptance Criteria

- [x] `pagerank(edges, damping, iterations, tol)` returns scores summing to ~1.0;
  cycle → near-uniform; two-source → sink ranks sink highest; empty → empty.
- [x] `betweenness(edges)` returns Brandes betweenness; a bridge node carries all
  through-traffic; parallel shortest paths split credit (0.5 each); empty → empty.
- [x] `communities(edges, max_passes)` returns a node→label partition via
  single-level modularity-greedy Louvain local-moving; a triangle collapses to
  one community; disconnected cliques stay separate; empty → empty.
- [x] No dependencies; per-crate gates green (13 tests).
- [x] No public contract change.

## Assumptions

- Technical: graph analytics was absent from the repo (A1 audit). The algorithms
  are pure and need no contract acquisition.
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
