# Spec: codegraph-queries

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (C4/C5 + dependency-path), AGENTS.md (on-top layer)
- **Brief:** none
- **Contract:** none — consumes accepted domain types + the shipped graph-analytics primitives; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

**Scope.** The **first on-top codegraph crate** — the "build on top of engram"
layer (RFC-0012, repo-strategy Q1 = monorepo-new-layer at `codegraph/`).
Code-specific queries over knowledge-graph **call edges**: dead-code (C4),
blast-radius (C5), and dependency-path — thin interpretations of the shipped
`engram-graph-analytics` primitives, scoped to `KnowledgeRelationship`
predicate `calls`.

## Objective

A coding agent asks "what breaks if I change `X`?" (blast-radius), "find dead
code" (dead-code), or "how does `X` depend on `Y`?" (dependency-path) over the
indexed call graph. This crate maps `KnowledgeRelationship` → call edges → those
three queries, delegating the graph math to `engram-graph-analytics`.

## Boundaries

### Always do
- Map `KnowledgeRelationship` (predicate `calls`) → a generic `(caller, callee)`
  edge list via `entity_key` (id if resolved, else name); delegate to
  `engram-graph-analytics`.
- Keep the crate on-top: depend only on `engram-domain` +
  `engram-graph-analytics`; no storage, no infra, no contract change.

### Ask first
- Richer result shapes (file:line, kind) — currently plain keys; enrich at the
  wiring layer.

### Never do
- Reimplement graph algorithms (use `engram-graph-analytics`) or duplicate
  domain truth.
- Depend on storage adapters, bindings, or infrastructure.

## Testing Strategy

- **TDD** — `dead_code` (zero in-degree), `blast_radius` (transitive callers),
  `dependency_path` (shortest path) over `KnowledgeRelationship` fixtures; plus
  `call_edges` filtering (non-`calls` predicates + unresolved refs skipped).

## Acceptance Criteria

- [ ] `dead_code` returns the zero-caller symbols (sorted, deterministic).
- [ ] `blast_radius(target, depth)` returns the transitive callers within `depth`.
- [ ] `dependency_path(from, to)` returns the shortest call path or `None`.
- [ ] Non-`calls` predicates and refs without a key are skipped.
- [ ] No contract change; depends only on `engram-domain` +
  `engram-graph-analytics`; per-crate gates green.

## Assumptions

- Technical: the graph primitives (`in_degree`, `ancestors`, `shortest_path`)
  ship in `engram-graph-analytics`; `calls` edges exist via the ingest extractor.
  (source: B3–B5 + reachability; A1 audit)
- Process: light mode. (source: user confirmation 2026-07-08)
