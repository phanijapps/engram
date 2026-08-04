# Plan: viz-observatory

- **Spec:** [`spec.md`](spec.md)
- **Status:** Shipped

> **Plan contract:** implementation strategy for the Observatory tab. Builds on
> `viz-foundation` (prerequisite).

## Approach

Extend the BFF with belief-network and hierarchy stat endpoints (read-only
`node:sqlite` counts; the binding lacks list-counts), then build the zbot
observatory UI (canvas + LearningHealthBar + slideovers) with honest empty-states
for the unpopulated belief/hierarchy surfaces. Order: (1) stat endpoints +
contract delta; (2) Observatory tab UI + health bar; (3) slideovers + empty-states;
(4) Playwright E2E.

## Constraints

- ADR-0003, ADR-0008, ADR-0022, `reference.md` (in-process transport; fail-closed).
- `docs/CONVENTIONS.md` §4 (contract header), §6 (full work-loop).

## Construction tests

- **Unit (TDD):** counts → health-bar segments; empty-surface → empty-state.
- **Integration:** `/api/belief-network/stats`, `/api/hierarchy/stats` conform to
  the extended OpenAPI; empty surfaces return empty-state shape; 503.
- **E2E (Playwright):** health bar renders; slideovers open; empty-states show for
  belief/hierarchy.

## Design (LLD)

### Design decisions

- Stats via read-only `node:sqlite` `COUNT` over belief/hierarchy tables (reuses
  `viz-foundation`'s secondary-path Boundary); graph stats reuse
  `/api/graph/stats`. *(extends `contracts/openapi/engram-viz-bff.yaml`.)*
- Port zbot's observatory layout (`.observatory` flex column, `.observatory__health`
  LearningHealthBar, `Slideover` for detail) into the React 19 + Tailwind v4 shell.
- Empty-surface → typed empty-state (not error); populate pointer is informational.

### Interfaces & contracts

Extends `contracts/openapi/engram-viz-bff.yaml`:

- `GET /api/belief-network/stats` → `{ enabled, total_beliefs, total_contradictions,
  total_unresolved }` (all 0 today → empty-state).
- `GET /api/hierarchy/stats` → `{ enabled, layer_counts, inter_cluster_relations }`
  (empty today → empty-state).

Both declare `503` + `Error`.

### Component / module decomposition

- **backend:** `src/routes/observatory.ts` (stat endpoints); `src/db/reader.ts`
  (belief/hierarchy counts — reuse).
- **frontend:** `src/features/observatory/ObservatoryTab.tsx` (canvas + health
  bar), `LearningHealthBar.tsx`, `Slideover.tsx`, `EmptyState.tsx`.

### Behavior & rules

Read-only. Empty surfaces → typed empty-state with informational populate pointer.

### Failure, edge cases & resilience

Degraded store → 503 + `Error`. Empty surface → empty-state.

### Quality attributes (NFRs)

- Operability: honest empty-states; degraded-mode surfaced.
- Extensibility: population runs out of band (future binding method or engram-mcp).

## Tasks

### T1: Belief-network + hierarchy stat endpoints + contract delta

**Depends on:** viz-foundation T5

**Tests:**
- Integration: the two stat endpoints conform to the extended OpenAPI; empty
  surfaces return empty-state; 503+Error. *(stat AC)*

**Approach:**
- `src/routes/observatory.ts`; read-only `COUNT` over belief/hierarchy tables;
  extend the OpenAPI (authored directly).

**Done when:** integration tests green; contract extended.

### T2: Observatory tab UI + LearningHealthBar

**Depends on:** T1, viz-foundation T7

**Tests:** Visual/manual QA — canvas + health bar render zbot-styled. *(shell AC)*

**Approach:**
- `src/features/observatory/ObservatoryTab.tsx` + `LearningHealthBar.tsx`; reuse
  the `viz-foundation` deck.gl overview as the canvas (no D3); port zbot
  observatory styling.

**Done when:** Observatory tab renders graph stats from real data.

### T3: Slideovers + empty-states

**Depends on:** T2

**Tests:**
- Unit (TDD): empty-surface → empty-state; health-bar segments from counts.

**Approach:**
- `Slideover.tsx` (belief-network/hierarchy detail); `EmptyState.tsx` with
  out-of-band populate pointer.

**Done when:** unit tests green; empty-states render for belief/hierarchy.

### T4: E2E (Playwright)

**Depends on:** T3

**Tests:** E2E — health bar renders; slideovers open; empty-states show. *(all ACs)*

**Done when:** E2E green.

## Rollout

- **Delivery:** adds the Observatory tab to the shipped shell; reversible via git.
  Read-only.
- **Deployment sequencing:** after `viz-foundation`; backend T1 before T2–T3.

## Risks

- Belief/hierarchy stat shapes may not map cleanly to zbot's richer
  `BeliefNetworkStatsResponse`/`HierarchyStatsResponse` (zbot has worker stats,
  distillation status engram lacks) → project only the counts engram has; mark
  the rest as "not applicable" rather than fake them.

## Changelog

- 2026-08-03: initial plan.
