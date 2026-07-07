# Spec: kg-redesign (knowledge graph as the centerpiece)

- **Status:** Draft
- **Shape:** ui
- **Constrained by:** demo-ui-shell; existing `/knowledge/overview` + `/explorer`
- **Contract:** none

## Objective

The knowledge graph becomes the **centerpiece** of the demo — taking 70%+ of the
viewport real estate, visually stunning ("living memory" feel), performant on
12K+ entities. The current `/explorer` route is promoted to the default `/` route.
Clicking a node opens a detail panel (properties, relationships, source file,
provenance, last updated). The graph supports virtualization (render only visible
nodes) for performance.

## Decision

Replace the 2D force-graph canvas with a WebGL-based renderer (`three-forcegraph`
or `react-force-graph-3d`) for the main view — handles 10K+ nodes smoothly. The
sidebar + command palette remain. Indexing becomes a popup/modal (see
`index-popup` spec) rather than a dedicated route. The dashboard moves to a
side panel or separate route.

## Acceptance Criteria

- [ ] `/` renders a WebGL force-directed graph filling 70%+ of the viewport.
- [ ] Clicking a node opens a detail panel (name, kind, source file, relationships, provenance).
- [ ] Graph renders 12K entities at 30+ FPS (virtualized/culled nodes).
- [ ] Color-coded by entity kind; edges colored by predicate type.
- [ ] Search + filter by kind/source/predicate.
