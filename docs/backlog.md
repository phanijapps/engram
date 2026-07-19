# Backlog — open items by spec

Single index of **open** work across every spec in `docs/specs/`. Each item
names the spec, the Acceptance Criterion (where one applies), what's blocking
it, and how it gets unblocked. Closed/shipped work is **not** kept here — see
each spec's Changelog and [`product/changelog.md`](product/changelog.md).

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

## backend-agnostic-retrieval

- **Durable dedup (deferred: durable-dedup):** `content_hash`-keyed `ON CONFLICT`
  upsert so re-indexing a repo reuses unchanged embeddings, plus a dead-vector GC
  sweep — blocked on nothing; unblocked by a `sqlite-vec` adapter slice. [spec O2]
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

## deployment-adapters (intent only — no spec yet)

- **pgvector(graph+vector) adapter:** one Postgres holding graph + chunks +
  embeddings. Documented target in RFC-0005 §Target deployments; needs an ADR +
  spec before work.
- **pgvector(vector) + neo4j(graph) adapter:** split deployment. Same — needs an
  ADR + spec before work.

## knowledge-graph-retraction (deferred nits — self-healing, demo-scale)

- **Repo-node GC error swallowed:** `maybe_delete_repo_node`'s error is discarded
  in the serial post-pass (scanner.rs); a failed GC leaves a harmless orphan
  Repository node that converges on the next scan. Optionally count it into
  `summary.errors` for observability.
- **Transient graph-drop on ingest-error-after-delete:** a file whose prior graph
  is deleted in the pre-pass but then hits `Outcome::Error` in the parallel write
  has its graph absent until the next scan (self-healing; inherent to
  delete-before-write).
- **Canonicalize-failure treated as removal:** a previously-ingested file hit by a
  transient canonicalize/I/O error is not added to `observed_paths`, so its graph
  is deleted as a "removal" and re-ingested next scan (self-healing availability
  edge, caller-scope-bounded). Fix: treat canonicalize failure on a prior-manifest
  path as retain, not remove.

## contract-first-ingestion (deferred hardening)

- **Contract manifest namespace fragility:** contract-op manifest entries live under
  `contract:<rel>` keys in the same flat `HashMap` as raw file-path keys; a repo
  file literally named `contract:...` at its root would collide. It fails safe
  today (`unwrap_or_default` → empty, no mis-retraction), so this is a robustness
  Nit. Fix: use a separate manifest map for contract keys, or a delimiter/prefix
  that cannot appear in a relative path.
- **Rust SCA gate + YAML crate re-eval:** `serde_yml` (a fork of the deprecated
  `serde_yaml`, RUSTSEC-2024-0320) parses untrusted OpenAPI docs, but CI runs only
  fmt/check/clippy/test — no `cargo audit`/`cargo deny`. Wire an advisory SCA gate
  in CI + a pre-handoff hook, and re-evaluate whether a maintained YAML crate with
  built-in parser limits (e.g. depth budget) is preferable for untrusted input.
  (A code-level YAML-bomb depth/alias guard ships with the feature; this is the
  dependency-hygiene follow-up.)

## knowledge-source-retraction (intent only — no spec yet)

- **Document/chunk/embedding retraction on re-ingest:** `knowledge-graph-retraction`
  converges the knowledge *graph* (entities/relationships/graphs + Repository node),
  but not the underlying `SourceDocument`s, `KnowledgeChunk`s, or their sqlite-vec
  embeddings (keyed by `document_id`/`source_id`, not `graph_id`). A changed/removed
  file's prior document + chunks + vectors currently linger. Needs `delete_document`/
  `delete_chunk` ports, a vec-index delete-by-target-id, and a stable-key handle on
  documents so the reconcile can find prior documents by `(stable_source_key, path)`.
  RFC-0009 flagged the embedding cascade (OQ2). Needs a spec before work.

## lexical-wiring

- **End-to-end keyword retrieval (deferred: lexical-wiring):** wire the shipped
  lexical `RetrievalIndex` (`engram-store-lexical`, B1) into the live pipeline so
  `RetrievalMode::Keyword` returns BM25-ranked chunks composed with graph + vector
  via the bindings-layer RRF fusion — a `lexical_candidates_json` binding +
  `SqlKnowledgeStore`-backed resolver. Deferred from
  [`lexical-keyword-retrieval`](specs/lexical-keyword-retrieval/spec.md) (eval
  fixture, router/fusion composition, full workspace gates). Blocked on: the
  composition layer is bindings-layer RRF fusion (`RetrievalRouter` is unused).
  Tracked in [`lexical-wiring`](specs/lexical-wiring/).
- **L6 — persistent lexical index + ingest feed (deferred: lexical-persistent-index):**
  populate-on-query (current) rebuilds the index per request — fine for demo
  corpora, not repo-scale. Production shape: a file-backed `LexicalIndex` fed by
  the ingest chunk-write path. Decide after latency measurement on a realistic
  corpus; recommend a separate `lexical-persistent-index` spec.

## cross-encoder-rerank (wiring + model)

- **compose_context rerank hook (deferred: rerank-wiring):** apply the shipped
  `engram-rerank-cross-encoder` (B2) inside `compose_context` between fusion and
  budget via a `RetrievalReranker` port. Deferred from
  [`cross-encoder-rerank`](specs/cross-encoder-rerank/spec.md).
- **Feature-gated real cross-encoder model:** ground the pinned `fastembed`
  reranker API (or an ONNX fallback) behind a feature flag; the adapter ships
  with an injected stub scorer today.

## graph-analytics (follow-ups)

- **Louvain multi-level aggregation + cluster wiring:** the single-level
  local-moving phase ships; multi-level aggregation and wiring communities to
  `HierarchyNode(kind=cluster)` are follow-ups. From
  [`graph-analytics`](specs/graph-analytics/spec.md).
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

