# Plan: codegraph-queries

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy; may change as we learn.

**Light-mode lean fill.** Approach + one task.

## Approach

A new on-top crate `engram-codegraph-queries` at `codegraph/queries/` (the first
`codegraph/` layer crate). It maps `KnowledgeRelationship` (predicate `calls`)
to a generic `(caller, callee)` edge list via `entity_key` (resolved id, else
name), then exposes `dead_code` / `blast_radius` / `dependency_path` as thin
wrappers over `engram-graph-analytics::{in_degree, ancestors, shortest_path}`.
Depends only on `engram-domain` + `engram-graph-analytics`.

## Constraints

- RFC-0012 (on-top layer) + codegraph-parity-roadmap (C4/C5 + dependency-path).
- AGENTS.md: on-top crate — no storage/infra/contract; reuse the primitives.

## Tasks

### T1: crate + edge mapping + 3 queries + tests
**Depends on:** engram-graph-analytics reachability (shipped)
**Tests:**
- `call_edges` keeps `calls` edges, drops other predicates + unresolved refs.
- `dead_code` returns the zero-caller set (sorted).
- `blast_radius(target, depth)` returns transitive callers within depth.
- `dependency_path(from, to)` returns the shortest call path or `None`.
**Approach:**
- `codegraph/queries/` (`engram-codegraph-queries`, workspace member); `lib.rs`
  facade + `queries.rs` (`entity_key`, `call_edges`, `dead_code`, `blast_radius`,
  `dependency_path` + tests).
**Done when:** `cargo test -p engram-codegraph-queries` green; fmt + clippy clean.

## Risks

- `entity_key` mixing resolved ids and name-only refs could fragment a node's
  identity once cross-file resolution improves (C1); acceptable for now — the
  queries operate on whatever keys the current extractor emits.

## Changelog

- 2026-07-08: initial plan (light mode); first on-top codegraph crate.
