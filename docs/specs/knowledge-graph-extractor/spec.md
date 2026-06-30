# Spec: Deterministic knowledge-graph extractor (demo Slice 2)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0003, ADR-0007, `docs/specs/knowledge-ingestion`
- **Contract:** none
- **Shape:** service

## Objective

Ingestion produces chunks but no graph, so "visualize the knowledge graph" had
nothing to show. This slice adds a deterministic `GraphExtractor` in
`engram-ingest` that turns ingested chunks into `KnowledgeEntity` +
`KnowledgeRelationship` records (code symbols → `Function`/`Class` entities with
`calls` edges from name co-occurrence; prose → `Concept` entities with `mentions`
edges), persists them through the knowledge ports, and surfaces an ingest+extract
pass over the binding so the demo can paste code/text and see a real graph
rendered in Cytoscape.

## Boundaries

### Always do
- Stay deterministic — no model calls, no AST dependency.
- Reuse `CodeSymbolChunker` declaration anchors for code entities.
- Persist via `KnowledgeRepository` + `KnowledgeGraphRepository` ports only.

### Ask first
- Adding `serde` to `DocumentIngestRequest`/`DocumentMetadata` so the binding can
  decode ingest requests (pre-authorized by RFC-0003 Slice 2).

### Never do
- Use an LLM or tree-sitter for extraction (a later model-backed extractor sits
  behind the same ports).
- Change v1 contract fields or couple adapters.

## Testing Strategy
- **TDD/integration (Rust):** the extractor ingests a Rust snippet and asserts
  the `alpha → beta` `calls` edge + a real `neighbors` traversal.
- **Goal-based:** real-load ingest smoke over the binding; backend `/ingest/extract`
  integration test.

## Acceptance Criteria
- [x] `GraphExtractor` produces scoped entities + `calls`/`mentions` relationships
  from chunks and persists them (`extract_into`).
- [x] `NativeIngestEngine.ingestExtractJson` runs ingest + extract + persist and
  returns the graph.
- [x] `@engram/node` exposes a `NativeIngestTransport`; backend has
  `/ingest/extract`.
- [x] `demo/frontend` IngestPanel renders the extracted graph in Cytoscape.
- [x] `cargo fmt/clippy/test --workspace`, `pnpm typecheck/test`, isolation gate,
  and contract/docs hooks pass with no drift.

## Assumptions
- Technical: code-symbol anchors encode kind + name (`"fn remember"`) (source:
  `adapters/ingest/src/code_symbol.rs`).
- Process: adding `serde` to internal ingest request types is additive and does
  not touch v1 contracts (source: request.rs; user confirmation 2026-06-30).
