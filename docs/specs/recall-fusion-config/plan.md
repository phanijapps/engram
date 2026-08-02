# Plan: recall-fusion-config

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy; changes noted in the Changelog.

## Approach

Make engram's unified recall full-hybrid by **wiring dormant machinery** around one new config contract. The keystone is **T1**: a serde `[recall_fusion]` config (RRF `k`, per-lane `source_weights`, `rerank` strategy) + **normalizing the lane source tags** to a stable vocabulary (without this, weights silently no-op — the tags are currently `vector.semantic`/`lexical.keyword`/`unknown`/`belief`). T1 also lands the JSON-schema contract. T2 (vector opt-in), T3 (MMR via an *injected* `EmbeddingProvider`), T4 (cross-encoder), T5 (hybrid `search`) build on T1.

Most of this reuses existing code: `ReciprocalFusionConfig` + `ReciprocalRankFusion::new`, the lanes' source stamps, `EngramConfig`'s profile loader, the built `CrossEncoderRerankerAdapter`, and `compose_context`'s fusion+rerank slot. Genuinely new: the config DTO + schema, the lane-tag rename, and the ~45-line MMR adapter (ported from zbot).

**Order:** T1 (foundation) → T2 (vector) → T5 (hybrid search end-to-end) → T3 (MMR) → T4 (cross-encoder).

## Constraints

- **RFC-0019** — design authority; supersedes RFC-0018 §6.2 + D2.
- **ADR-0022 / retrieval-port neutrality** — config + reranker-strategy are engine-neutral; MMR/cross-encoder live in adapter crates; the `RetrievalReranker` port is **not** extended (MMR injects an embedder instead).
- **No app-policy in engram** — category/contradiction/KG-decay/episode/intent policy stays in gateway-memory.
- **Backward compatible** — absent config ⇒ equal-weight RRF, no reranker.
- **No god-constructor** — extend `with_reranker` to take the `ReciprocalFusionConfig`; do not add a parallel `with_fusion_config`.

## External fusion-config design (the user's ask)

```jsonc
// [recall_fusion] section of the launch profile, OR .engram/recall.json
{
  "rrf_k": 60,
  "default_source_weight": 1.0,
  "source_weights": {                 // keys = normalized lane tags (post-T1)
    "vector": 0.7, "lexical": 0.3, "associative_graph": 0.5,
    "graph": 0.4, "community_summary": 0.4, "temporal": 0.3,
    "facts": 1.0, "belief": 0.8
  },
  "rerank": { "strategy": "mmr", "lambda": 0.5 }   // "none" | "mmr" | "cross_encoder"
}
```

- **Types** (serde, `engram-retrieval::config`): `RecallFusionConfig`, `RerankConfig { strategy, lambda }`, `RerankStrategy::{None,Mmr,CrossEncoder}`; a `to_reciprocal_config()` builder using the validated `ReciprocalFusionConfig::new`.
- **Loading** (`core/integration/src/config.rs`): `recall_fusion: Option<RecallFusionConfig>` on the profile + `EngramConfig`; resolve `[recall_fusion]`, else discover `.engram/recall.json` (the `scan.json` ladder).
- **Wiring** (`SqlUnifiedRecall`): extend `with_reranker(memory, lanes, beliefs, fusion: ReciprocalFusionConfig, reranker)`; `recall()` builds `ReciprocalRankFusion::new(fusion)`. `new()` keeps `ReciprocalFusionConfig::default()` + `None`. `bootstrap.rs` reads the config, selects the reranker, calls it. Absent ⇒ default + none.
- **Lane source tags are the weight keys** — T1 normalizes them to the vocabulary above.

## Construction tests

Per-task `Tests:` below. Cross-cutting:
**Integration:** T1 — a weighted config reorders recall vs equal-weight; T5 — a multi-term `search` regression test through hybrid recall.
**Manual verification:** T2 — `--features fastembed` build's `capability_report` shows vector + vector candidates appear; T3/T4 — each rerank strategy changes output ordering.

