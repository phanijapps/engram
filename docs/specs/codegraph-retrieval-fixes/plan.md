# Plan: codegraph-retrieval-fixes

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Done -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Three independent slices ship as three loops. **This loop = D1 (bounded
traversal).** D1 layers bottom-up: (1) add additive bounded BFS variants to the
pure `engram-graph-analytics` crate; (2) add additive bounded query wrappers to
`engram-codegraph-queries`; (3) wire the two MCP tools (`symbol_context`,
`change_impact`) to the bounded wrappers and lower their depth defaults. TDD for
the two logic layers (the bound's correctness is a compressible invariant); a
goal-based check for the MCP wiring (the tool fns take `&App` and have no
App-fixture harness). Riskiest part: getting the bounded variants' semantics
exactly right — per-direction cap, `truncated` only when the cap actually bites,
depth still respected.

D3 (honest `fetch_rels`) and D2 (`LexicalSearch` trait + BM25 `search`) are
sketched as T4 / T5-T6 below for sequencing; they are filled out in their own
loops.

## Constraints

- RFC-0018 (Accepted) — the design authority; additive-only, per-direction cap, surfaced `truncated`, binding unchanged.
- AGENTS.md:151 — `engram-graph-analytics` stays pure / generic / dependency-free.
- AGENTS.md:156 — `codegraph/queries` depends only on `engram-domain` / `engram-graph-analytics`.
- ADR-0022 — N-API binding surface unchanged (composite parity deferred).

## Construction tests

Per-task `Tests:` below hold the unit/edge tests. Cross-cutting:

**Integration tests:** none beyond per-task — D1 is pure logic + thin wiring; the graph-analytics and codegraph-queries unit tests are the regression net, and `cargo test --workspace` is the integration gate.
**Manual verification:** after T3, confirm `symbol_context`/`change_impact` output is bounded and prints `truncated` (goal-based, via build + the live MCP if available).

## Design (LLD)

### Design decisions

- **Per-direction cap, not shared.** `symbol_context` calls ancestors and descendants separately; capping each independently means a hub-heavy callees side doesn't starve the callers side. Traces to: AC "caps visited nodes at 64 per direction".
- **Additive siblings, not modified originals.** New `*_bounded` fns + wrapper structs (`SymbolContextBounded`, `BlastRadiusBounded`); originals untouched so the N-API binding (`cgq::blast_radius`) and existing callers are unaffected. Traces to: AC "public signatures unchanged".
- **`truncated` lives on the wrapper, not the original `SymbolContext`.** Keeps the existing struct's shape stable. Traces to: AC "truncated flag surfaced".

### Component / module decomposition

- `engram-graph-analytics::reachability` — gains `ancestors_bounded` / `descendants_bounded` (generic, `(HashSet<N>, bool)`).
- `engram-codegraph-queries` — gains `symbol_context_bounded` / `blast_radius_bounded` + `SymbolContextBounded` / `BlastRadiusBounded`.
- `mcp/engram-mcp/src/codegraph.rs` — `symbol_context` / `change_impact` tools call the bounded wrappers; depth defaults lowered; `truncated` printed.

### Behavior & rules

- The bounded BFS stops enqueuing once `found.len() >= max_visited`. `truncated` is **true iff the BFS exited because the visited cap was reached before the queue drained naturally** — computable as `truncated = cap_reached && !queue_was_empty_at_exit`, where `cap_reached` fires when `found.len()` hit `max_visited`. When the natural reachable set is at or under the cap, the queue drains and `truncated` is false. Depth (`max_depth`) is the outer bound; the visited cap is the inner safety net.

## Tasks

### T1: Add bounded BFS variants to engram-graph-analytics

**Depends on:** none

**Tests:**
- `ancestors_bounded` / `descendants_bounded` return the full set with `truncated: false` when the reachable set is under the cap.
- They stop at the cap and return `truncated: true` when the reachable set exceeds it (construct a chain/fan that exceeds 64, assert len == cap and truncated == true).
- **Exact boundary:** when the natural reachable set equals exactly `max_visited`, the queue drains naturally and `truncated` is `false` — pins the invariant so an implementer can't satisfy the suite with the wrong `found.len() >= max_visited`.
- The cap is independent per direction (a graph where ancestors exceed the cap but descendants do not → ancestors truncated, descendants not).
- `max_depth` is still respected (cap does not extend reach beyond depth).
- Existing `ancestors` / `descendants` tests still pass unchanged (signatures untouched).

**Approach:**
- In `core/graph-analytics/src/reachability.rs`, add `ancestors_bounded(edges, target, max_depth, max_visited) -> (HashSet<N>, bool)` and `descendants_bounded(...)` mirroring the existing BFS, adding an early-exit when `found.len() >= max_visited` and computing `truncated` (whether more would have been enqueued).
- Keep `ancestors` / `descendants` exactly as-is.
- Add unit tests alongside the existing `#[cfg(test)] mod tests`.

**Done when:** `cargo test -p engram-graph-analytics` green; new tests pass; existing tests unchanged.

### T2: Add bounded query wrappers to engram-codegraph-queries

**Depends on:** T1

**Tests:**
- `symbol_context_bounded(rels, symbol, depth, cap)` returns a `SymbolContextBounded { ctx: SymbolContext, truncated: bool }` whose `truncated` is the OR of the ancestors and descendants truncation, per-direction.
- `blast_radius_bounded(rels, target, depth, cap)` returns `BlastRadiusBounded { callers, truncated }` with `callers` sorted for determinism (mirrors `SymbolContext.callers`).
- Existing `symbol_context` / `blast_radius` tests still pass unchanged.

**Approach:**
- In `codegraph/queries/src/queries.rs`, define `SymbolContextBounded { ctx: SymbolContext, truncated: bool }` and `BlastRadiusBounded { callers: Vec<String>, truncated: bool }` (derives `Debug, Clone, PartialEq, serde::Serialize`).
- `symbol_context_bounded` calls `ancestors_bounded` and `descendants_bounded` (cap per direction) and sets `truncated = anc_truncated || desc_truncated`.
- `blast_radius_bounded` calls `ancestors_bounded`.
- Leave `symbol_context` / `blast_radius` untouched.

**Done when:** `cargo test -p engram-codegraph-queries` green.

### T3: Wire MCP symbol_context / change_impact to bounded variants + lower depth defaults

**Depends on:** T2

**Tests:** (goal-based)
- `cargo check --workspace` and `cargo test --workspace` green.
- `grep -n 'unwrap_or(1)' mcp/engram-mcp/src/codegraph.rs` matches the `symbol_context` body; `unwrap_or(2)` matches the `change_impact` body (depth defaults 2→1, 3→2).
- `grep -n '_bounded' mcp/engram-mcp/src/codegraph.rs` matches inside both the `symbol_context` and `change_impact` tool bodies (the bounded variants were actually wired, not the unbounded originals). `truncated` surfaces at runtime via the wrapper structs' Debug output, not as a literal token in source.
- `grep` for the `cap` arg name confirms both tools accept it (default 64).

**Approach:**
- In `mcp/engram-mcp/src/codegraph.rs`: `symbol_context` and `change_impact` call `symbol_context_bounded` / `blast_radius_bounded` with the per-direction cap (default 64) and print `truncated`.
- Change `args["depth"].as_u64().unwrap_or(2)` → `unwrap_or(1)` for `symbol_context` (`:205`); `unwrap_or(3)` → `unwrap_or(2)` for `change_impact` (`:214`).
- Surface the cap as an explicit arg so a caller can widen it.

**Done when:** workspace builds + tests green; depth defaults lowered; output prints `truncated`.

### T4: (Subsequent loop — D3) Honest fetch_rels

**Depends on:** none (independent of D1)

Make `fetch_rels` fallible; propagate `ToolError` in the five pure graph tools (`symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed`); degrade with a note in `get_context`. Distinguishes store-error from unwired-capability. Goal-based.

### T5-T6: (Subsequent loop — D2) LexicalSearch trait + BM25 search

**Depends on:** none (independent of D1)

Add additive `LexicalSearch` trait to `engram-integration` + SQLite impl over the shared index + inherent method on `EngramProvider`; route MCP `search` through it with entity-id resolution; remove the whole-string `.contains()` loop. TDD on the trait; goal-based on the wiring.

## Rollout

Pure-logic + thin wiring. Big-bang, fully reversible (behavior-only + additive API; revert restores prior depth defaults and flood). No schema migration, no infra, no external-system dependency.

## Risks

- **Depth-default change is user-visible.** Callers get smaller neighborhoods by default (intended — the prior default flooded). Mitigation: the cap is an explicit arg; `truncated` signals when a caller should widen.
- **`truncated` semantics off-by-one.** The flag must mean "the cap prevented exploration," not "the set is large." Mitigation: T1 tests assert `truncated == true` iff the BFS was cut off mid-exploration.

## Changelog

- 2026-08-01: initial plan — D1 (bounded traversal) as this loop; D3/D2 sketched as subsequent loops per RFC-0018.
