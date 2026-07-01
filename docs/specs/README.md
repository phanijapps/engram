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
- [`demo-ui-shell`](demo-ui-shell/spec.md): re-skin `demo/frontend` onto the
  [shadcn-admin](https://github.com/satnaing/shadcn-admin) shell (TanStack Router
  + Tailwind + shadcn/ui + sidebar + command palette + dark mode) — one route per
  capability, keep `Graph3D`, backend untouched.
- [`ast-symbol-extraction`](ast-symbol-extraction/spec.md): tree-sitter AST
  symbol extraction for 10 languages (Java, Kotlin, Apex, Perl, Bash, PHP, COBOL,
  Rust, TS, Python) + chunk entity-ref population so Q&A finds actual code.
- [`llm-text-extraction`](llm-text-extraction/spec.md): LLM entity/relationship
  extraction for markdown/text docs (RFCs, ADRs, skill docs) — fixes Q&A
  grounding on documentation.
- [`kg-redesign`](kg-redesign/spec.md): knowledge graph as the centerpiece —
  WebGL force-graph (70%+ viewport), node detail panel, performance
  virtualization, indexing as popup modal.
- [`dashboard-tenant-view`](dashboard-tenant-view/spec.md): tenant + indexed
  repos (git remote/branch/SHA) + document/chunk counts overview.
- [`benchmark-lazy-embeddings`](benchmark-lazy-embeddings/spec.md): query-time
  (lazy) embeddings + KG, with a warm-up benchmark — quality vs the KG-only
  baseline + latency/coverage across passes. Shipped; results in
  `docs/perf/lazy-embeddings.md`.
- [`backend-agnostic-retrieval`](backend-agnostic-retrieval/spec.md): RRF-fused
  hybrid over the composition seam — graph + vector behind `RetrievalIndex`,
  durable sqlite-vec, configurable RRF, orchestrator + backend config.
  SQLite-only; the two Postgres/Neo4j deployments are documented targets
  (RFC-0005 / ADR-0009). Shipped.
- [`mcp-server`](mcp-server/spec.md): expose backend as MCP HTTP server —
  index_repo, search, agentic_search, get_job tools for any MCP client.
- [`db-adapter-skill`](db-adapter-skill/spec.md): Claude Code skill for adding
  database backends (Postgres, Neo4j) with cloud sizing support.
- [`tf-deploy-skill`](tf-deploy-skill/spec.md): Claude Code skill for Terraform
  deployment to AWS/GCP/Azure with LLM + embedding model config.

## Existing Slices

Older slice directories in this folder remain the historical implementation
ledger. Prefer adding new active work to the list above when a fresh spec is
opened.
