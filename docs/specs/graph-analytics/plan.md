# Plan: graph-analytics

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy; may change as we learn.

**Light-mode lean fill.** Approach + short Tasks list.

## Approach

A std-only crate `engram-graph-analytics` at `core/graph-analytics` exposing
`pagerank(edges, damping, iterations, tol) -> HashMap<String, f64>` over a
generic `(source, target)` edge list. Standard iterative PageRank with uniform
dangling-mass distribution and tolerance-based early stop. Betweenness (B4) and
Louvain (B5) are follow-on micro-specs in the same crate.

## Constraints

- RFC-0012 + codegraph-parity-roadmap (B3). Pure algorithm — no contract change.

## Tasks

### T1: crate + `pagerank` + tests
**Depends on:** none
**Tests:**
- cycle → near-uniform; two-source sink → sink highest; empty → empty; sum ≈ 1.0.
**Approach:**
- `core/graph-analytics/` (`engram-graph-analytics`, workspace member, std-only);
  `lib.rs` facade + `pagerank.rs` (algorithm + tests).
**Done when:** `cargo test -p engram-graph-analytics` green; fmt + clippy clean.

## Risks

- None material — pure algorithm with deterministic tests.

## Changelog

- 2026-07-08: initial plan (light mode); PageRank only (B4/B5 follow).
