# Codegraph Parity Audit (A1)

Read-only capability audit confirming the exact implementation state of each
borderline memtrace-parity capability, by reading code (not guessing). Feeds
micro-spec scoping in `docs/codegraph-parity-roadmap.md`. Audit date: 2026-07-08.

Method: targeted `grep`/`sed` over `core/`, `adapters/`, `docs/specs/` with
file:line citations; cross-checked against `docs/implementation-roadmap.md`
phases and `docs/specs/` status.

Legend: ✅ shipped · 🟡 partial · ❌ gap.

## Findings

### Retrieval

| Capability | State | Evidence | Missing → work item |
|---|---|---|---|
| Keyword retrieval mode (dispatch + match) | 🟡 | `core/retrieval/src/router.rs:28,40,51,65` maps `RetrievalRouteMode::Keyword`↔`RetrievalMode::Keyword`; in-memory adapter does substring/token match (Phase 3) | Ranked **BM25/Tantivy** full-text index → **B1** |
| Hybrid fusion | 🟡 | `hybrid-retrieval-fusion` Shipped (Phase 18): `WeightedRetrievalFusion` + RRF (`core/retrieval/src/reciprocal.rs`); dedup + `FusionTrace` | **Cross-encoder rerank** (`RerankStrategy::cross_encoder`) → **B2** |
| Vector retrieval | ✅ | `vector-retrieval-candidates` Shipped (Phase 22): sqlite-vec + injected `RetrievalIndex`, `VectorQueryProvider`, `VectorTargetResolver` | — |
| Temporal + Cue retrieval | ✅ | `temporal-cue-retrieval` Shipped: dispatches `RetrievalMode::Temporal` (recency, `since/until`) and `::Cue` (slot-value via links) in `engram-store-memory` | — (distinct from the 6 scoring modes) |
| Embedding providers | 🟡 | Only `FastEmbedBgeSmallQueryProvider` wired (`EmbeddingModel::BGESmallENV15`, id `fastembed/bge-small-en-v1.5`); an `EmbeddingSpace` abstraction exists (test shows `ollama/nomic-embed-text`) | **Code-specialised model** (jina-code) + provider registry → **B7** |

### Graph intelligence

| Capability | State | Evidence | Missing → work item |
|---|---|---|---|
| Graph extractor (symbols + edges) | 🟡 | `knowledge-graph-extractor` Shipped (Phase 54): `adapters/ingest/src/extractor.rs` emits `KnowledgeEntity` + `KnowledgeRelationship` (`calls`, etc.) with `graph_id`; AST + co-occurrence fallback | — |
| Cross-file edge resolution | 🟡 (not ❌) | `extractor.rs:~196-210`: callees not in the current doc become **name-only `EntityRef`s** ("The cross-file resolver connects it by name") | Resolution is **name-based**, not type/scope-aware; quality + cross-language → **C1/C2** |
| Graph analytics (PageRank/betweenness/Louvain) | ❌ | grep for `pagerank\|betweenness\|louvain\|centrality\|community\|modularity` across `core/`,`adapters/` → empty | All three primitives → **B3, B4, B5** |
| Hierarchy clustering | 🟡 | `durable-hierarchy`/`hierarchy-navigation` Shipped: aggregate/base/first-entity build + nav; but grep for clustering terms in `core/hierarchy/src` → **empty** (no clustering *algorithm*) | Louvain → cluster `HierarchyNode`s → **B5** (algorithm; build path exists) |
| Complexity / dead-code / blast-radius / dep-path | ❌ | grep for `cyclomatic\|dead.code\|blast.radius\|in_degree\|out_degree` → empty | All four → **C3, C4, C5** |
| HTTP API topology (framework scanners) | ❌ | grep for express/fastapi/flask/gin/nestjs/axum/router.route in `adapters/ingest` → only generic tree-sitter hits | Endpoint + call-site detection → **C7, C8** |

### Temporal / versioning

