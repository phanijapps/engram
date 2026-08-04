# Plan: viz-memory

- **Spec:** [`spec.md`](spec.md)
- **Status:** Draft

> **Plan contract:** implementation strategy for the Memory tab. Builds on
> `viz-foundation` (prerequisite).

## Approach

Extend the BFF with keyset-paginated memory/belief/contradiction/procedure
endpoints (memory provider reads + belief transport), then build the zbot
command-deck Memory UI (three-column deck + sub-tabs + debounced hybrid search)
with honest empty-states. Order: (1) backend endpoints + contract delta;
(2) Memory tab UI + sub-tabs; (3) search + empty-states; (4) Playwright E2E.

## Constraints

- ADR-0003, ADR-0008, ADR-0022, `reference.md` (in-process transport; fail-closed).
- `docs/CONVENTIONS.md` §4 (contract header), §6 (full work-loop).

## Construction tests

- **Unit (TDD):** record → card projection; empty-table → empty-state; sub-tab
  state; search debounce.
- **Integration:** memory/belief/contradiction/procedure endpoints conform to the
  extended OpenAPI; keyset + caps; empty tables return the empty-state shape; 503.
- **E2E (Playwright):** sub-tabs switch; search filters; empty-states render for
  unpopulated surfaces.

## Design (LLD)

### Design decisions

- List via **keyset + capped** BFF endpoints over in-memory binding reads; empty
  tables return a typed empty-state shape (not an error). *(extends
  `contracts/openapi/engram-viz-bff.yaml`.)*
- Port zbot's Memory command-deck layout (`.memory-tab-deck__grid` 220/1fr/230,
  `.memory-deck` sub-tabs) into the React 19 + Tailwind v4 shell.
- Hybrid search reuses the provider recall API (debounced, keyset).

### Interfaces & contracts

Extends `contracts/openapi/engram-viz-bff.yaml`:

- `GET /api/memory?cursor=&limit=&kind=` → keyset-paged memories.
- `GET /api/beliefs?cursor=&limit=` / `GET /api/contradictions?cursor=&limit=` →
  belief/contradiction lists (empty-state shape when 0 rows).
- `GET /api/procedures?cursor=&limit=` → procedures (via `node:sqlite`).
- `POST /api/memory/search` → hybrid recall `ContextPayload` (no keyset cursor).

The four list endpoints are keyset + capped + declare `503`/`Error`/`422 BadCursor`;
search returns the `ContextPayload`.

### Component / module decomposition

- **backend:** `src/routes/memory.ts`, `src/engram/belief.ts` (wrap
  `listBeliefs`/`listContradictions`).
- **frontend:** `src/features/memory/MemoryTab.tsx` (deck + sub-tabs),
  `MemoryItemCard.tsx`, `SearchBar.tsx`, `EmptyState.tsx`.

### Behavior & rules

Read-only. Empty tables → typed empty-state (not 404/error). Search debounced.

### Failure, edge cases & resilience

Degraded store → 503 + `Error`. Empty table → empty-state with populate action.

### Quality attributes (NFRs)

- Operability: honest empty-states; degraded-mode surfaced.
- Extensibility: write paths (populate) slot in later behind the seam.

## Tasks

### T1: Memory/belief/procedure endpoints + contract delta

**Depends on:** viz-foundation T5

**Tests:**
- Integration: the four list endpoints + search conform to the extended OpenAPI;
  keyset + caps; empty tables return empty-state; 503+Error. *(list AC)*

**Approach:**
- `src/routes/memory.ts`; beliefs/contradictions via `createNativeBeliefTransport`;
  memories + procedures via read-only `node:sqlite` (no provider list/procedures
  transport); search returns the recall `ContextPayload`; extend the OpenAPI with
  `503`/`Error` + `422 BadCursor` on keyset endpoints (authored directly).

**Done when:** integration tests green; contract extended.

### T2: Memory tab UI (command deck + sub-tabs)

**Depends on:** T1, viz-foundation T7

**Tests:** Visual/manual QA — deck + sub-tabs render zbot-styled. *(shell AC)*

**Approach:**
- `src/features/memory/MemoryTab.tsx` + cards; port zbot command-deck styling.

**Done when:** Memory tab renders against real data.

### T3: Hybrid search + empty-states

**Depends on:** T2

**Tests:**
- Unit (TDD): empty-table → empty-state; search debounce/filter.

**Approach:**
- `SearchBar.tsx` (debounced recall); `EmptyState.tsx` with populate action.

**Done when:** unit tests green; empty-states render for belief/procedure.

### T4: E2E (Playwright)

**Depends on:** T3

**Tests:** E2E — sub-tabs switch; search filters; empty-states render. *(all ACs)*

**Done when:** E2E green.

## Rollout

- **Delivery:** adds the Memory tab to the shipped shell; reversible via git.
  Read-only.
- **Deployment sequencing:** after `viz-foundation`; backend T1 before T2–T3.

## Risks

- Memory provider read surface for listing all memories may be narrow (recall is
  query-shaped) → fall back to read-only `node:sqlite` keyset over `memories` if
  no list API (mirrors `viz-foundation`'s secondary-path Boundary).

## Changelog

- 2026-08-03: initial plan.
