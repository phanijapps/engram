# Specs

This directory holds active spec-driven implementation slices. Each feature
directory owns a `spec.md` contract and a `plan.md` implementation strategy.

## Active

- [`lexical-keyword-retrieval`](lexical-keyword-retrieval/spec.md): the
  BM25/Tantivy lexical `RetrievalIndex` adapter crate (`engram-store-lexical`:
  store + identifier tokenizer + resolver + `RetrievalResult` shaping),
  implementing the already-contracted `RetrievalMode::keyword`. No contract
  change. Codegraph-parity item B1. **Shipped (adapter unit).**
- [`lexical-wiring`](lexical-wiring/spec.md): wire the B1 lexical adapter into
  the live retrieval pipeline — `lexical_candidates_json` binding + a
  `SqlKnowledgeStore`-backed resolver, composed with graph + vector via the
  existing RRF fusion, plus an end-to-end eval fixture and full gates. Split from
  B1 after the composition layer was found to be bindings-layer RRF fusion, not
  the unused `RetrievalRouter`. Draft.
- [`cross-encoder-rerank`](cross-encoder-rerank/spec.md): a cross-encoder
  reranker adapter (`engram-rerank-cross-encoder`) implementing the contracted
  `RerankStrategy::CrossEncoder` — reorders fused candidates by an injected
  `RerankScorer`, stamps `FusionTrace`. No contract change. Codegraph-parity B2.
  **Shipped (adapter unit).**
- [`graph-analytics`](graph-analytics/spec.md): a std-only graph-analytics crate
  (`engram-graph-analytics`) with PageRank + betweenness (Brandes) + communities
  (single-level Louvain) + reachability primitives (`in_degree`, `ancestors`,
  `shortest_path`) over a generic edge list. No contract change. Codegraph-parity
  B3/B4/B5 (+ reachability for C4/C5). **Shipped.**
- [`codegraph-queries`](codegraph-queries/spec.md): the first on-top codegraph
  crate (`engram-codegraph-queries` at `codegraph/queries/`) — dead-code,
  blast-radius, dependency-path, central symbols (PageRank), bridge symbols
  (betweenness), and communities (Louvain) over `KnowledgeRelationship` `calls`
  edges, delegating to `engram-graph-analytics`. No contract change.
  Codegraph-parity C4/C5 + dependency-path + architecture-overview. **Shipped.**
- [`rust-crate-integration`](rust-crate-integration/spec.md): stable Rust crate integration contract for embedding Engram as a library — provider facade with capability reporting, typed repository handles, embedding provider abstraction (FastEmbed + Ollama), embedding-space validation, migration/import API with dry-run/apply gating, retrieval trace contract, and conformance harness. Draft.
- [`sqlite-open-options`](sqlite-open-options/spec.md): common `SqliteOpenOptions` configuration for all SQLite adapters (WAL mode, busy timeout, foreign keys, migrations, directory creation) with `open_with_options` constructors. Shipped.
- [`demo-reimagine`](demo-reimagine/spec.md): prune the demo to 5 views
  (Dashboard, hero Graph, Chat, Memory, Belief); rebuild the graph as a 2D
  community-clustered force graph readable at the class level; add a
  spec-compliant stdio MCP server GitHub Copilot can spawn. Draft.
- [`memory-cue-anchors`](memory-cue-anchors/spec.md): entity extraction at write
  time populates `MemoryContent.entities`; SQLite adapter dispatches
  `RetrievalMode::Cue` against stored entity anchors for multi-hop retrieval
  (ADR-0015). Shipped.
- [`source-assertion-reconciliation`](source-assertion-reconciliation/spec.md):
  reconciliation core — a federated `SourceAssertion` domain type + an in-memory
  authority-aware survivorship synthesizer (injected authority policy, advisory
  contradiction on tie) (ADR-0012, ADR-0013, RFC-0007). Draft.
- [`contract-first-ingestion`](contract-first-ingestion/spec.md): Phase A of
  cross-repo linkage — parse OpenAPI documents in scanned repos into
  `EntityKind::Api` contract nodes keyed by a normalized `METHOD /path` identifier
  with `exposes` edges; two repos declaring the same key merge into one node
  (ADR-0016, ADR-0017, RFC-0008). Shipped.
- [`structured-repo-identity`](structured-repo-identity/spec.md): foundation for
  cross-repo linkage — a SHA-free stable-source-key (normalized git remote) on
  each `KnowledgeGraph` (metadata + lifted columns), `graph_id` attribution on
  entities, `SourceKind::GitRepository` tagging, and one `EntityKind::Repository`
  node with `belongs_to` edges (ADR-0017, ADR-0018, RFC-0008/0009). Shipped.
- [`knowledge-graph-retraction`](knowledge-graph-retraction/spec.md): re-ingest
  converges the knowledge graph to current state — `delete_*` ports on
  KnowledgeRepository/KnowledgeGraphRepository (cascade + scope-checked) and a
  per-(stable_source_key,path) reconcile in ingest (RFC-0009, ADR-0018). Shipped.
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
- [`retire-knowledge-inmem`](retire-knowledge-inmem/spec.md): retire the
  dedicated in-memory knowledge adapter and use SQLite-backed knowledge tests as
  the executable local conformance surface.
- [`retire-memory-inmem`](retire-memory-inmem/spec.md): retire the broad
  in-memory memory adapter and use focused SQLite-backed stores as the
  executable local conformance surface.
- [`workspace-architecture-alignment`](workspace-architecture-alignment/spec.md):
  align architecture docs, planning references, and shared tooling with the
  research-backed v2 architecture direction.
- [`agentzero-engram-adapter-integration`](agentzero-engram-adapter-integration/spec.md):
  define the AgentZero-side `zbot-engram-adapter` provider contract, parity
  fixtures, gateway wiring, migration dry-run, and rollout gates for running
  AgentZero memory jobs against Engram-backed storage and operations.
- [`research-architecture-parity`](research-architecture-parity/spec.md):
  close the remaining gaps between `docs/research/` and implementation so
  Engram reaches research-architecture parity as a pristine local Rust library
  and TypeScript integration surface, excluding the actual AgentZero provider
  cutover.

## Existing Slices

Older slice directories in this folder remain the historical implementation
ledger. Prefer adding new active work to the list above when a fresh spec is
opened.
