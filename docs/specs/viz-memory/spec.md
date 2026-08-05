# Spec: viz-memory

- **Status:** Shipped
- **Owner:** engram-viz
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003 (implementation stack), ADR-0008 (durable ontology/taxonomy), ADR-0022 (surface parity), [`docs/architecture/reference.md`](../../architecture/reference.md)
- **Brief:** [`docs/product/briefs/engram-viz-overhaul.md`](../../product/briefs/engram-viz-overhaul.md)
- **Contract:** [`contracts/openapi/engram-viz-bff.yaml`](../../../contracts/openapi/engram-viz-bff.yaml) *(extends — adds memory/belief/procedure endpoints)*
- **Shape:** mixed (ui-led)

> **Spec contract:** the Memory tab over engram's memory + belief + procedure
> surfaces. Library names live in `plan.md`; this body stays behavioral.

## Objective

The Memory tab mirrors zbot's command-deck layout (a three-column deck with
Facts / Beliefs / Contradictions / Procedures sub-tabs and hybrid search) over
engram's memory, belief, contradiction, and procedure records, served by
`viz-foundation`'s in-process BFF (memory provider reads + the belief transport),
keyset-paginated. Because engram's belief/contradiction/procedure tables are
empty today, the tab renders honest empty-states with a path to populate them —
never fabricating content. Success: a user browses and searches engram's memories
and sees clearly when a surface is unpopulated.

## Boundaries

### Always do

- List memories/beliefs/contradictions/procedures via **keyset-paginated** BFF
  endpoints over the in-process binding (memory reads + `createNativeBeliefTransport`).
- Render the zbot Memory command-deck layout + sub-tabs + search, ported from
  zbot's styling.
- Show honest empty-states when a table is empty (belief/procedure today), with
  the action that would populate it.

### Ask first

- Any new top-level dependency.
- Any **write** path (`write_memory`, `belief_put`, distillation) — this slice is
  read-only; populating empty surfaces is a separate, sign-off-gated action.

### Never do

- Ship an **unbounded list** payload — every memory/belief endpoint keysets + caps.
  *(Structural.)*
- Fabricate or placeholder-fill records when a table is empty — render the
  empty-state instead. *(Structural.)*
- Bypass the BFF or route browser data through engram-mcp. *(Structural.)*

## Testing Strategy

- **List shaping + empty-state logic — TDD** (record → card projection; empty-table
  → empty-state).
- **Memory/belief endpoints — goal-based + integration** against a fixture store:
  shapes + keyset + caps conform to the extended OpenAPI; empty tables return the
  empty-state shape, not an error.
- **Memory tab — visual / manual QA via a Playwright E2E**: sub-tabs switch,
  search filters, empty-states render for unpopulated surfaces.

## Acceptance Criteria

- [x] The Memory tab renders the zbot command-deck layout with Facts / Beliefs /
  Contradictions / Procedures sub-tabs, styled consistently with zbot.
- [x] Memory/belief/contradiction/procedure lists are keyset-paginated + capped via
  BFF endpoints, added to `contracts/openapi/engram-viz-bff.yaml` (with
  `503`/`Error` and `422 BadCursor`).
- [ ] Hybrid recall search filters memory results (debounced; returns the recall
  `ContextPayload`, not a keyset page). *(deferred: viz-memory-search)*
- [x] Empty engram tables (belief/contradiction/procedure today) render an honest
  empty-state with the populate action — no fabricated records.
- [x] All data flows through the `viz-foundation` in-process BFF (no engram-mcp,
  no browser store access).

## Assumptions

- Inherits `viz-foundation`'s verified stack/contract/assumptions.
- Technical: today the `agentzero` store has 24 memories and **0** beliefs /
  contradictions / procedures — Memory renders real facts + empty-states for the
  rest (source: read-only sqlite probe).
- Technical: `createNativeBeliefTransport` exposes `listBeliefs` /
  `listContradictions`; `NativeMemoryApi` has **no list** (only search/write/
  forget) and recall returns a one-shot `ContextPayload`, so `/api/memory` lists
  via read-only `node:sqlite` over `memories` and `/api/procedures` over
  `procedures` (secondary-path Boundary); search returns the `ContextPayload`
  without a keyset cursor (`packages/node/src/{transport,provider}.ts`;
  `bindings/node/src/provider.rs`).
- Product: local single-user, multi-user-ready seam inherited from S1.
