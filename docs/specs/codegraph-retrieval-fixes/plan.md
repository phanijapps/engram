# Plan: codegraph-retrieval-fixes

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing <!-- D1 shipped (PR #95); D3 remains. D2 + D2.1a/b/c moved to recall-fusion-config (RFC-0019). -->

> **Plan contract:** implementation strategy; changes noted in the Changelog.

## Approach

D1 (bounded traversal) **shipped** as PR #95. D2 (search) and the hybrid-recall work (D2.1a/b/c) moved to [`../recall-fusion-config/plan.md`](../recall-fusion-config/plan.md) under RFC-0019 — they are repo-wide recall, not codegraph-scoped. **Only D3 (honest `fetch_rels`) remains here.**

## Constraints

- RFC-0018 (D1/D3); ADR-0022 (binding unchanged). D2 superseded by RFC-0019.
- D3 is behavior-only in `mcp/engram-mcp/src/codegraph.rs`; no contract/storage change.

## Tasks

### T1: Honest `fetch_rels` (D3)

**Depends on:** none (independent)

**Tests:** (goal-based) the five pure graph tools (`symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed`) propagate a store/capability error as `ToolError`; `get_context` degrades to recall + a `(graph unavailable: …)` note. `cargo check --workspace` + `cargo test --workspace` green.

**Approach:** `fetch_rels` (`mcp/engram-mcp/src/codegraph.rs:162-168`) returns `Result<Vec<_>, ToolError>`; distinguish unwired `knowledge_query` capability vs store error; propagate in the five graph tools; degrade in `get_context` (which computes recall before `fetch_rels`).

**Done when:** graph tools fail loudly on store unavailability; `get_context` degrades; workspace green.

## Rollout

Behavior-only; reversible. No schema/infra/contract change.

## Risks

- Error propagation changes tool-call ergonomics (callers that swallowed empty graphs now get errors) — intended; document it.

## Changelog

- 2026-08-01: D1 (bounded traversal) shipped as PR #95.
- 2026-08-01: D2 + D2.1a/b/c (hybrid recall fusion) moved to `recall-fusion-config` (RFC-0019) after the pre-EXECUTE review — repo-wide recall, not codegraph. Only D3 (honest `fetch_rels`) remains here.
