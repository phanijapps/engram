# Backlog — open items by spec

Single index of **open** work across every capability (capabilities live in
`docs/product/engram.md`). Each item names the spec/area, the Acceptance Criterion
(where one applies), what's blocking it, and how it gets unblocked. Closed/shipped
work is **not** kept here — see [`product/changelog.md`](product/changelog.md).

This is the tactical **backlog**: per-instance, no pack-side source after first
install — it's yours to curate. It is distinct from the **product roadmap**
(strategy, not a work index) at [`product/roadmap.md`](product/roadmap.md).
"Roadmap" = direction; "backlog" = the work/deferral index.

Deferred acceptance criteria point here by **anchor**: a spec criterion written
`- [ ] <outcome> (deferred: <anchor>)` means `<anchor>` resolves to a heading in
this file (GitHub heading-slug rules — lowercase, spaces become hyphens). The
deferral lives here, version-controlled and greppable, not in a PR comment that
rots. See `CONVENTIONS.md` § 4 (Spec metadata contract).

## How this file is maintained

- Every spec records its own `Status:` field and `Acceptance Criteria`
  checkboxes. This file aggregates the **open** items so they're visible in one
  place — it is not the source of truth.
- When an AC closes or a spec ships, update the spec first, then **remove** the
  now-closed item here in the same change (closed work lives in the spec
  Changelog / `product/changelog.md`, not here).
- When a new spec lands with open ACs, add a section here.
- If an item here is no longer accurate against the underlying spec, trust the
  spec and fix this file.

---

## viz-non-graph-view-types

- **Non-graph view-types (deferred: viz-non-graph-view-types):** `BeliefView`,
  `HierarchyNodeView`, `OntologyView`, `TaxonomyConceptView` are type-only stubs
  in `viz-foundation`; their projections + fixture parity tests land in S3
  (beliefs) / S4 (hierarchy/ontology/taxonomy) where those surfaces render.
  Blocked on nothing. [spec viz-foundation AC5]

## viz-memory-search

- **Memory hybrid search (deferred: viz-memory-search):** the Memory tab's hybrid
  recall search (debounced; returns the recall `ContextPayload`, not a keyset page)
  is deferred — the `agentzero` store's retrieval is `Unsupported` (`UnsupportedStoreFamily`)
  and vectors are `RequiresReindex` (`EmbeddingSpaceMismatch`), so recall returns no
  usable results today. Ships when retrieval is supported on this store (or vectors
  are reindexed). The browse lists (Facts/Beliefs/Procedures) + empty-states are
  shipped without it. Blocked on retrieval support. [spec viz-memory AC3]

## viz-community-friendly-names

- **Community friendly names + dominant kind (deferred: viz-community-friendly-names):**
  communities are labeled with raw Louvain integer IDs ("Community 65"), which carry
  no meaning. Derive a human `label` + dominant `kind` per community **at compute
  time** in `computeOverview` — no extra DB queries: tally `subject.kind`/`object.kind`
  per label during the existing relationship stream (the records already carry kind),
  and extract a namespace via longest-common-prefix over a name sample from the
  already-computed `getLabelMap`. Scheme = "dominant kind, plus namespace when the
  sample shares one" (e.g. a repo cluster → `repository · zbot`, an API cluster →
  `api · /api/curator/*`); bare generic names (`validate`, `write`) have no shared
  prefix, so those fall back to the dominant kind alone (`functions`) — never a
  fabricated name. Surface `kind`/`label` on the globe, the 2D overview, and the
  drill panel, and color the globe by kind. Folded into the cached overview, so zero
  added latency. Blocked on nothing. [specs viz-graph-explorer, viz-observatory]

## backend-agnostic-retrieval

- **Durable dedup (deferred: durable-dedup):** `content_hash`-keyed vector reuse
  on re-index is partially done — the `gc_orphan_targets` sweep + upsert-by-id
  shipped (#72). The full `content_hash`-skip-reembed path (check hash before
  calling the embedder) remains. Blocked on nothing. [spec O2]
