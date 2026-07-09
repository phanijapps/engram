# Spec: graph-analytics

- **Status:** Shipped (PageRank + betweenness + communities + reachability)
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (items B3/B4/B5 + reachability primitives for C4/C5), `docs/domain-data-model.md`
- **Brief:** none
- **Contract:** none — pure algorithms; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

**Scope.** A focused pure-algorithm crate (`engram-graph-analytics`) with graph
centrality / community / traversal primitives over a generic directed edge list,
decoupled from domain types. Callers map `KnowledgeRelationship` (or any edge
source) to `(source, target)` id pairs at the call site. No dependencies.

**Shipped:** PageRank centrality (B3), betweenness centrality — Brandes (B4),
community detection — single-level modularity-greedy Louvain local-moving (B5),
and reachability primitives — `in_degree`, `ancestors`, `shortest_path` (enable
dead-code C4, blast-radius C5, dependency-path).
**Follow-ups (out of scope here):** Louvain multi-level aggregation; wiring the
outputs to retrieval (popularity prior, bridge detection) and to
`HierarchyNode(kind=cluster)`; the code-specific dead-code/blast-radius queries
that consume these primitives.

## Objective

Graph-analytics returns node centrality scores, communities, and traversal
results for a directed graph, enabling popularity-prior retrieval, bridge-symbol
detection, community/cluster detection, dead-code / blast-radius / dependency
queries, and code-graph analysis downstream. General-purpose: any edge list
benefits, not code alone.

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

- **TDD** — each algorithm/primitive on known small graphs (deterministic
  ordering / values / partitions / paths); per-crate gates
  (fmt/clippy `-D warnings`/test).

## Acceptance Criteria

- [x] `pagerank(edges, damping, iterations, tol)` returns scores summing to ~1.0;
  cycle → near-uniform; two-source → sink ranks sink highest; empty → empty.
- [x] `betweenness(edges)` returns Brandes betweenness; a bridge node carries all
  through-traffic; parallel shortest paths split credit (0.5 each); empty → empty.
- [x] `communities(edges, max_passes)` returns a node→label partition via
  single-level modularity-greedy Louvain; triangle → one community; disconnected
  cliques stay separate; empty → empty.
- [x] `in_degree` / `ancestors` / `descendants` / `shortest_path` traversal
  primitives: in-degree counts incoming; ancestors/descendants return transitive
  callers/callees within a depth; shortest_path returns the BFS path or `None`.
- [x] No dependencies; per-crate gates green (21 tests).
- [x] No public contract change.

## Assumptions

- Technical: graph analytics was absent from the repo (A1 audit). The algorithms
  are pure and need no contract acquisition.
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
