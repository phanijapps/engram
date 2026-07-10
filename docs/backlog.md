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