- **Rust composition orchestrator (deferred: rust-orchestrator):** a composition
  orchestrator in `core/orchestration` (the demo currently orchestrates in TS) —
  blocked on a second backend making TS-side orchestration insufficient;
  unblocked by the Postgres or Neo4j adapter. [spec T3; option b′]

## demo-reimagine-manual-qa

- **Manual QA (deferred: demo-reimagine-manual-qa):** two human-in-the-loop
  checks the automated gates can't cover — (1) render `/graph` against an indexed
  repo and confirm it reads at the class level (module/class hubs labeled, methods
  small until hover, colored clusters); (2) add the built stdio server to GitHub
  Copilot via the documented `bin` command and confirm `tools/list` + a
  `tools/call` succeed end-to-end. Blocked on nothing; needs a browser + a Copilot
  client. Mechanical proxies already green: graph-model unit tests, MCP in-memory
  protocol tests, and a stdout-clean `initialize` smoke. [spec manual-QA ACs]

## deployment-adapters

> **Closed:** pgvector(graph+vector) adapter shipped (RFC-0017 Phase A, #76/#77) —
> knowledge/graph + memory + belief + hierarchy + procedures + vectors + recall
> over Postgres. The pgvector(vector) + neo4j(graph) split deployment remains
> an alternative layout if a graph-native engine is needed.

## knowledge-graph-retraction (deferred nits — self-healing, demo-scale)

- **Transient graph-drop on ingest-error-after-delete:** a file whose prior graph
  is deleted in the pre-pass but then hits `Outcome::Error` in the parallel write
  has its graph absent until the next scan (self-healing; inherent to
  delete-before-write).

> Closed: "Repo-node GC error swallowed" (now counted into `summary.errors`) and
> "Canonicalize-failure treated as removal" (prior-manifest paths are now retained
> on a transient canonicalize error) — see the scanner fix.

## contract-first-ingestion (deferred hardening)

> **Closed:** Contract manifest namespace fixed (#78) — entries now use the ASCII
> Unit Separator (U+001F) as a prefix, which cannot appear in a file path on any OS.
- **Rust SCA gate + YAML crate re-eval:** The `cargo audit` SCA gate is now
  wired in CI (#81). `serde_yml` 0.0.12 is the maintained fork of the deprecated
  `serde_yaml`; RUSTSEC-2024-0320 applies to `serde_yaml`, not `serde_yml`. The
  code-level YAML-bomb depth/alias guard (yaml_safety.rs) remains. If `cargo audit`
  ever flags `serde_yml`, swap to a crate with built-in parser limits.

## knowledge-source-retraction

> **Closed:** Document/chunk retraction on re-ingest shipped (#71) — the reconcile
> cascade now retracts prior SourceDocuments + KnowledgeChunks alongside the graph.
> Embedding GC (dead-vector sweep) shipped (#72) via `gc_orphan_targets`.
> The only remaining tail is the fastembed-gated full-dedup path (skip re-embed
> on unchanged content_hash), tracked under durable-dedup above.

## lexical-wiring

> **Closed:** Lexical BM25 lane is wired into the live recall pipeline —
> `LexicalRetrievalIndex` (Tantivy) is pushed into `retrieval_lanes` in the
> SQLite bootstrap, fed via `scan_repo` through the `LexicalFeed` handle,
> and fused with graph + vector + temporal via RRF. Keyword `search`/`recall`
> return BM25-ranked candidates.
  Tracked in `lexical-wiring` (see `docs/backlog.md`).
- **L6 — persistent lexical index + ingest feed (deferred: lexical-persistent-index):**
  populate-on-query (current) rebuilds the index per request — fine for demo
  corpora, not repo-scale. Production shape: a file-backed `LexicalIndex` fed by
  the ingest chunk-write path. Decide after latency measurement on a realistic
  corpus; recommend a separate `lexical-persistent-index` spec.

## cross-encoder-rerank (wiring + model)

- **compose_context rerank hook (deferred: rerank-wiring):** apply the shipped
  `engram-rerank-cross-encoder` (B2) inside `compose_context` between fusion and
  budget via a `RetrievalReranker` port. Deferred from
  [`cross-encoder-rerank`](product/engram.md).
- **Feature-gated real cross-encoder model:** ground the pinned `fastembed`
  reranker API (or an ONNX fallback) behind a feature flag; the adapter ships
  with an injected stub scorer today.

## graph-analytics (follow-ups)

- **Louvain multi-level aggregation + cluster wiring:** the single-level
  local-moving phase ships; multi-level aggregation and wiring communities to
  `HierarchyNode(kind=cluster)` are follow-ups. From
  [`graph-analytics`](product/engram.md).
- **Analytics → retrieval wiring:** popularity prior (PageRank) and bridge
  detection (betweenness) as retrieval signals.

## codegraph-parity remaining (RFC-0012)

Most items shipped (A1-A2, B1-B8, C1-C9, D3-D4, D6-D8). What remains:

- **B6b** — `as_of` retrieval filter (v1 contract change on `QueryFilter`).
- **B6 ingest-stamping** — stamp `validFrom`/`validUntil` on re-index
  (conflicts with ADR-0018 hard-delete; needs a retraction-mode decision).
- **D1** — N-API bindings for lexical/rerank/analytics base capabilities
  (currently standalone; MCP server calls Rust directly).
- **D5** — Dashboard UI (graph/timeline/insights) — on the demo branch.
- **D7** — Fleet coordination — needs a protocol design (weakest-grounded).
- **lexical-wiring** — wire B1 (BM25) into the live RRF pipeline.
- **rerank-wiring** — wire B2 (cross-encoder) into `compose_context`.

<!-- Add one section per spec with open work, e.g.:

## <spec-name>

- **AC<N> (deferred: <anchor>):** <what's open> — blocked on <X>; unblocked by <Y>.

-->

## provider-sdk-capability-report

- **AC4 (deferred — engine-neutrality full coverage):** ADR-0022 rule 1 is enforced by S1 only on the clean port-trait crates (`domain`, `memory`, `knowledge`, `retrieval`, `belief`, `hierarchy`, `consolidation`, `orchestration`) + `core/integration/src/{provider,capability}.rs`. Full coverage is deferred for four pre-existing violations — `engram-runtime` (home-grown `SqliteOpenOptions`/`SqliteJournalMode`/`SqlitePath` in `core/runtime/src/options.rs`, re-exported from the crate root), `core/integration/src/config.rs` (`SqliteStorageLayout`), `core/eval/tests/fixture_runner.rs` (constructs `SqlMemoryService`), and `bindings/node` (pervasive `Sql*`, bypasses the provider). Blocked on: moving engine-specific config/types behind a backend recipe (`backends/<name>` per ADR-0022) and routing `bindings/node` through `EngramProvider`. Unblocked by: adoption of a second storage engine (forces the `backends/` extraction) or a dedicated bindings-node-through-provider slice.

## unified-recall-taxonomy-episodes

- **AC6 (deferred: unified-recall-taxonomy-episodes):** The v1 `UnifiedRecall` lanes are facts (memory `retrieve`), graph/vector/lexical (`RetrievalIndex` lanes), and beliefs (`BeliefRepository::get_belief`, 0-or-1). Two lanes are deferred: **taxonomy-expanded terms** — query expansion through concept aliases / broader-narrower relations to improve recall across synonymous entity names; no `expand_terms` port exists yet (taxonomy concepts live behind `TaxonomyRepository`, but a term-expansion contract — input query → expanded term set with provenance — has not been designed). **Episodes/evidence lane** — surfacing provenance/evidence records (the S2 `ProvenanceQuery` read) as recall candidates; this is a provenance read of a different result shape (`ProvenanceEntry`, not `RetrievalResult`), and bridging it into the fusion candidate stream requires a mapping decision (what `RetrievalTargetType` / score does a `ProvenanceEntry` carry?). Blocked on: an `expand_terms` port design (taxonomy expansion) and a provenance→candidate mapping ADR (episodes lane). Unblocked by: an ADR + spec for each lane. See `docs/product/engram.md` (deferred anchor).
- **Beliefs-lane exact-match (follow-up):** The v1 beliefs lane uses `BeliefQuery::live_subject(scope, request.query, now)`, which does an exact-match on `subject.key`. A free-text query that does not verbatim equal a stored subject key yields `None` from this lane. Fuzzy / entity-alias / semantic mapping from query to subject key is a follow-up; it needs a `match_belief_subject` port or an alias-resolution step before `get_belief`. Blocked on: a query→subject mapping design (does the lane search by content substring? alias? entity ref?). Unblocked by: an ADR deciding the matching strategy.


## export-import-hierarchy-belief

- **S5 AC5 (deferred: export-import-hierarchy-belief):** The v1 `ExportImport` export covers the families whose concrete stores expose scope-wide listing methods — knowledge (sources, documents, chunks, entities, relationships) and memory. ~~Three families were deferred~~ — hierarchy, beliefs, and concept schemes/concepts are now wired (PR #23). Only vectors remain deferred on their `Sql*` stores today: **hierarchy** (`SqlHierarchyStore` has no `list_nodes(scope)`), **belief** (`SqlBeliefStore` has no `list_beliefs(scope)`; only `list_stale` and by-id/by-source lookups), and **concept schemes/concepts** (no scope-wide `list_concept_schemes`; `TaxonomyRepository::list_concepts` is per-scheme, and there is no way to first enumerate schemes). **Contradictions** are not in the round-trip at all — `ImportData` carries no contradictions vec, so they have no export lane by construction. **Vectors** (`VectorImportRecord`) are deferred because vector storage lives behind `engram_retrieval::VectorIndex` (the `SqliteVectorIndex` adapter), which `SqlExportImport { knowledge, memory }` does not compose. Blocked on: adding scope-wide listing methods to the concrete hierarchy/belief stores (or going through their port traits — an "Ask first" item in the spec) and deciding whether vector export composes a `VectorIndex` handle or delegates to a reindex. Unblocked by: a slice adding `list_*` methods to `SqlHierarchyStore`/`SqlBeliefStore` (+ a scope-wide `list_concept_schemes`/`list_concepts` on `SqlKnowledgeStore`) and an ADR on the vector-export composition. See `docs/product/engram.md` (deferred anchor) and ADR-0022.
- **Documents carry no inline content (note):** `SourceDocument` stores no inline text in this model — document text is chunked and lives in `KnowledgeChunk` records (exported separately); the exported `KnowledgeDocumentImportRecord.content` is therefore empty, with the document's identity/title/metadata preserved. Not a deferral — recorded for round-trip clarity.


## engram-viz-graph-perf

- **C7 (deferred: focus-node-pruned-by-cap):** When a user clicks an insight/search result whose node was pruned by the server-side `maxNodes` degree cap, `GraphCanvas`'s recenter effect no-ops silently (`graphData.nodes.find(...)` misses). Pre-cap this almost always found the node; post-cap, low-degree symbols (e.g. some dead-code entries) can be absent. The honest fix is a design call: either refetch that node's neighborhood with `?maxNodes` disabled for the focus target, or surface a "node hidden by cap — click to expand" affordance. Blocked on: deciding the affordance (refetch vs. inline expand) and whether it composes with the Strategy 2 overview/detailed modes. Unblocked by: a small slice implementing focus-target neighborhood refetch. See `docs/product/engram.md`.
- **Lexical index blocks the event loop (RESOLVED 2026-07-12):** The >90s freeze was a root-cause bug, not scale: `LexicalIndex::upsert` committed once per document, and `index_for_search_json` called it per entity (~18k commits → ~218s host freeze). Fixed by adding `LexicalIndex::upsert_batch` (one commit for the whole corpus) and switching the binding to it — full-corpus build dropped from ~218s to ~810ms, well under any perceptible freeze, so no `worker_threads` offload is needed. The `ensureLexical` `building` flag remains for readiness reporting. Forward-looking note: if the indexed corpus grows ~100×, revisit a worker-thread offload; the build is synchronous N-API (in-RAM `LexicalIndex`, not DB-persisted, so not cross-thread shareable today). See `adapters/retrieval/tantivy-lexical/src/index.rs`, `bindings/node/src/knowledge.rs`, `docs/product/engram.md`.


## associative-graph-retrieval

- **Surface-parity lint (follow-up):** associative retrieval is fully shipped
  across both surfaces (adapter unit → N-API binding → Rust SDK facade
  unified-recall lane). Open: a `check-surface-parity.sh` lint to mechanically
  enforce the new AGENTS.md surface-parity rule (every capability reachable via
  both `engram-integration` and the N-API binding), mirroring
  `check-engine-neutrality.sh`. Blocked on: nothing; unblocked by a small
  tooling slice.

<!-- The sections below were swept from the historical feature specs when they
     were consolidated into docs/product/engram.md. They preserve the open /
     deferred / out-of-scope items that lived only in those specs. -->

## workspace-architecture-alignment

- **adapt-to-project skill dangling (bug):** session startup advertises an
  `adapt-to-project` skill that does not exist locally — restore or remove the
  advertisement.
- **Specs lifecycle groups:** (superseded — `docs/specs/` was removed and
  historical specs consolidated into `docs/product/engram.md`) the
  `active/` / `shipped/` / `retired/` grouping no longer applies.
- **Crate-path normalization:** move `adapters/orchestration/belief-sqlite` →
  `adapters/belief/sqlite`.
- **Root governance files:** decide whether README/AGENTS/GOVERNANCE/CONTRIBUTING
  stay full docs (GitHub discoverability) or stub into `docs/governance/`.

## background-repo-indexer

- **Out of scope (logged):** job cancellation; multi-repo queuing; persistent
  job history across restarts; LLM-enhance during the parallel scan (stays
  per-doc on `/ingest` today); server-side clustering.

## predictive-retrieval

- **Wiring follow-up:** wire `RetrievalHints` into the in-memory `retrieve()`
  path or a query router.
- **Model-assisted (deferred):** expectation models; prediction-error / surprise
  signals; hierarchical multi-level prediction. Baseline is dependency-free and
  mirrors the `query_terms` tokenizer (no stopword/NLP on predicted queries).
- **Scope-binding design note:** `RetrievalHints` / `AgentState` are deliberately
  scope-agnostic — the query router binds hints to the `RetrievalRequest`'s
  `Scope` at wiring time, so no contract amendment is required.

## belief-contradiction-bitemporal

- **Out of scope (logged):** full memory-assertion → belief consolidation driver
  (a DryRun ConsolidationService exists for planning); enforced belief policy;
  temporal / as-of queries; hierarchy (deferred program-wide).

## ast-symbol-extraction

- **Out of scope (logged):** AST-level call-edge formation (tree-sitter
  `call_expression` queries); additional languages beyond the 10 shipped;
  changing the `Chunker` trait signature; LLM-enhanced extraction during scan
  (stays per-doc on `/ingest`).

## ontology-it-org

- **Out of scope (logged):** enforced (write-rejecting) ontology validation;
  generated typed ontology contract; ontology imports resolution; hierarchy
  (deferred program-wide).

## qa-over-knowledge

- **Out of scope (logged):** LLM-grounded knowledge-graph keyword search (no
  entity-search port); streaming answers; multi-turn.

## demo-ui-shell

- **Out of scope (logged):** new backend capabilities; auth (Clerk); replacing
  the 3D viz; mobile-native builds; i18n/RTL; multi-tenant.

## provenance-confidence-viz

- **Out of scope (logged):** confidence/method filtering; backend provenance
  enrichment.

## enterprise-3d-graph

- **Out of scope (logged):** hierarchy aggregation/clustering (TBD).

## scale-repo-ingestion

- **Out of scope (logged):** binary / PDF / office ingestion.

## llm-relationship-extraction

- **Out of scope (logged):** LLM enhance on the whole repo by default (opt-in
  only today).

## graph-explorer

- **Out of scope (logged):** value-stream / requirement / API-endpoint
  extraction + cross-doc semantic linking (phase 2 — needs extractor + ontology
  work); server-side clustering / pagination; community detection; saving
  explorer layouts.

## engram-viz (Phases 2–5)

- **Phase 2 — polish:** README; graph controls (zoom-to-fit, top-N node limiter,
  community filter); hover tooltips; collapsible sidebar; search warmup fix
  (persist / pre-warm the Tantivy lexical index).
- **Phase 3 — taxonomy view:** `GET /api/taxonomy`; taxonomy panel (concept-scheme
  tree; click highlights tagged entities; honest empty state).
- **Phase 4 — ontology view:** `GET /api/taxonomy`; ontology panel
  (classes / properties / findings); EntityKind legend + filter.
- **Phase 5 — advanced graph:** blast-radius highlight; dependency-path finder;
  node grouping (EntityKind / file).
- **Ask-first (deferred):** WebSocket/SSE live re-index; multi-repo overlay;
  auth / multi-user.

## engram-mcp-code-intel (RFC-0015 Phase 2)

- **N-API parity (deferred: phase-2-napi-parity):** The new `EngramProvider`
  handles (`knowledge_query`, `lexical_feed`) are reachable from Rust but not
  yet from the N-API binding (`bindings/node`). ADR-0022 surface parity
  requires they reach TS too. Blocked on: adding the handles to the binding.
- **directional temporal (deferred: phase-2-directional-temporal):** The
  `whats_changed` tool's `directional` mode needs per-project scan-baseline
  retention (storing prior manifests between scans). Blocked on: a
  `DirectionalStore` design. Unblocked by: a small slice implementing
  per-project manifest persistence.
- **api_topology (deferred: phase-2-api-topology):** The `api_topology` composite
  needs source-text access (`find_endpoints`/`find_api_calls` take `&str`). The
  provider exposes no "list chunk text" surface. Blocked on: chunk-text listing
  via `KnowledgeQuery` or a new port. Unblocked by: a slice adding that surface.
- **search entity-resolver (deferred: phase-2-search-entity-resolver):** The `search`
  tool feeds entity IDs to the lexical lane via `LexicalFeed`, but the unified-recall
  resolver (`KnowledgeLexicalResolver`) resolves BM25 hits via `get_chunk` — entity
  IDs are not chunk IDs, so every hit resolves to None and is dropped. Needs a sibling
  entity-id resolver lane (resolve via `get_entity` + return `RetrievalTargetType::Entity`)
  wired in bootstrap. Blocked on: the resolver design + bootstrap wiring.

## backends-sqlite-extraction

Phase B (`pgvector-recipe`) promotes pgvector to a `backends/pgvector` recipe and
removes pgvector from `engram-integration`, but leaves the SQLite composition in
`EngramProvider::open` (sqlite stays the in-process default per RFC-0017). Full
ADR-0022 neutrality — extracting `backends/sqlite` and making `open` engine-neutral
(hosts call `backends::<engine>::open`) — is deferred to here. Trigger: when a
third engine arrives or when `open`'s sqlite-default coupling blocks a host.

## pgvector-conformance-suite

`ConformanceHarness::new()` (adapters/integration/src/harness.rs) takes no
provider — it builds its own via `bootstrap_provider` (sqlite). To run the full
conformance fixture suite against the pgvector recipe, the harness needs a
provider-injection refactor (`run_all(provider)` or `new_for(provider)`).
Phase B ships a recipe-level bootstrap test (capabilities + knowledge
round-trip) instead; full-suite parity with SQLite waits on this.

## pgvector-get-entity-concepts

Observation (Phase B): `PgKnowledgeStore::get_entity(id, scope)` returns `None`
for a `Concept`-kind entity that `put_entity` stored and `delete_entity` can
resolve by the same id + scope. Either Concept entities are stored/read through
a different path than `get_entity` queries, or `get_entity` has a kind/scoping
bug. Pre-existing cell behavior (not introduced by the recipe move); investigate
when a consumer relies on `get_entity` for Concept entities.

## runtime-facade-belief-forget

The Phase A facade `createNativeProviderTransport` (@engram/node) dispatches
recall/write/putEntity/scan/consolidate/batchIngest but NOT `beliefPut` or
`forget`. The N-API binding already exposes them
(`requireBeliefsApi().upsertBeliefJson`, `requireMemoryApi().forgetJson`), so
these are TS facade widenings (mirror `putEntity`). Phase D (HTTP-MCP) scopes its
v1 tools to the facade's current dispatch and defers `belief_put` + `forget` MCP
tools to here.

## runtime-binding-put-relationship

`put_relationship` is NOT a TS facade widening — the held provider's graph proxy
`NativeGraphApiBinding` exposes only `getEntityJson`/`putEntityJson`/
`neighborsJson` (`packages/node/src/binding.ts`); `putRelationshipJson` exists only
on the flat `NativeKnowledgeEngineBinding`. Adding a `put_relationship` MCP tool
requires widening the Rust binding first: a `put_relationship_json` method on
`NativeGraphApi` (`bindings/node/src/provider.rs`) + the `NativeGraphApiBinding`
TS interface. Tracked here; do before exposing `put_relationship` over HTTP-MCP.

## mcp-http-remote-tls

Phase D (HTTP-MCP) v1 binds loopback (`127.0.0.1`) with the MCP SDK's
`localhostHostValidation` + `localhostOriginValidation` guards — no TLS, no auth,
no non-loopback path. The Objective's "networked/remote" framing is realized here:
TLS termination, an auth layer, and a documented non-loopback bind before the
server is exposed beyond the host.

## pgvector-recall-fusion

The `[recall_fusion]` config (RFC-0019 / `recall-fusion-config` spec) wires
weighted fusion + reranker into the default SQLite `SqlUnifiedRecall`. The
pgvector backend's `PgUnifiedRecall` (`backends/pgvector/src/recall.rs:59`) also
hard-codes `ReciprocalRankFusion::default()` + `reranker=None`; threading the
same config through it is deferred until pgvector is the active backend. Unblocks
when pgvector moves out of recipe/preview into a deployed backend.

---

## retrieval-quality-near-term (from Codex assessment + investigation)

Shipped (PRs #95-#105): bounded traversal, hybrid recall fusion, `[recall_fusion]`
config, MMR, fastembed default-on, incremental+batched embedding, honest
`fetch_rels`, slowness fix (65s→3.6s), repository filtering, lexical persistence,
NL→symbol anchor, graph cache (trait-based), size cap, capability_report vector,
search path in results, chunker split, source dedup, lane budgets, query-shape
classification, provenance + diagnostics, community label cache, exact-match
injection, discovery/evidence mode, score threshold, raw per-lane scores,
search returns chunks, `--tools` profiles, batch symbol_context/change_impact,
multi-anchor get_context, escalation handoff, score calibration, skill update.

Remaining near-term:

- **Re-scan Pi with the new chunker** — Pi was scanned before the container-anchor-only
  fix (PR #101). Re-scanning will split oversized class chunks. Trivial (just re-scan).
- **Batch `resolve_entity`** — currently one call per entity; accept `["id1","id2"]`
  to halve call count during anchor resolution.
- **`recall` discovery mode** — `recall` (the primary discovery tool) doesn't have
  `mode="discovery"` (compact names+paths only, no content). Currently capped at
  20k chars via the output bounding fix, but a true discovery mode (~80 chars/item)
  would be even cheaper.

## retrieval-quality-architectural (RFC-scale)

- **Domain-level repository filter** (deferred: domain-repo-filter): The MVP
  tool-level post-filter shipped (#100, #104). A domain-level pre-filter
  (`QueryFilter.repository` + SQL `WHERE` column before vector top-K) needs a
  schema migration + contract change. Prevents candidate starvation when an
  unrelated large repo dominates the vector top-K. Blocked on: an RFC + schema
  migration. Unblocked by: a slice adding the column + filter.

- **Qualified/per-definition identity** (deferred: qualified-identity): The graph
  collapses symbols by bare name (`entity_key` returns name, not id). Every `new`,
  `get`, `open` across the repo merges into one node. The bounded-traversal cap
  (PR #95) is a bandage; the true fix is per-definition-site identity with a
  name→entity-id disambiguation step. Breaks the name-query contract; needs a
  disambiguation design. Blocked on: an RFC.

- **Edge provenance** (deferred: edge-provenance): Every graph edge should carry
  source file, source line, target line, edge type, extractor, confidence. The
  tree-sitter extractor computes the call-site line but discards it. Additive but
  touches extraction (`adapters/ingest`) + storage schema (`knowledge_relationships`
  needs provenance columns). Blocked on: a schema migration.

- **Field-level dataflow** (deferred: field-level-dataflow): Field reads/writes,
  value-flow paths, credential/token transformations. TS/JS: incremental (weeks);
  C/C++: needs compiler backend (LLVM/SVF) — research-grade. Blocked on: for
  managed languages, an extraction spec; for C/C++, a compiler-backend adapter.

- **Indexed graph backend** (deferred: indexed-graph-backend): Replace the
  full-load model (`fetch_rels` loads ALL relationships per call) with an indexed
  structure (CSR adjacency matrix or embedded graph DB). The graph cache (#100)
  is the current bandage. The ultimate scalability fix for kernel-sized repos.
  Blocked on: a backend design + migration.

- **Enterprise metadata contract** (deferred: enterprise-metadata): Every item
  should carry authority status (authoritative/approved/draft/proposed/historical/
  commentary/inferred), valid time, publication time, source authority, ownership,
  retention classification. This is the enterprise equivalent of qualified symbol
  identity — "Refund Policy" can exist in multiple business units/versions.
  Blocked on: an ADR + domain-type additions.

- **Enterprise chunking** (deferred: enterprise-chunking): Code chunking is fixed
  (container → anchor-only). Documents (handbooks, transcripts, contracts, incident
  reports) need the same treatment: document → section → paragraph, preserving
  hierarchy + parent links. Blocked on: a chunker spec for document types.

- **Question decomposition** (deferred: question-decomposition): Complex questions
  should decompose into subqueries (the Codex "evidence-planning" workflow).
  May belong in the `engram-investigate` skill (agent-side) rather than inside
  `get_context` (tool-side). Blocked on: deciding the layer (skill vs tool).

## gpu-embedding-acceleration

GPU execution providers for fastembed: NVIDIA→CUDA (Linux), CoreML/Metal (Mac).
The runtime change is small (`InitOptions::with_execution_providers([CUDA, CPU])`
— ort auto-selects GPU when present). The build change is larger: the bge-small
model runs on **ort (ONNX)**, and fastembed's `cuda` cargo feature is
**candle-only** (for qwen3/nomic) — so ort-CUDA for bge needs `ort` built with
the CUDA execution provider + the CUDA toolkit on the host. Windows has a cleaner
`directml` path. Blocked on: confirming the ort-CUDA toolchain on the target
machine. Unblocked by: an ort execution-provider build spike.

## cross-encoder-rerank-model

The dispatch + warning landed (RFC-0019 T4). Selecting `rerank.strategy =
"cross_encoder"` currently warns + falls back to `None`. A real
`RerankScorer` model is needed to actually re-score. Blocked on: choosing +
integrating a cross-encoder model (ONNX or API-based). Unblocked by: a model
integration slice.

## describes-doc-code-enrichment

The `describes` doc↔code bridge is sparse (~241 links in the 3-repo test DB vs
11,450 `mentions` and 42,071 `calls`). The scan-filter concept-link filter
(`should_link_concept`) is deliberately conservative. Enriching the bridge (more
doc→code links) is a scan-filter tunable. Blocked on: calibration against
evaluation fixtures.

## eval-harness-runs

The `engram-perf-eval-harness` project (`~/projects/engram-perf-eval-harness`)
is initialized with 4 fixtures but has not been run against a freshly-scanned DB
with the latest binary. Needs: (1) re-scan Pi with the new chunker, (2) run the
harness, (3) record baseline metrics, (4) iterate. Blocked on: nothing (just run it).

## release-version-bump

The `release/v0.2.0` branch is stale (created before PRs #100-#105 merged).
Needs to be recreated or the workspace version bumped on main. Blocked on: nothing.