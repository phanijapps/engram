# Spec — engram-mcp P2 Belief + Hierarchy (RFC-0016, Layers 4+5)

- **Status:** Implemented (P2 complete 2026-07-29; verified on live agentzero data)
- **Mode:** full (new deps + new module + new public tool surface)
- **Constrained by:** RFC-0016 D1 (surface the `beliefs` + `hierarchy` provider handles), ADR-0022 (surface parity — also reflect in `CapabilityReport`)
- **Branch:** `feat/engram-mcp-core`

## Objective

Surface the `beliefs` (Layer 5) and `hierarchy` (Layer 4) provider handles as MCP
tools, the next Zbot-parity layers. Beliefs are the system's current stance over
evidence (bi-temporal, lifecycle-managed); hierarchy is the navigation/context-
compression path over clustered structure.

## API grounding (verified)

- `BeliefRepository` (`core/belief`): `upsert_belief`, `get_belief(BeliefQuery)`, `retract_belief(id,scope,at)`, `list_stale(scope)`. No `list_beliefs` — reads are query-based.
- `BeliefQuery::live_subject(scope, key, as_of)` — the live-belief lookup constructor.
- `HierarchyRepository` (`core/hierarchy`): `path_for(seed_ids, scope, max_layer) -> HierarchyPath`. No builder handle on the provider — `path_for` returns empty until a hierarchy is built (consolidation/external).
- Port traits are NOT re-exported by `engram-integration`; the MCP gains `engram-belief` + `engram-hierarchy` deps.

## Tools

1. **`belief_get`** `{subject, as_of?}` — the live belief for a subject (valid at `as_of`, default now). The "what do we believe about X?" read.
2. **`belief_put`** `{subject, statement, confidence?}` — assert/upsert a manual belief (new valid-time version).
3. **`belief_retract`** `{id}` — close a belief's valid interval (retract).
4. **`belief_stale_list`** `{}` — beliefs flagged stale (need review).
5. **`hierarchy_path`** `{seeds, max_layer?}` — navigation path (LCA + nodes/relations) for seed entity ids.

## Boundaries

**Always do** — route through the `beliefs`/`hierarchy` handles (no engine-store bypass);
additive (5 new tools, existing 23 untouched); reflect `beliefs` in `capability_report`.
**Never do** — no `core/domain` change; no belief *synthesis* or contradiction
resolution in v1 (deferred); no hierarchy *build* (no builder handle; build via
consolidation/external); no server-side LLM.

## Testing Strategy

TDD: `belief_put` then `belief_get` round-trips; `belief_stale_list`/`belief_retract`
behave on a seeded belief. `hierarchy_path` goal-based (returns a `HierarchyPath`,
possibly empty pre-build).

## Acceptance Criteria

1. `belief_put` + `belief_get` round-trip: assert "X is A", then `belief_get("X")` returns it.
2. `belief_retract(id)` then `belief_get` no longer returns it as live.
3. `belief_stale_list` returns beliefs marked stale.
4. `hierarchy_path` returns a `HierarchyPath` (nodes/relations; empty acceptable pre-build).
5. `tools/list` exposes the 5 new tools (28 total); `capability_report` now lists `beliefs`.
6. Gates: `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp`,
   `check-engine-neutrality.sh`, `check-docs.sh` all pass.

## Tasks

- **B1** Cargo.toml: add `engram-belief` + `engram-hierarchy` deps. · goal-based · none.
- **B2** `belief.rs`: `belief_get` + `belief_put` + `belief_retract` + `belief_stale_list`. · TDD · B1.
- **B3** `hierarchy.rs`: `hierarchy_path`. Add `beliefs` to `capability_report`. · goal-based · B1.
- **B4** Register 5 tools in `main.rs`. · goal-based · B2,B3.
- **B5** Gates + commit + deploy. · B1–B4.
