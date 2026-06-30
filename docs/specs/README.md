# Specs

This directory holds active spec-driven implementation slices. Each feature
directory owns a `spec.md` contract and a `plan.md` implementation strategy.

## Active

- [`napi-bridge-completion`](napi-bridge-completion/spec.md): demo Slice 0 — make
  the `engram-node` N-API binding loadable from Node and ship a Hono + Vite/React
  demo proving a browser→Node→Rust memory round-trip (RFC-0003, PHASE52). Shipped.
- [`sqlite-knowledge-graph`](sqlite-knowledge-graph/spec.md): demo Slice 1 —
  `engram-store-knowledge-sqlite` adapter + `TaxonomyRepository` port + forbidden-
  import gate (RFC-0003, PHASE53). Shipped.
- [`knowledge-graph-extractor`](knowledge-graph-extractor/spec.md): demo Slice 2 —
  deterministic `GraphExtractor` (code symbols + calls/mentions) + ingest+extract
  over the binding + Cytoscape graph panel (RFC-0003, PHASE54). Shipped.
- [`fastembed-passage-embeddings`](fastembed-passage-embeddings/spec.md): demo
  Slice 3 — feature-gated `NativeRetrievalEngine` (BGE-small passage + query
  embeddings over sqlite-vec) + `/retrieval/*` + SearchPanel (RFC-0003, PHASE55).
  Shipped.
- [`engram-demo-app`](engram-demo-app/spec.md): demo Slice 4 — durable shared
  SQLite across memory/knowledge/ingest engines + README (RFC-0003, PHASE56).
- [`workspace-responsibility-layout`](workspace-responsibility-layout/spec.md):
  groups Rust crates by architectural responsibility before adding more memory
  and knowledge storage backends.
- [`retrieval-composition-boundary`](retrieval-composition-boundary/spec.md):
  moves multi-source retrieval composition out of store adapters and into a
  storage-neutral retrieval boundary.

## Existing Slices

Older slice directories in this folder remain the historical implementation
ledger. Prefer adding new active work to the list above when a fresh spec is
opened.