| Capability | State | Evidence | Missing → work item |
|---|---|---|---|
| Bi-temporal on Belief/Memory/Assertion | ✅ | `core/domain/src/belief.rs:51,53,78,80`; `memory.rs:131,133`; `assertion.rs:73,75` carry `valid_from`/`valid_until` | — |
| Bi-temporal on `KnowledgeEntity` | ❌ | `valid_from`/`valid_until` **absent** from the entity type (grep hits only belief/memory/assertion) | Optional `validFrom`/`validUntil` on entity + `as_of` retrieval → **B6** (ADR-gated) |
| 6 temporal scoring modes + significance budget | ❌ | grep for `novelty\|compound.*score\|directional.*score\|significance.budget` → empty (temporal *retrieval* is shipped, but not the *scoring modes*) | recent/impact/novel/directional/compound/overview + budgeting → **C6** |

### Cross-repo / workspace

| Capability | State | Evidence | Missing → work item |
|---|---|---|---|
| Repo identity + scale ingestion | ✅ | `structured-repo-identity`, `scale-repo-ingestion`, `background-repo-indexer` Shipped | — |
| Cross-repo linkage design | ✅ (design) | `docs/rfcs/0008-cross-repo-linkage.md` **Status: Accepted** | — |
| Multi-repo workspace fusion (one graph across repos) | ❌ | RFC 0008 accepted; no workspace-fusion implementation found in ingest (cross-*file* name-resolution exists; cross-*repo* edge fusion does not) | Workspace marker + cross-repo edge fusion → **B8** |

### Integration surface

| Capability | State | Evidence | Missing → work item |
|---|---|---|---|
| MCP server | ❌ | grep for `rmcp\|jsonrpsee\|mcp_server\|streamable.http` → empty; an `mcp-server` spec was **Draft** at audit time (proposed TS-over-demo backend, 4 tools: `index_repo`/`search`/`agentic_search`/`get_job`); capability now tracked in `docs/product/engram.md` | Build server + expand to ~38 tools → **D3, D4** (Q2: host language open; Draft leans TS) |
| Dashboard UI | 🟡 | `enterprise-3d-graph` Shipped (3D graph, demo/engram-ui) | Node CODE/INFO/HISTORY + timeline + insights + fleet panes → **D5** |
| Agent skills / fleet / CapabilityReport | ❌ | none specced | **D6, D7, D8** |

## Corrections to the parity-roadmap matrix

Two entries shift on the evidence:

1. **C1 (cross-file edge resolution): ❌ → 🟡.** Name-based cross-file resolution
   already exists (`extractor.rs`); C1 becomes *harden resolution quality +
   add type/scope-awareness + cross-language*, not "build from zero." Smaller.
2. **Temporal retrieval vs scoring modes.** The roadmap already separates them,
   but to be explicit: temporal/cue *retrieval dispatch* is **shipped**
   (`temporal-cue-retrieval`); only the **6 code-churn scoring modes** (C6) are
   the gap. Don't conflate them.

## Net: confirmed vs re-scoped micro-specs

- **Confirmed gaps (unchanged):** B1 (BM25), B2 (cross-encoder), B3/B4/B5
  (graph analytics), B6 (entity bi-temporal), B7 (code embeddings), B8
  (cross-repo fusion), C3/C4/C5 (quality ops), C6 (scoring modes), C7/C8
  (HTTP topology), D3–D8 (integration).
- **Re-scoped smaller:** C1 (harden existing name-based cross-file resolution;
  add cross-language), C2 (taxonomy is additive mapping, not new infra).
- **No new micro-spec needed:** graph extractor, hybrid fusion wiring, vector
  retrieval, temporal/cue retrieval, repo identity/scale ingestion, 3D graph UI,
  bi-temporal belief — all shipped.

## Open items for deeper read (optional, before B/C start)

- Measure the **recall/precision of the name-based cross-file resolver** on a
  real multi-file repo (informs C1's scope).
- Confirm the **in-memory keyword match shape** (substring vs token set) to size
  B1 precisely — but B1 (add Tantivy) is correct regardless.
