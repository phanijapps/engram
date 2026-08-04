# Spec: viz-observatory

- **Status:** Shipped
- **Owner:** engram-viz
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003 (implementation stack), ADR-0008 (durable ontology/taxonomy), ADR-0022 (surface parity), [`docs/architecture/reference.md`](../../architecture/reference.md)
- **Brief:** [`docs/product/briefs/engram-viz-overhaul.md`](../../product/briefs/engram-viz-overhaul.md)
- **Contract:** [`contracts/openapi/engram-viz-bff.yaml`](../../../contracts/openapi/engram-viz-bff.yaml) *(extends — adds belief-network/hierarchy stats)*
- **Shape:** mixed (ui-led)

> **Spec contract:** the Observatory tab over engram's graph/belief/hierarchy
> health. Library names live in `plan.md`; this body stays behavioral.

## Objective

The Observatory tab mirrors zbot's observatory layout (reusing `viz-foundation`'s deck.gl community-overview canvas — no new
rendering dependency — plus a LearningHealthBar that folds graph, belief-network,
and hierarchy stats,
with slideovers for belief-network and hierarchy detail) over engram's
`agentzero` store, served by `viz-foundation`'s in-process BFF. It surfaces
graph stats (entities/relationships/communities), belief-network stats, and
hierarchy stats, with honest empty-states for the surfaces engram has not
populated (belief and hierarchy tables are empty today). Populating those
surfaces is out of scope for the read-only viz (run via engram-mcp out of band).
Success: a user sees engram's graph health at a glance and clearly which
synthesis surfaces are unpopulated.

## Boundaries

### Always do

- Serve stats via the `viz-foundation` in-process BFF, extending it with
  belief-network and hierarchy stat endpoints.
- Render the zbot observatory layout (canvas + LearningHealthBar + slideovers),
  ported from zbot's styling.
- Show honest empty-states when a synthesis surface (belief/hierarchy) is
  unpopulated, with the out-of-band populate pointer.

### Ask first

- Any new top-level dependency.
- Any **write** path — this slice is read-only; hierarchy/belief population runs
  out of band via engram-mcp, not from the viz.

### Never do

- Claim health/activity for an unpopulated surface — render the empty-state.
  *(Structural.)*
- Bypass the BFF or route browser data through engram-mcp. *(Structural.)*
- Introduce a write path from the viz (hierarchy_build etc. stays out of band).
  *(Structural.)*

## Testing Strategy

- **Stat shaping + empty-state logic — TDD** (counts → health-bar segments;
  empty surface → empty-state).
- **Stat endpoints — goal-based + integration** against a fixture store: shapes
  conform to the extended OpenAPI; empty surfaces return the empty-state shape.
- **Observatory tab — visual / manual QA via a Playwright E2E**: health bar
  renders, slideovers open, empty-states show for unpopulated surfaces.

## Acceptance Criteria

- [x] The Observatory tab is the zbot-style graph command center — the SOLE graph
  view (the separate Graph tab was merged in): a toolbar (highlight search + top-N
  density + refresh) + the `viz-foundation` deck.gl overview canvas (no D3/new dep)
  with drill + a bottom LearningHealthBar whose belief/hierarchy items open detail
  slideovers (honest empty-states today, since those surfaces are unpopulated).
- [x] Graph, belief-network, and hierarchy stats are served via the extended
  `/api/graph/stats` (added `hierarchyNodes`/`hierarchyRelations`) in
  `contracts/openapi/engram-viz-bff.yaml` (with `503`/`Error`).
- [x] Unpopulated synthesis surfaces (belief/hierarchy today) render an honest
  empty-state with the out-of-band populate pointer — no fabricated activity.
- [x] All data flows through the `viz-foundation` in-process BFF (no engram-mcp,
  no browser store access, no viz write path).

## Assumptions

- Inherits `viz-foundation`'s verified stack/contract/assumptions.
- Technical: today the `agentzero` store has **0** beliefs / hierarchy nodes /
  relations — Observatory shows real graph stats + empty-states for belief/
  hierarchy (source: read-only sqlite probe).
- Technical: graph stats reuse `viz-foundation`'s `/api/graph/stats`;
  belief-network/hierarchy counts come from read-only `node:sqlite` `COUNT` over
  the belief/hierarchy tables (the binding lacks list-count helpers).
- Product: local single-user, multi-user-ready seam inherited from S1.

## Non-goals

- Populating engram's belief/hierarchy tables from the viz (read-only; population
  runs out of band via engram-mcp). Recorded as a follow-up, not an AC.