## Design (LLD)

### Design decisions
- **Weighted RRF via existing `ReciprocalFusionConfig`** (`weight/(k+rank)`), not a new algorithm. Traces to AC "honors configured weights".
- **Reranker = strategy enum dispatched in bootstrap** (none/mmr/cross_encoder); `SqlUnifiedRecall` stays reranker-agnostic. Traces to ACs D2.1b/c.
- **MMR injects an `EmbeddingProvider`** (the `RetrievalReranker` port exposes no embeddings); embeds candidate texts per call (top-K, bounded). Traces to AC D2.1b.
- **Vector opt-in by feature**, config sets its weight. Traces to AC "vector activatable".
- **Lane-tag normalization is a flagged one-time rename** (consumed by traces/logs/tests). Traces to AC "normalized vocabulary".

### Interfaces & contracts
- New: `RecallFusionConfig` / `RerankConfig` / `RerankStrategy` (serde, `engram-retrieval`); `contracts/v1/schemas/recall-fusion.schema.json`; `MmrReranker` (`adapters/retrieval/mmr-rerank`).
- Extended: `EngramConfig.recall_fusion`; `SqlUnifiedRecall::with_reranker` (takes `fusion`).
- Reused unchanged: `ReciprocalFusionConfig`, `ReciprocalRankFusion::new`, `CrossEncoderRerankerAdapter`, `RetrievalReranker`, `compose_context`.

### Behavior & rules
- Weighted RRF: `Σ_sources source_weight(source)/(k+rank)`. Default weights 1.0 (pure RRF) when config absent.
- Reranker runs between fusion and budget (composer already does this).
- Validation: `k>=1`; weights finite & ≥0 (zero = visible but scoreless). Invalid ⇒ `CoreError::InvalidRequest`.

### Failure, edge cases & resilience
- Lane `Err` already degrades (`source_failures`); unchanged. Vector off ⇒ its weight inert. MMR with no embedder available ⇒ degrade to relevance-only (documented; not a panic).

## Tasks

### T1: Fusion config + weighted wiring + lane-tag normalization (D2.1a-core)

**Depends on:** none

**Tests:**
- `RecallFusionConfig` validation rejects `k=0`, negative/NaN weights (`CoreError::InvalidRequest`).
- Weighted config (`vector=0.7`,`lexical=0.3`) reorders recall vs equal-weight (integration over seeded lanes). Absent config ⇒ equal-weight default.
- Each lane stamps its **normalized** tag (`vector`, `lexical`, …); traces/tests carry the new names.

**Approach:**
- `core/retrieval/src/config.rs`: serde `RecallFusionConfig` + `RerankConfig` + `RerankStrategy` + `to_reciproval_config()`.
- **Normalize lane source tags** so every lane stamps its vocabulary key (the weight key): `vector.semantic`→`vector`, `lexical.keyword`→`lexical`, `sql.memory.keyword`/`sql.memory.cue`/`sql.memory.keyword+cue`→`facts`, **add** a `FusionTrace { source: "temporal", .. }` to the temporal lane (currently `None`, collapsed to `unknown` at `reciprocal.rs:195` — an *addition*, not a rename), confirm `belief`. Update trace/log/test consumers.
- `core/integration/src/config.rs`: `recall_fusion` on profile + `EngramConfig`; `[recall_fusion]` + `.engram/recall.json` discovery. **`EngramConfig` derives `Eq` (config.rs:157); `f32` weights/`lambda` can't** — drop `Eq` from `EngramConfig` (audit call sites) or use an `Eq`-safe newtype.
- `core/integration/src/sqlite/recall.rs`: extend `with_reranker(.., fusion, reranker)`; `recall()` uses `ReciprocalRankFusion::new(self.fusion.clone())`.
- `core/integration/src/sqlite/bootstrap.rs`: read config → reciprocal config + reranker (none for now) → `with_reranker`.
- Land `contracts/v1/schemas/recall-fusion.schema.json`.
- **pgvector** (`backends/pgvector/src/recall.rs:59`) hard-codes the same default fusion + `reranker=None`; wiring it is **deferred** (not the default backend) — see `docs/backlog.md` `pgvector-recall-fusion`. SQLite (default) is in scope.

