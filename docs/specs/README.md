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
- [`associative-graph-retrieval`](associative-graph-retrieval/spec.md): an
  associative `RetrievalIndex` adapter (`engram-store-associative-graph`) for the
  already-accepted `RetrievalMode::Graph` — ranks knowledge-graph entities by
  Personalized PageRank seeded at query entities (HippoRAG-style), behind an
  injected `GraphRelationshipSource` trait. Adds `personalized_pagerank` to
  `engram-graph-analytics`. No contract change. **Shipped (adapter unit).**
- [`associative-graph-wiring`](associative-graph-wiring/spec.md): wire the
  associative-graph adapter into the live TS/N-API binding — a
  `SqlKnowledgeStore`-backed `GraphRelationshipSource` (orphan-rule newtype in
  `bindings/node`), `associativeGraphCandidatesJson` binding + transport method,
  and the pre-existing `packages/node` + `packages/client` typecheck-debt fix.
  No contract change. Rust SDK facade wiring deferred. **Shipped.**
- [`associative-graph-facade-wiring`](associative-graph-facade-wiring/spec.md):
  wire the associative-graph adapter into the Rust SDK facade (`EngramProvider`)
  as a unified-recall lane — an orphan-rule `KnowledgeRelationshipSource` newtype
  + `associative_recall_lane` over `SqlKnowledgeStore`, pushed in
  `bootstrap.rs`. No contract change. Closes the AGENTS.md surface-parity gap for
  associative (now reachable via both `engram-integration` and the N-API binding).
  **Shipped.**
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
- [`codegraph-temporal`](codegraph-temporal/spec.md): the temporal scoring engine
  (`engram-codegraph-temporal`) — `recent`, `impact`, `compound` modes over
  versioned symbols (ADR-0019). Codegraph-parity C6 (3 of 6 modes). **Shipped.**
- [`rust-crate-integration`](rust-crate-integration/spec.md): stable Rust crate integration contract for embedding Engram as a library — provider facade with capability reporting, typed repository handles, embedding provider abstraction (FastEmbed + Ollama), embedding-space validation, migration/import API with dry-run/apply gating, retrieval trace contract, and conformance harness. Draft.
- [`provider-sdk-capability-report`](provider-sdk-capability-report/spec.md): S1 of the `engram-host-sdk` brief — completes the `rust-crate-integration` provider facade against the brief's 18 capability areas and 10 `CoreError` categories, adds the ADR-0022 rule-1 engine-neutrality gate (clean port-trait crates + `core/integration` provider/capability modules), and documents `EngramProvider` as the canonical Rust SDK entry. SQLite untouched. Shipped.
- [`episode-evidence-api`](episode-evidence-api/spec.md): S2 of the `engram-host-sdk` brief — a read-only `ProvenanceQuery` port + provider handle (core/integration) backed by a SQLite impl reading `Provenance`/`EvidenceRef` already embedded in records (no schema change); flips the `episodes_evidence` capability to `Supported`. Write-side deferred. Shipped.
- [`atomic-batch-ingest`](atomic-batch-ingest/spec.md): S3 of the `engram-host-sdk` brief — a best-effort `BatchIngest` port + provider handle (core/integration) that writes a semantic batch (episode/facts/entities/relationships; evidence+embeddings `Skipped` in v1) across the separate SQLite stores with per-step partial-failure reporting and `TransactionGuarantee::BestEffort` surfaced; flips `atomic_batch` to `Supported`. Not cross-store ACID (infeasible without a forbidden storage restructure). Shipped.
- [`unified-recall-api`](unified-recall-api/spec.md): S4 of the `engram-host-sdk` brief — a `UnifiedRecall` port + provider handle (core/integration) that fans one query across facts/graph/vector/lexical/beliefs and fuses them via the existing RRF + `ContextComposer` into a `ContextPayload` (reused), with per-lane degraded mode (`source_failures`) + ranking trace; flips `unified_recall` to `Supported`. Taxonomy-expansion + episodes lanes deferred. **Shipped** — production wiring landed (graph+lexical+beliefs lanes; vector behind fastembed). Taxonomy/episodes lanes still deferred.
- [`export-import-api`](export-import-api/spec.md): S5 of the `engram-host-sdk` brief — an `ExportImport` port + provider handle (core/integration) that exports a scope's semantic state into `ImportData` (reused) via the existing concrete store reads; import stays on `MigrationService`. **Shipped** — export-only port shipped (validation-only apply_import); hierarchy/belief/concept-schemes deferred.
- [`observability-api`](observability-api/spec.md): S6 of the `engram-host-sdk` brief — an `Observability` port + provider handle (core/integration) that aggregates the `CapabilityReport`, record counts by semantic type, embedding config, and versions into a `DiagnosticsSnapshot`; flips `observability` to `Supported`. Slow-query diagnostics deferred v1. Shipped.
- [`backend-conformance-coverage`](backend-conformance-coverage/spec.md): S7 (capstone) of the `engram-host-sdk` brief — a non-SQLite stub backend (HashMap `MemoryService`) passes the same lifecycle ops as the SQLite fixtures, proving the port abstraction is backend-parametric. Combined with the ADR-0022 neutrality gate (S1), this is the full proof that backend swap is config, not rewrite. Shipped.
- [`engram-viz`](engram-viz/spec.md): a web-based, single-page code-graph visualization workspace — Hono backend (REST proxy to `@engram/node` N-API) + React frontend (`react-force-graph-2d`, community-colored nodes, node detail panels, insights, timeline). Inspired by memtrace screenshots. Draft.
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
- [`context-packet-contract-additions`](context-packet-contract-additions/spec.md):
  Phase 1 of RFC-0013 — the four framework contract types (`ContextSubgraph`,
  `ApplicabilityRule`, `DecisionTrace`, `KnowledgeEntity.ontologyClassRefs`) +
  `RetrievalTargetType` variants (`Rule`/`Policy`/`Axiom`/`DecisionTrace`), as
  inert contract surface only (no composition/population/writer wiring; that
  lands in Phases 2–4). Constrained by ADR-0025, ADR-0009, ADR-0022. Shipped.

## Existing Slices

Older slice directories in this folder remain the historical implementation
ledger. Prefer adding new active work to the list above when a fresh spec is
opened.
