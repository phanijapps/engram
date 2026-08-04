# Plan: ts-integration-read-facade

- **Spec:** [`spec.md`](spec.md)
- **Status:** Draft

> **Plan contract:** the read/query surface moves into `engram-integration` (ports) +
> SQLite adapters, surfaced through `bindings/node` + `@engram/node`. Phase 1 is a
> vertical spike of `list_memories_paged` end-to-end; Phase 2 fans out; Phase 3 retires
> `node:sqlite` + wires MCP reads. Substantial changes land in the changelog.

## Approach

A vertical-slice-first reduction of risk: prove the full path (domain primitive → port →
SQLite impl → inmem impl → facade → N-API binding → TS → viz consumer rewired off
`node:sqlite`) on **memories** (the smallest read surface, 24 rows), then replicate the
pattern across entities/relationships/beliefs/hierarchy/chunks + counts, then retire
`node:sqlite` entirely + put the MCP read tools on the facade.

The port stays engine-neutral (`Page<T>` + opaque `Cursor`); the keyset SQL (the viz's
current `node:sqlite` logic) moves verbatim into the SQLite adapter — one canonical impl.

## Constraints

- ADR-0003, ADR-0022 (neutrality: no SQL/engine in domain/core/integration/binding;
  surface parity: facade ↔ N-API ↔ TS), RFC-0017, `reference.md`.
- `docs/CONVENTIONS.md` §4 (contract), §6 (full work-loop — public-interface + structural).

## Construction tests

- **Unit (TDD, engram-domain):** `Page<T>` + `Cursor` — opaque, round-trippable; cap
  enforced; a page + its `next_cursor` yields a disjoint next page.
- **Integration (SQLite adapter):** `list_memories_paged` disjoint keyset pages over a
  fixture; scope-filtered; terminal `next_cursor: None`.
- **Binding smoke:** `listMemoriesPagedJson` round-trips through Node.
- **Regression (viz):** `/api/memory` (rewired to the facade) returns the same paged shape.

## Design (LLD)

### Design decisions

- **Primitive in `engram-domain`:** `Page<T: Serialize>` (`{ items: Vec<T>, next_cursor:
  Option<Cursor> }`) + `Cursor` (an opaque `String` newtype; serde-transparent). The SQLite
  impl encodes `rowid` in it; the type carries no SQL/engine knowledge. *(neutrality AC.)*
- **Keyset over `rowid`** (the viz's proven approach) in the SQLite impl; `after: Option<&Cursor>`
  decodes to `rowid > ?`. The port signature is `list_memories_paged(&self, scope, after,
  limit) -> CoreResult<Page<MemoryRecord>>`.
- **Facade delegates** via `MemoryService` (already reachable through `NativeProvider`);
  only the paged method + its N-API/TS exposure are new. *(parity AC.)*
- **`Cursor` opacity** keeps the port neutral; a future pgvector adapter encodes its own.

### Interfaces

- `engram-domain`: `Page<T>`, `Cursor`.
- `engram-memory` (`MemoryRepository` or `MemoryService`): `list_memories_paged`.
- `engram-store-sqlite` (`adapters/sqlite/src/memory/`): impl (keyset).
- inmem reference impl + fixtures/stubs: impl (same trait).
- `engram-integration`: reaches it via `memory()` (no new facade method needed).
- `bindings/node` `NativeMemoryApi`: `list_memories_paged_json`.
- `@engram/node`: `NativeMemoryApiBinding.listMemoriesPagedJson` + TS `listMemoriesPaged`.
- viz `/api/memory`: rewired to `@engram/node`.

## Tasks

### Phase 1 — memories paged vertical spike

**P1.1 `engram-domain` paged primitive (TDD).** Depends on: none.
**P1.2 `MemoryService`/`MemoryRepository` `list_memories_paged` port method.** Depends on: P1.1.
**P1.3 SQLite adapter impl (keyset) + inmem impl + fixture/stub impls.** Depends on: P1.2.
**P1.4 `bindings/node` `listMemoriesPagedJson` + binding rebuild.** Depends on: P1.3.
**P1.5 `@engram/node` `listMemoriesPaged` (binding type + transport method).** Depends on: P1.4.
**P1.6 rewire viz `/api/memory` off `node:sqlite` → facade; regression-green.** Depends on: P1.5.
**P1.7 gates:** `cargo check --workspace`, neutrality + parity lints, `pnpm typecheck`,
viz memory test; commit. Depends on: P1.6.

### Phase 2 — fan out the read/query surface

Paged ports + adapter impls + binding + TS for: entities, relationships, chunks, beliefs,
procedures, hierarchy; + counts via `Observability.record_counts` (→ `countsJson` / TS `counts()`).

### Phase 3 — retire `node:sqlite` + MCP reads

Remove `node:sqlite` from the viz backend (paginate/countTable/aggregation → facade +
`call_communities` already exposed); add the friendly read tools to the existing
`engram-mcp-http` on the facade.

## Rollout

- **Delivery:** additive ports + impls first (no behavior change); then viz rewire
  (behavior-preserving); then `node:sqlite` removal. Reversible via git.
- **Deployment sequencing:** Phase 1 (memories) proves + ships the pattern; Phase 2/3 follow.

## Risks

- Trait method addition ripples to every `MemoryRepository` impl (SQLite, inmem, surreal,
  pgvector, fixtures, stubs) — must update all or the workspace won't compile. Mitigation:
  default impl (`unimplemented!`/`CoreError::Unsupported`) on the trait so non-SQLite impls
  can opt in incrementally without blocking the workspace.
- N-API binding rebuild is slow (full `cargo build -p engram-node`); sequence behind the
  Rust-core tasks so it builds once.
- `Cursor` opacity vs debuggability — keep a documented encoding (base64url of rowid) but
  treat it as opaque at the port.