- **AC6 (deferred: unified-recall-taxonomy-episodes):** The v1 `UnifiedRecall` lanes are facts (memory `retrieve`), graph/vector/lexical (`RetrievalIndex` lanes), and beliefs (`BeliefRepository::get_belief`, 0-or-1). Two lanes are deferred: **taxonomy-expanded terms** — query expansion through concept aliases / broader-narrower relations to improve recall across synonymous entity names; no `expand_terms` port exists yet (taxonomy concepts live behind `TaxonomyRepository`, but a term-expansion contract — input query → expanded term set with provenance — has not been designed). **Episodes/evidence lane** — surfacing provenance/evidence records (the S2 `ProvenanceQuery` read) as recall candidates; this is a provenance read of a different result shape (`ProvenanceEntry`, not `RetrievalResult`), and bridging it into the fusion candidate stream requires a mapping decision (what `RetrievalTargetType` / score does a `ProvenanceEntry` carry?). Blocked on: an `expand_terms` port design (taxonomy expansion) and a provenance→candidate mapping ADR (episodes lane). Unblocked by: an ADR + spec for each lane. See `docs/specs/unified-recall-api/spec.md` (deferred anchor).
- **Beliefs-lane exact-match (follow-up):** The v1 beliefs lane uses `BeliefQuery::live_subject(scope, request.query, now)`, which does an exact-match on `subject.key`. A free-text query that does not verbatim equal a stored subject key yields `None` from this lane. Fuzzy / entity-alias / semantic mapping from query to subject key is a follow-up; it needs a `match_belief_subject` port or an alias-resolution step before `get_belief`. Blocked on: a query→subject mapping design (does the lane search by content substring? alias? entity ref?). Unblocked by: an ADR deciding the matching strategy.


## export-import-hierarchy-belief

- **S5 AC5 (deferred: export-import-hierarchy-belief):** The v1 `ExportImport` export covers the families whose concrete stores expose scope-wide listing methods — knowledge (sources, documents, chunks, entities, relationships) and memory. ~~Three families were deferred~~ — hierarchy, beliefs, and concept schemes/concepts are now wired (PR #23). Only vectors remain deferred on their `Sql*` stores today: **hierarchy** (`SqlHierarchyStore` has no `list_nodes(scope)`), **belief** (`SqlBeliefStore` has no `list_beliefs(scope)`; only `list_stale` and by-id/by-source lookups), and **concept schemes/concepts** (no scope-wide `list_concept_schemes`; `TaxonomyRepository::list_concepts` is per-scheme, and there is no way to first enumerate schemes). **Contradictions** are not in the round-trip at all — `ImportData` carries no contradictions vec, so they have no export lane by construction. **Vectors** (`VectorImportRecord`) are deferred because vector storage lives behind `engram_retrieval::VectorIndex` (the `SqliteVectorIndex` adapter), which `SqlExportImport { knowledge, memory }` does not compose. Blocked on: adding scope-wide listing methods to the concrete hierarchy/belief stores (or going through their port traits — an "Ask first" item in the spec) and deciding whether vector export composes a `VectorIndex` handle or delegates to a reindex. Unblocked by: a slice adding `list_*` methods to `SqlHierarchyStore`/`SqlBeliefStore` (+ a scope-wide `list_concept_schemes`/`list_concepts` on `SqlKnowledgeStore`) and an ADR on the vector-export composition. See `docs/specs/export-import-api/spec.md` (deferred anchor) and ADR-0022.
- **Documents carry no inline content (note):** `SourceDocument` stores no inline text in this model — document text is chunked and lives in `KnowledgeChunk` records (exported separately); the exported `KnowledgeDocumentImportRecord.content` is therefore empty, with the document's identity/title/metadata preserved. Not a deferral — recorded for round-trip clarity.


## engram-viz-graph-perf

- **C7 (deferred: focus-node-pruned-by-cap):** When a user clicks an insight/search result whose node was pruned by the server-side `maxNodes` degree cap, `GraphCanvas`'s recenter effect no-ops silently (`graphData.nodes.find(...)` misses). Pre-cap this almost always found the node; post-cap, low-degree symbols (e.g. some dead-code entries) can be absent. The honest fix is a design call: either refetch that node's neighborhood with `?maxNodes` disabled for the focus target, or surface a "node hidden by cap — click to expand" affordance. Blocked on: deciding the affordance (refetch vs. inline expand) and whether it composes with the Strategy 2 overview/detailed modes. Unblocked by: a small slice implementing focus-target neighborhood refetch. See `docs/specs/engram-viz-graph-perf/spec.md`.
- **Lexical index blocks the event loop (RESOLVED 2026-07-12):** The >90s freeze was a root-cause bug, not scale: `LexicalIndex::upsert` committed once per document, and `index_for_search_json` called it per entity (~18k commits → ~218s host freeze). Fixed by adding `LexicalIndex::upsert_batch` (one commit for the whole corpus) and switching the binding to it — full-corpus build dropped from ~218s to ~810ms, well under any perceptible freeze, so no `worker_threads` offload is needed. The `ensureLexical` `building` flag remains for readiness reporting. Forward-looking note: if the indexed corpus grows ~100×, revisit a worker-thread offload; the build is synchronous N-API (in-RAM `LexicalIndex`, not DB-persisted, so not cross-thread shareable today). See `adapters/retrieval/tantivy-lexical/src/index.rs`, `bindings/node/src/knowledge.rs`, `docs/specs/engram-viz-graph-perf/spec.md`.


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
- **Specs lifecycle groups:** (mostly superseded by the historical-specs
  consolidation) evaluate whether `docs/specs/` still wants `active/` /
  `shipped/` / `retired/` grouping.
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
