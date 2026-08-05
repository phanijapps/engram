# Specs

This directory holds **active** spec-driven implementation slices. Each feature
directory owns a `spec.md` contract and a `plan.md` implementation strategy.

The historical feature specs were consolidated: the capability roll-up + status
lives in [`docs/product/engram.md`](../product/engram.md), open/deferred items in
[`docs/backlog.md`](../backlog.md), durable decisions in [`docs/adr/`](../adr/),
and accepted behavior contracts in [`contracts/v1/`](../../contracts/v1/). The
spec *process* (how to author a new spec) lives in the `new-spec` + `work-loop`
skills and [`docs/CONVENTIONS.md`](../CONVENTIONS.md).

## Active

- [`viz-foundation`](viz-foundation/spec.md): the foundation slice of the
  engram-cc overhaul — a greenfield 3-tab shell (Memory/Observatory/Graph) with
  ported zbot styling (React 19 + Tailwind v4), a Hono Backend-for-Frontend that
  reads engram in-process via `@engram/node` (never engram-mcp), TS view-types,
  and a deck.gl community-overview Graph view with keyset pagination +
  aggregation. Constrained by ADR-0003, ADR-0008, ADR-0022.
  **Shipped** (T1–T9: in-process `@engram/node` BFF, zbot-styled 3-tab shell,
  deck.gl bounded community-overview; self-contained Playwright E2E + FPS-gated
  on reference hardware). S2–S4 are separate specs.
- [`viz-graph-explorer`](viz-graph-explorer/spec.md): S2 of the engram-cc
  overhaul — the full Graph tab: a legible concentric-ring overview (T0, replaces
  the foundation spiral) + community drill (click a community → bounded sample of
  its member entities via a cached label→entityId index) + entity-detail panel
  (kind/community/degree/provenance). deck.gl LOD. **Shipped** (T0–T4; backend
  38 tests, frontend build, self-contained Playwright E2E for overview + drill).
- [`viz-memory`](viz-memory/spec.md): S3 — the Memory tab: facts/beliefs/
  contradictions/procedures over engram surfaces (keyset lists via read-only
  `node:sqlite`), honest empty-states for unpopulated surfaces. **Shipped**
  (hybrid search deferred — the store's retrieval is Unsupported; browse +
  empty-states shipped). Depends on viz-foundation.
- [`viz-observatory`](viz-observatory/spec.md): S4 — the Observatory tab:
  reuses the deck.gl overview canvas + a LearningHealthBar over `/graph/stats`
  (graph/memory/belief/hierarchy), with honest empty-states for the unpopulated
  belief/hierarchy surfaces. **Shipped** (belief/hierarchy empty today;
  slideovers deferred until those surfaces have data). Depends on viz-foundation.
- [`ts-integration-read-facade`](ts-integration-read-facade/spec.md): move engram's
  read/query surface — paged list + counts — out of TypeScript (`node:sqlite` in the viz
  BFF) into `engram-integration` (ports) + the SQLite adapters, exposed via `bindings/node`
  + `@engram/node`. Retires `node:sqlite` from the TS layer; MCP read tools go on the
  facade. Constrained by ADR-0003, ADR-0022, RFC-0017. Phase 1 = memories paged vertical
  spike. Draft.
- [`knowledge-graph-identity`](knowledge-graph-identity/spec.md): storage-neutral,
  caller-policy-driven identity operations for KG entities and exact relationships,
  plus transactional duplicate consolidation. All six RFC-0014 decisions (D1–D6);
  focused `EntityIdentityRepository` port. Constrained by RFC-0014, ADR-0022.
  Draft.
- [`surreal-identity-cell`](surreal-identity-cell/spec.md): the SurrealDB adapter
  cell implementing `EntityIdentityRepository` over embedded SurrealKV with
  SURQL-native semantics (UNIQUE indexes, UPSERT, BEGIN TRANSACTION, MERGE).
  Depends on `knowledge-graph-identity` E0–E1. Constrained by RFC-0014,
  ADR-0022. Draft.
- [`engram-mcp-core`](engram-mcp-core/spec.md): Phase 1 (generic core) of RFC-0015 —
  the unified `engram-mcp` server: thin JSON-RPC loop + tool registry, one
  `EngramProvider`, fused-per-project scope, multi-layer ontology/taxonomy as MCP
  launch config, generic write/recall tools, `MarkdownChunker` + `index_docs`, and
  the `engram-distill` agent skill. Constrained by RFC-0015, ADR-0008, ADR-0009,
  ADR-0022, ADR-0020, ADR-0025. Draft.
- [`engram-mcp-code-intel`](engram-mcp-code-intel/spec.md): Phase 2 (code intelligence)
  of RFC-0015 — `scan_repo` (treesitter, routed through the provider via a fan-in
  adapter), the six consolidated composites (`symbol_context`/`change_impact`/
  `code_health`/`architecture`/`api_topology`/`whats_changed`), and `search`; adds
  new `engram-integration` exposure (`KnowledgeQuery` list methods + a lexical feed)
  so code-intel routes through `EngramProvider` with no provider bypass. Constrained
  by RFC-0015, ADR-0008, ADR-0009, ADR-0022, ADR-0020, ADR-0025. Draft.
