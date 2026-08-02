# Spec: codegraph-retrieval-fixes

- **Status:** Implementing <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0018 (Accepted), ADR-0022 (engine grid / surface parity)
- **Brief:** none
- **Contract:** none <!-- internal Rust API; no contracts/<type>/ surface -->
- **Shape:** mixed <!-- core graph logic + MCP tool wiring -->

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The code-intelligence tools return **correct, bounded, and honest** results. An
agent debugging real code gets the few relevant symbols, not floods or silent
empties: `symbol_context` / `change_impact` return a bounded neighborhood with a
truncation signal instead of a transitive explosion through generic hub names;
`search` returns ranked symbol hits for multi-term queries instead of "no
results"; and the graph tools surface store/capability errors instead of
masquerading them as empty graphs. The three fixes ship as three loops under
this spec — **D1 (bounded traversal) is this loop**; D3 (honest `fetch_rels`)
and D2 (BM25 `search` via a `LexicalSearch` facade trait) are subsequent loops.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.

### Always do

- Keep `engram-graph-analytics` pure, generic over `N`, and dependency-free (no `engram-domain`, no storage) — AGENTS.md:151.
- Additive only: add new `*_bounded` variants beside unchanged originals.
- Bound per-direction (ancestors and descendants capped independently) and surface a `truncated` flag for both `symbol_context` and `change_impact`.

### Ask first

- Changing any existing public signature in `engram-graph-analytics` or `engram-codegraph-queries`.
- Enabling `fastembed` or wiring the reranker (a separate, larger initiative — out of scope).

### Never do

- Add a dependency to `engram-graph-analytics`, or import `engram-domain` / storage types there.
- Modify the N-API binding's `blast_radius` / `symbol_context` surface (composite parity is deferred).
- Change the storage schema or any frozen-v1 contract type.
- Introduce hub-degree pruning in this spec (documented follow-on).

## Testing Strategy

- **D1 bounded-traversal logic — TDD.** Pure functions with compressible invariants (cap truncation, depth interaction, per-direction independence). Unit tests in `engram-graph-analytics` and `engram-codegraph-queries`. This is where the bound's correctness lives.
- **D1 MCP wiring (depth defaults, `truncated` print) — goal-based check.** `cargo check --workspace` + `cargo test --workspace` green; the depth-default reduction verified by code, the bounded output proven through the bounded-variant unit tests the wiring calls. The MCP tool fns take `&App` and have no App-fixture harness, so the logic is tested at the layer below and the wiring is verified by build + grep.
- **D3 / D2 — per their own loops** (D3: goal-based, behavior change in `fetch_rels`; D2: TDD on the `LexicalSearch` trait + goal-based on the MCP wiring).

## Acceptance Criteria

D1 — bounded traversal (this loop):

- [x] `symbol_context` default depth is 1; `change_impact` default depth is 2.
- [x] Bounded traversal caps visited nodes at 64 **per direction**; a `truncated` flag is surfaced by both `symbol_context` and `change_impact`.
- [x] At the new defaults, an ordinary symbol's neighborhood is **not** truncated; a hub neighborhood at raised depth reports `truncated: true`.
- [x] Existing `ancestors` / `descendants` / `symbol_context` / `blast_radius` public signatures are unchanged (additive only); the N-API binding builds unchanged.
- [x] `symbol_context` and `change_impact` accept a `cap` arg (default 64) so a caller can widen the bound.

D3 — honest `fetch_rels` (subsequent loop):

- [ ] `fetch_rels` propagates store errors and unwired-capability as `ToolError` in the five pure graph tools; `get_context` degrades with a `(graph unavailable: …)` note.

D2 — BM25 `search` (subsequent loop):

- [ ] MCP `search` returns ranked symbol hits for multi-term queries via the additive `LexicalSearch` facade trait; the whole-string `.contains()` loop is removed.

## Assumptions

- Technical: `engram-graph-analytics` is pure, dependency-free, generic over `N` — bounded variants must stay generic with no domain types (AGENTS.md:151; `core/graph-analytics/src/reachability.rs:29,68`).
- Technical: `ancestors`/`descendants` are `pub fn -> HashSet<N>`; `symbol_context`/`blast_radius` are pub; `blast_radius` returns `HashSet<String>` consumed by the N-API binding (`codegraph/queries/src/queries.rs:62,132`; `bindings/node/src/codegraph.rs:50`).
- Technical: MCP depth defaults are `symbol_context=2` (`mcp/engram-mcp/src/codegraph.rs:205`), `change_impact=3` (`:214`); `fetch_rels` (`:162-168`) swallows errors and feeds 6 tools.
- Technical: gates are `cargo fmt --all` / `cargo check --workspace` / `cargo test` (AGENTS.md §Validation).
- Process: full mode — public-API additions to published crates. Constrained by RFC-0018 (Accepted) + ADR-0022 (binding unchanged).
- Process: `codegraph/queries` may depend only on `engram-domain` / `engram-graph-analytics` (AGENTS.md:156).
- Product: visited cap = 64 per direction (calibrated: depth-1 `recall` = 38 < 64); this loop implements D1 only, order D1 → D3 → D2 (user confirmation 2026-08-01).
