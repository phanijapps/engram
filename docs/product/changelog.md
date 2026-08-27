# Changelog

All notable user-visible changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> Maintenance: this file is updated in the same PR that introduces the
> change. CI will warn (configurable: block) when a PR touches code that
> changes user-visible behavior but does not touch this file.
>
> Entries can be drafted from conventional commits: `git log --oneline`
> filtered to `feat:` and `fix:` since the last tag is a starting point,
> not a finished product. Rewrite for users, not contributors. See the
> [Common Changelog guidance](https://common-changelog.org/) — the audience
> is humans who use the software, not humans who wrote it.

## [Unreleased]

### Added

- Lazy query-time embeddings (BGE-small) generated on demand, cached, and
  persisted to a durable sqlite-vec store; per-query warm-up (hit-rate climbs
  across passes).
- Reciprocal-rank fusion (RRF, configurable k + per-source weights) of graph +
  vector retrieval — true hybrid Q&A over the `RetrievalIndex` seam (RFC-0005 /
  ADR-0009).
- Graph `RetrievalIndex` behind the port — knowledge-graph results now fuse with
  vector results; a documented path to Postgres/pgvector/Neo4j backends.
- Tree-sitter AST chunking for 13 languages (Rust, C/C++/C#, TS/JS, Python,
  Java, Kotlin, Apex, Perl, Bash, PHP) with AST call-edge extraction.
- MCP server (index_repo, search, agentic_search, get_job) for any MCP client.
- Friendlier graph explorer: meaningful node labels, source-file paths, repo +
  neighbor context on click, noise-name deprioritization.
- 8-question and 50-question code-intelligence eval suites + a warm-up benchmark
  (`docs/perf/`).
- Reference architecture + charter + roadmap instantiated for engram.
- BM25 lexical retrieval (`engram-store-lexical`, Tantivy) implementing the
  contracted `RetrievalMode::keyword` — codegraph-parity B1.
- Cross-encoder reranker (`engram-rerank-cross-encoder`) implementing
  `RerankStrategy::cross_encoder` (injected scorer) — B2.
- Graph analytics crate (`engram-graph-analytics`): PageRank, betweenness, Louvain
  community detection, and reachability (`in_degree` / `ancestors` /
  `shortest_path`) — B3/B4/B5.
- Bi-temporal knowledge entities: optional `validFrom` / `validUntil` on
  `KnowledgeEntity` (ADR-0019).
- Extended `EntityKind` vocabulary: `struct`, `interface`, `trait`, `type_alias`,
  `enum`, `endpoint` (ADR-0020).
- On-top codegraph layer begun: `engram-codegraph-queries` (the first `codegraph/`
  crate) — dead-code, blast-radius, and dependency-path over `calls` edges.
- Agentic **Ask tab** in engram-cc: an LLM agent loop over the BFF
  (`/api/ask`) whose model autonomously calls `recall`, `list_memories`,
  `graph_overview`, and `write_memory` tools, iterating until it can answer —
  with the full tool-call trace rendered in the UI.
- Runtime `createLlmProvider` gains a `completeAgent` surface (full message
  history + tools → content blocks incl. tool calls) for agent loops, wired
  through pi-mono, the dry-run fixture, and test overrides.
- Hybrid recall search in engram-cc: `/api/recall` BFF endpoint + Memory-tab
  debounced two-phase search (instant content match, then recall-fusion
  upgrade). The vector lane is opt-in via `ENGRAM_ENABLE_VECTOR=true`
  (FastEmbed BGE-small, also wired through `mcp/dev.sh`).
- ForceGraph: a d3-force 2D canvas overview replaces the deck.gl viewport in
  engram-cc (smaller bundle, no WebGL dependency); e2e drill clicks use live
  node positions exposed by an e2e hook.

### Changed

- Demo Q&A now grounds answers in RRF-fused (graph + semantic) evidence.
- Graph view defaults to structural kinds (repo/module/class/function); a toggle
  shows all kinds. Dashboard is the first route.

### Deprecated

- (nothing yet)

### Removed

- (nothing yet)

### Fixed

- `scan_repo` now returns within client timeouts on a populated store: the
  lexical feed is scoped to the scan's own entities (was: every entity in the
  scope re-fed per scan) and the vector-embed step is scoped + capped per call
  with the remainder reported (was: an unbounded cross-source backlog ground
  through the shared model mutex inside the tool response). Chunk text sent to
  the embedder is bounded to 8 KiB.
- Maintenance tools (`graph_health`, `list_maintenance_candidates`,
  `build_maintenance_plan`, `apply_maintenance_plan`) default a missing/null
  `scope` argument to the launch scope instead of erroring.
- `code_health` and `architecture` responses are capped (dead list truncated
  to 100 + count; community map to top 10) — previously up to 255 KB of
  inlined output.
- Code-graph analytics (`dead_code`, `central_symbols`, `bridge_symbols`,
  `call_communities`, `repository_stats`) count only relationships with both
  endpoints resolved to entity ids — legacy name-only edges no longer fake
  caller evidence or centrality.
- JSX element usage (`<Button …>` in tsx/jsx) now counts as a reference from
  the enclosing component, so JSX-referenced components are no longer flagged
  as dead code; lowercase DOM intrinsics do not emit edges.

### Security

- (nothing yet)