- [`ts-provider-facade`](ts-provider-facade/spec.md): RFC-0017 Phase A keystone —
  make the held `NativeProvider` (held `EngramProvider`, 20 capabilities incl.
  consolidation execution; parity gate 0 debt) consumable from TypeScript: a
  `NativeProvider` binding surface in `@engram/node`, a `scanRepositoryJson`
  method (promoting the scan fan-in to `engram-ingest`), and a thin provider
  facade dispatching recall/write/scan/consolidate. The adoption slice the ingest /
  HTTP-MCP / maintenance modules compose on. Constrained by RFC-0017, ADR-0022,
  RFC-0015. Shipped.
- [`scan-filter-config`](scan-filter-config/spec.md): externalize the scanner's
  two hardcoded tuning lists — the cross-document concept-link filter
  (`should_link_concept`) and the file denylist (`is_denylisted`) — behind an
  optional `.engram/scan.json` (or `scan_config` arg) merged with built-in
  defaults. `engram-ingest` takes a ready `ScanFilter`; discovery lives in
  `engram-mcp`. Constrained by ADR-0022. Shipped.
- [`ownership-dependency-import`](ownership-dependency-import/spec.md): two post-index
  MCP tools — `scan_dependencies` (Cargo.toml + package.json → `Module` entities +
  `depends_on` edges) and `scan_ownership` (CODEOWNERS → `Organization`/`Person`
  entities + `owns` edges) — turning the graph into a multi-team/multi-repo program
  view. Mirrors `scan_protocols`; routes through `require_knowledge()`. Constrained
  by RFC-0012, ADR-0022. Draft.
- [`retrieval-completeness`](retrieval-completeness/spec.md): close the retrieval-mode
  gap — re-land `Temporal` / `Cue` / `Hierarchical` modes on durable `RetrievalIndex`
  adapters (currently enum-only; the impls died with the process-local fixture) and
  wire predictive retrieval. Phased; Phase 1 (predict_context tool + predictor tests)
  is the first slice. Constrained by RFC-0005, ADR-0022. Draft.
- [`knowledge-source-retraction`](knowledge-source-retraction/spec.md): on re-ingest of a
  changed/removed file, retract not just the knowledge graph (RFC-0009, already converges)
  but also the prior `SourceDocument`s, `KnowledgeChunk`s, and their sqlite-vec embeddings
  (which linger today — keyed by content-derived ids, not graph_id). Adds document
  source-key/path lookup + `delete_document`/`delete_chunk` ports + vector `delete_by_target_id`,
  cascaded children-first in reconcile. Done before pgvector so ports are defined once.
  Constrained by RFC-0009, ADR-0022. Draft.
- [`pgvector-backend`](pgvector-backend/spec.md): Postgres + pgvector as the second
  storage backend (RFC-0017 Phase A). One Postgres holds graph + chunks + memory +
  embeddings (pgvector type) + keyword (tsvector). Feature-gated engine submodule
  (`core/integration/src/pgvector/`) + `adapters/pgvector/` crate; reuses every port
  trait (no domain change). SQLite stays default. Conformance-gated. Constrained by
  ADR-0022, RFC-0017. Draft.
- [`ts-runtime-maintenance`](ts-runtime-maintenance/spec.md): RFC-0017 Phase E /
  Module 3 — the maintenance module in @engram/runtime: an `engram-maintain` CLI that
  runs consolidation (reflection + decay) over the held provider facade, one-shot or
  on an `--every` setInterval. Mirrors the ingest module (scan→consolidate). Light
  mode. Constrained by RFC-0017, ADR-0022. Shipped.
- [`ts-runtime-ingest`](ts-runtime-ingest/spec.md): RFC-0017 Phase C / Module 1 — the TS
  operational layer (`@engram/runtime`) and its first module, `engram-ingest`: a
  CLI that scans a repo into the held provider over the Phase A facade, one-shot
  or on an `--every` setInterval schedule. Cron-first; queue/webhook adapters are
  later. Constrained by RFC-0017, ADR-0022. Shipped.
- [`pgvector-recipe`](pgvector-recipe/spec.md): RFC-0017 Phase B — promote the
  pgvector backend from an engine module inside the SDK facade into a
  `backends/pgvector` recipe crate (ADR-0022: recipe owns connection lifecycle +
  composition + conformance; the only place a backend identity exists). The recipe
  becomes the pgvector host entry; `EngramProvider::open` stays engine-neutral
  (sqlite default) and rejects pgvector configs. Constrained by RFC-0017,
  ADR-0022. Shipped.
- [`codegraph-retrieval-fixes`](codegraph-retrieval-fixes/spec.md): code-intel
  tool fixes from RFC-0018 — D1 bounded traversal (shipped, PR #95) + D3 honest
  `fetch_rels`. D2 + the hybrid-recall work moved to `recall-fusion-config`
  (RFC-0019). Constrained by RFC-0018, ADR-0022. Implementing.
- [`recall-fusion-config`](recall-fusion-config/spec.md): full hybrid recall
  (vector + BM25 + RRF + reranking) with an externally configurable
  `[recall_fusion]` contract (per-lane weights + reranker strategy), vector
  opt-in, MMR + cross-encoder rerankers, and `search` via hybrid recall.
  Supersedes RFC-0018 §6.2 + D2. Constrained by RFC-0019, ADR-0022. Draft.