**Done when:** `cargo test` green; a weighted config reorders recall (incl. the facts lane); schema lands; lane tags normalized; `EngramConfig` compiles.

### T2: Vector lane activation (D2.1a-vector)

**Depends on:** T1

**Tests:** (goal-based) `cargo build --features fastembed -p engram-mcp`; `capability_report` shows vector; recall returns vector candidates fused at the configured weight.

**Approach:** keep `fastembed` non-default; the lane/embedder/scan embedding already exist — verify against the new weight + `capability_report`. Document the opt-in build.

**Done when:** fastembed build's recall includes vector candidates at the configured weight.

### T3: MMR reranker via injected embedder (D2.1b)

**Depends on:** T1

**Tests:**
- MMR demotes a near-duplicate vs pure relevance; `lambda` trades relevance/diversity; `None`-embedding graceful.
- The injected `EmbeddingProvider` is exercised (not the port).

**Approach:** new `adapters/retrieval/mmr-rerank/` implementing `RetrievalReranker`, holding an `EmbeddingProvider`; port zbot's `mmr_select`. `bootstrap` wires `MmrReranker` when `rerank.strategy=="mmr"`.

**Done when:** `cargo test` green; MMR selectable + effective.

### T4: Cross-encoder wiring (D2.1c)

**Depends on:** T1

**Tests:** (goal-based) build with the cross-encoder feature; `rerank.strategy=="cross_encoder"` wires `CrossEncoderRerankerAdapter` and re-scores.

**Approach:** wire the built adapter behind its feature gate in `bootstrap`, dispatched on strategy.

**Done when:** cross-encoder selectable via config.

### T5: search via full-hybrid recall (D2)

**Depends on:** T1

**Tests:** a **regression test** (not manual) — multi-term `search` returns ranked hits through hybrid recall; the `.contains()` loop is gone. Use a stubbed recall handle (existing fixture pattern) or a minimal `App` fixture.

**Approach:** `mcp/engram-mcp/src/codegraph.rs::search` routes through `provider.recall(...)` + entity-id resolution; remove substring loop.

**Done when:** multi-term query returns ranked results via hybrid recall; regression test green.

## Rollout

Additive/config-driven + one new adapter crate + one JSON schema. Reversible (absent config ⇒ today's behavior). Vector + cross-encoder opt-in features. No schema migration, no infra. ADR for the `[recall_fusion]` contract filed with the schema.

## Risks

- **Weight tuning is deployment-specific** — ship defaults + mechanism; document the vocabulary + example. (Mitigation: schema + README.)
- **Lane-tag rename breaks trace/test consumers** — T1 flags it as a one-time migration. (Mitigation: update all consumers; grep-verify.)
- **MMR re-embed cost** — top-K only; cache as a follow-on if profiling shows cost.
- **fastembed-off surprises users** — `capability_report` + README make vector presence explicit.

## Changelog

- 2026-08-01: initial plan — split out of `codegraph-retrieval-fixes` per the pre-EXECUTE review (this work is repo-wide recall, not codegraph). Corrections applied: lane-tag normalization is real work incl. the facts lane + a temporal FusionTrace *addition* (T1), MMR injects an embedder (T3), `with_reranker` takes the fusion config (no god-constructor — note: its signature changes, single internal caller `SqlUnifiedRecall::new`, no external break), contract schema lands under `contracts/v1/schemas/` (T1), T5 has a regression test, EngramConfig-`Eq` resolution flagged (T1), pgvector recall wiring deferred to backlog. Constrained by RFC-0019.
