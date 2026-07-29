# Spec — engram-mcp P1 Graph Tools (RFC-0016, Layer 2)

- **Status:** Implementing
- **Mode:** full (new module + new public tool surface)
- **Constrained by:** RFC-0016 D1 (surface, not build — route through `KnowledgeQuery`), ADR-0022 (surface parity)
- **Branch:** `feat/engram-mcp-core`

## Objective

Make the unified knowledge graph (code + concepts, now bridged by P0's `describes`
edges) **traversable through the MCP surface**. Add general graph-traversal tools that
work over *any* entity kind — a concept, a function, a class — unlike the existing
codegraph composites which are code-specific. Built on existing `KnowledgeQuery` reads
(`list_entities` / `list_relationships`); no new provider handle.

## Tools

1. **`graph_neighbors`** `{name, limit?}` — every entity directly connected to `name`
   and the edges between them (bidirectional). e.g. a concept `describes` a function, or
   a function `calls` another.
2. **`graph_subgraph`** `{name, depth?, limit?}` — a breadth-first subgraph around `name`
   up to `depth` hops (default 2), edges labelled with their natural direction. This is
   the query that makes doc↔code connections explorable.
3. **`resolve_entity`** `{name}` — resolve a name to its entity (exact, else first
   substring) + kind, id, source count, aliases. The "is X in the graph?" lookup.

## Boundaries

**Always do** — route through `KnowledgeQuery` (no new handle, no engine-store bypass);
additive (new tools only, existing 20 untouched); general over entity kinds.
**Never do** — no `core/domain` change; no persisted writes (read-only traversal); no
code-specific assumptions (works for concepts too).

## Testing Strategy

TDD: seed a fixture via `scan_repo` (code + a markdown section sharing a name → a live
`describes` edge), then assert `graph_neighbors` / `graph_subgraph` / `resolve_entity`
return the bridged concept↔code link. Goal-based: registration (`tools/list` exposes the
3 new tools).

## Acceptance Criteria

1. `graph_neighbors("flange")` on the P0 fixture returns the concept↔function `describes`
   edge (and any `calls`/`mentions`).
2. `graph_subgraph("flange", depth=2)` returns a non-empty edge set including the
   `describes` bridge.
3. `resolve_entity("flange")` resolves to the entity(ies) named `flange` with kind + id.
4. `tools/list` exposes `graph_neighbors`, `graph_subgraph`, `resolve_entity` (23 tools total).
5. `capability_report` unaffected; existing 20 tools unchanged.
6. Gates: `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp`,
   `check-engine-neutrality.sh`, `check-docs.sh` all pass.

## Tasks

- **G1** `graph.rs` module: `fetch_entities` + `graph_neighbors` + `graph_subgraph`
  (bidirectional BFS, natural-direction edge labels) + `resolve_entity`. Make
  `codegraph::fetch_rels` `pub(crate)`. · TDD · Depends: none.
- **G2** Register the 3 tools in `main.rs` (`mod graph;` + `register_all`). · goal-based ·
  Depends: G1.
- **G3** Gates + commit. · Depends: G1, G2.
