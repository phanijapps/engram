# Spec: ts-integration-read-facade

- **Status:** Draft
- **Owner:** core
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003 (implementation stack), ADR-0022 (engine neutrality + surface parity), RFC-0017 (TS runtime facade), [`docs/architecture/reference.md`](../../architecture/reference.md)
- **Brief:** [`docs/product/briefs/engram-viz-overhaul.md`](../../product/briefs/engram-viz-overhaul.md) (read-path debt), [`docs/specs/viz-foundation/spec.md`](../viz-foundation/spec.md) (node:sqlite Boundary)
- **Shape:** mixed (rust-core-led)

> **Spec contract:** move engram's read/query surface — paged list + counts — out of
> TypeScript (`node:sqlite` in the viz BFF) and into `engram-integration` (the ports),
> implemented in the SQLite adapters, exposed through `bindings/node` + `@engram/node`.
> Result: a fully Rust-backed TS read/query facade; `node:sqlite` leaves the TS layer;
> the friendly MCP read tools (and the viz) consume the facade. The implementing PR
> must match this spec, or update it.

## Objective

Today the read/query the UI/agent needs (paginated lists, counts, aggregates) is **not
in the integration facade**: `KnowledgeQuery.list_*` and `MemoryService` are firehose /
absent (no `list_memories`), and `bindings/node` exposes no paged/count surface. So the
viz BFF fell back to read-only `node:sqlite` (`COUNT`/keyset/`GROUP BY`) — adapter SQL
that leaked into TypeScript, against ADR-0022 (SQL belongs in adapter crates).

This slice moves that read/query surface to its correct home: an engine-neutral **paged
port** + **counts**, implemented in the SQLite adapter, surfaced via `engram-integration`
→ `bindings/node` → `@engram/node`. The viz (and the MCP) then consume the facade;
`node:sqlite` is retired from TS. Phase 1 is a **vertical spike** of one port (`list_memories_paged`)
end-to-end to prove the pattern; Phase 2 fans out; Phase 3 retires `node:sqlite` + wires MCP reads.

Success: a TS consumer can list memories (paged) + read counts via `@engram/node`, backed
entirely by Rust, with the viz's `/memory` route no longer touching `node:sqlite`.

## Boundaries

### Always do

- Define an **engine-neutral** paged primitive (`Page<T>` + opaque `Cursor`) in
  `engram-domain` (serde, version-marked). No SQL, no engine name, no `rowid` in the type.
- Add paged port methods returning `Page<T>`; implement them in the SQLite adapter(s)
  with keyset SQL (the logic the viz currently runs in `node:sqlite`, moved into Rust).
- Expose every new port method through **both** `engram-integration` (the facade) **and**
  `bindings/node` (N-API) **and** `@engram/node` (TS) — surface parity (ADR-0022). Reflect
  them in `CapabilityReport` where they constitute a capability.
- Update **every** port impl (SQLite adapter, the inmem reference impl, fixtures/stubs) so
  the trait change compiles across the workspace.

### Ask first

- Any non-`Cursor` pagination model (offset vs keyset trade-off) — default keyset.
- Exposing the firehose `list_*` as deprecated vs removed.

### Never do

- Put SQL, `rowid`, or an engine name in `engram-domain`, the `core/*` ports,
  `engram-integration`, or `bindings/node` (the neutrality lint gates this). *(Structural.)*
- Leave a port method un-wired on one surface (facade / N-API / TS). *(Structural — parity.)*
- Add `node:sqlite` read paths to TS to "finish" a port. *(Structural — the slice's purpose.)*

## Testing Strategy

- **Port contract (TDD):** a `Page<T>` paged contract — disjoint pages across a cursor,
  the cap enforced, cursor opaque + round-trippable; keyset monotonicity.
- **SQLite impl:** `list_memories_paged` returns disjoint keyset pages over a fixture store;
  scope-filtered; `next_cursor` is null at the end.
- **Binding smoke:** `bindings/node` `listMemoriesPagedJson` round-trips a page (Node side).
- **Integration (viz):** the `/api/memory` route (rewired to the facade) returns the same
  disjoint paged shape it did via `node:sqlite` (regression).
- **Parity/neutrality:** the engine-neutrality + surface-parity lints pass; `cargo check --workspace` green.

## Acceptance Criteria

- [ ] `engram-domain` exposes `Page<T>` + opaque `Cursor`; no SQL/engine types leak.
- [ ] `MemoryService` (or `MemoryRepository`) exposes `list_memories_paged(scope, after,
  limit) -> Page<MemoryRecord>`; implemented in the SQLite adapter + inmem; contract-tested.
- [ ] The method reaches `engram-integration` → `bindings/node` (`listMemoriesPagedJson`) →
  `@engram/node` (`listMemoriesPaged`) — surface parity.
- [ ] The viz `/api/memory` route is rewired to `@engram/node` and no longer uses `node:sqlite`
  for memories; the paged shape is unchanged (regression-green). *(Phase 1 done.)*
- [ ] (Phase 2) paged reads + counts for entities/relationships/beliefs/hierarchy/chunks reach the facade + TS.
- [ ] (Phase 3) `node:sqlite` is removed from the viz backend; the friendly MCP read tools
  consume the facade.
- [ ] `cargo check --workspace` + the neutrality/parity lints + `pnpm typecheck` are green.

## Assumptions

- The held `NativeProvider` (`bindings/node`) already reaches `MemoryService` (it does —
  `require_memory()`), so the facade can delegate; only the paged method + its N-API/TS
  exposure are new.
- Keyset pagination over the SQLite `rowid` (the viz's existing approach) is the impl; the
  port stays neutral by making `Cursor` opaque.
