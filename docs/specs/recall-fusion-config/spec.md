# Spec: recall-fusion-config

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0019, ADR-0022 (engine neutrality)
- **Brief:** none
- **Contract:** `contracts/v1/schemas/recall-fusion.schema.json` (landed in T1)
- **Shape:** mixed <!-- config contract + retrieval wiring + one adapter crate -->

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram's unified recall is a **competent generic full-hybrid retriever**: vector + BM25 + RRF + reranking run together, with **externally configurable fusion weights and reranker strategy**. An operator biases vector vs BM25 vs graph (zbot's `vector_weight`/`bm25_weight` pattern, engram-neutral), chooses a reranker (MMR / cross-encoder / none), and enables the vector lane — all from a `[recall_fusion]` config, no code changes. This closes the gap that out-of-box recall has no vector, no rerank, and equal-weight fusion, by wiring machinery engram already half-built. zbot's application-specific policy (category weights, contradiction penalty, KG decay, episodes) stays in gateway-memory.

## Boundaries

### Always do
- Carry validation in the config (`k >= 1`, weights finite & ≥ 0); reject invalid with `CoreError::InvalidRequest`.
- Reuse weighted RRF (`ReciprocalFusionConfig` + `ReciprocalRankFusion::new`); do not invent a new fusion algorithm.
- Normalize lane source tags to the stable vocabulary before keying weights on them.

### Ask first
- Extending the `RetrievalReranker` port or `RetrievalResult` (domain-truth surface) — prefer injecting an embedder into the adapter.
- Making `fastembed` (vector) default-on — it is opt-in by decision (RFC-0019 D3).

### Never do
- Port zbot product-policy (category weights, contradiction penalty, KG decay, episodes, intent boost) into `engram-retrieval`, `engram-integration`, or any adapter — those stay in gateway-memory.
- Change the storage schema or any frozen-v1 domain type.
- Hard-code weights in `SqlUnifiedRecall` (it must read them from config, defaulting to equal-weight only when config is absent).

## Testing Strategy

- **Fusion config + weighted wiring — TDD.** `RecallFusionConfig` validation (k, weights); a weighted config measurably reorders recall candidates vs equal-weight (the contract that weights "do something"); absent config ⇒ equal-weight default (backward compat). Unit + one integration over seeded lanes.
- **MMR reranker — TDD.** Diverse top-K selection; `lambda` trade-off; `None`-embedding graceful degradation; injected-embedder path.
- **Lane-tag normalization — TDD.** Each lane stamps its normalized tag; traces carry the new names.
- **MCP `search` routing — TDD.** A regression test (not a manual one-off) that a multi-term query returns ranked hits through hybrid recall.
- **Vector activation / cross-encoder wiring — goal-based.** `--features fastembed` build's `capability_report` shows vector + recall returns vector candidates; cross-encoder re-scores when selected.

## Acceptance Criteria

D2.1a — external fusion config + weighted wiring + vector activation:

- [x] A serde `[recall_fusion]` config (`rrf_k`, `default_source_weight`, per-lane `source_weights`, `rerank`) loads from the launch profile and/or `.engram/recall.json`; validated; defaults to equal-weight RRF when absent. The schema lands at `contracts/v1/schemas/recall-fusion.schema.json`. (Profile section + `.engram/recall.json` ladder landed in T1c; the MCP `open_provider` feeder landed with the review pass — `mcp/engram-mcp/src/bootstrap.rs` resolves `<storage_path>/.engram/recall.json` and applies `.with_recall_fusion`, surfacing a malformed file as a boot error. ADR-0026 records the contract.)
- [x] `SqlUnifiedRecall` honors the configured weights via weighted RRF (the `with_reranker` constructor takes the `ReciprocalFusionConfig`); no hard-coded `default()`.
- [x] Lane source tags are normalized to a stable vocabulary (`vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`) and documented as the weight keys. (Vocabulary surfaced as `engram_retrieval::KNOWN_LANE_TAGS`; `to_reciprocal_config` warns on keys outside it so a typo like `"vectors"` does not silently no-op.)
- [x] The vector lane is activatable (opt-in `fastembed` build); when on, it contributes candidates fused at `source_weights["vector"]`; `capability_report` reflects it. (Verified under `--features fastembed`: vector lane + embedding provider wire on, `vectors_state = Supported`.)

D2.1b — MMR diversity reranker:

- [x] An `MmrReranker` adapter (new `adapters/retrieval/mmr-rerank/`) implements `RetrievalReranker`, **injecting an `EmbeddingProvider`** to embed candidate texts (the port exposes no embeddings); selectable via `rerank.strategy = "mmr"` (+ `lambda`). (`lambda ∈ [0,1]` is now validated in `to_reciprocal_config`.)

D2.1c — cross-encoder reranker wiring:

- [ ] The existing `CrossEncoderRerankerAdapter` is wired behind a feature gate, selectable via `rerank.strategy = "cross_encoder"`. (deferred: `cross-encoder-rerank`) — the dispatch recognition + warning landed (`select_cross_encoder` in `core/integration/src/sqlite/bootstrap.rs`), but `select_cross_encoder()` returns `None` today: no in-tree `RerankScorer` model is wired, so selecting `cross_encoder` warns + falls back rather than re-scoring. The backlog anchor `cross-encoder-rerank` (feature-gated real model) tracks the remaining model-integration work.

D2 — search via full-hybrid recall:

- [x] MCP `search` returns ranked symbol hits for multi-term queries by routing through the (now-hybrid) recall with entity-id resolution; the whole-string `.contains()` loop is removed. A regression test (not a manual run) guards it.

## Assumptions

- Technical: `ReciprocalFusionConfig { k, default_source_weight, source_weights }` + `ReciprocalRankFusion::new(config)` exist (`core/retrieval/src/{config,reciprocal}.rs`) — weighted fusion is wiring, not new code.
- Technical: lanes stamp source tags today, but mixed (`vector.semantic`, `lexical.keyword`, `unknown` for temporal, `belief` singular, `associative_graph`) — normalization is real work, not verification (adversarial review, 2026-08-01).
- Technical: `RetrievalReranker::rerank(request, candidates)` exposes no embeddings → MMR must inject an `EmbeddingProvider` (`core/retrieval/src/ports.rs`).
- Technical: `CrossEncoderRerankerAdapter` is built but unwired (`adapters/retrieval/cross-encoder-rerank`); `SqlUnifiedRecall::new()` wires `reranker=None` (`core/integration/src/sqlite/recall.rs`).
- Technical: `EngramConfig` + a profile-loading mechanism exist (`core/integration/src/config.rs:158`); a `[recall_fusion]` section plugs in additively.
- Process: full mode — public config contract + new adapter crate + `EngramConfig` field. Constrained by RFC-0019 (supersedes RFC-0018 §6.2 + D2) + ADR-0022.
- Product: vector opt-in (not default-on); no zbot app-policy in engram (user confirmation 2026-08-01).
- Technical (parity): `[recall_fusion]` reaches the N-API binding transitively — `NativeProvider` holds an `EngramProvider` built from `EngramConfig`, so `provider.recall()` uses the configured fusion. If `bindings/node` fuses independently of `provider.recall`, that is a parity gap to verify at T1.
- Process: the pgvector backend's `PgUnifiedRecall` is **not** wired this spec (deferred: `pgvector-recall-fusion`); SQLite (default) is.
