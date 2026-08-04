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

### Phase 3 — community-aggregate port (retire the viz's USED node:sqlite) + MCP reads

The viz's runtime `node:sqlite` is the **community aggregation** (`relationEdges` meta-edge
stream + `getMemberIndex` member index + member hydration) + `countTable` stats + `entityDetail`
degree + `/neighbors`. P3 ports the centerpiece — the community aggregate — into Rust.

**New engine-neutral port** `CommunityOverview` (engram-integration; types in engram-domain):
- `overview(scope, limit) -> CommunityOverview { communities: Vec<CommunityMetaNode>, edges: Vec<CommunityMetaEdge>, total: usize }` — top-N Louvain communities + inter-community meta-edges (the meta-edge tally moves from the viz's `relationEdges` stream into Rust).
- `members(scope, label, after, limit) -> Page<KnowledgeEntity>` — a community's member entities, paged (replaces `communityMembers`).
- `community_of(scope, entity_id) -> Option<u32>` — the label for an entity (replaces `entityCommunity`).
- **Layout stays in TS** (`layoutGraph` — a view concern); the port returns data only.

**Domain types:** `CommunityMetaNode { label, member_count }`, `CommunityMetaEdge { source_label, target_label, weight }`.

**SQLite impl** (engram-store-sqlite / knowledge adapter): reuses Louvain (`call_communities`) +
tallies meta-edges from `knowledge_relationships` (streamed, capped) + builds the member index
(label → entity ids, capped). The logic moved verbatim from the viz's `aggregation/{communities,members}.ts`.

**Open design question (resolve at P3.0):** Louvain (`call_communities`) today lives on the
*flat* `NativeKnowledgeEngine` (N-API), not on a Rust trait the facade/adapter can call. The
port impl needs Louvain reachable from the adapter layer — so P3.0 promotes `call_communities`
to a `KnowledgeGraphRepository` (or community) trait method backed by the SQLite adapter, then
the aggregate impl uses it. (If Louvain is already reachable Rust-side, P3.0 is a no-op spike-verify.)

**Tasks:**
- P3.0 — Louvain reachability: promote/verify `call_communities` behind a Rust trait the adapter calls.
- P3.1 — domain types `CommunityMetaNode`/`CommunityMetaEdge` + the `CommunityOverview` port (default `Unsupported`).
- P3.2 — SQLite impl (Louvain + meta-edge tally + member index) + tests.
- P3.3 — facade → `bindings/node` (`overviewJson`, `communityMembersJson`, `communityOfJson`) → `@engram/node`.
- P3.4 — rewire the viz (`computeOverview`, `communityMembers`, `entityCommunity`, the `/neighbors` + degree + stats reads follow under P3.5 scope-counts/neighbors ports).
- P3.5 — scope-counts port (retires `countTable`) + paged-neighbors-by-id port (retires `/neighbors`); remove `node:sqlite` from the viz backend.
- P3.6 — friendly MCP read tools on the existing `engram-mcp-http`, consuming the facade.

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
