# RFC-0019: Hybrid recall fusion + externally configurable ranking

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-08-01
- **Date closed:**
- **Decision weight:** standard
- **Related:** supersedes RFC-0018 §"Why BM25 not vector+reranking?" + its D2; ADR-0022 (engine neutrality); ADR for the `[recall_fusion]` config contract (to be filed). Evidence: the zbot-vs-engram recall gap analysis (in-session; summarized in `docs/specs/recall-fusion-config/plan.md`).

## Reviewer brief

- **Decision:** Approve making engram's unified recall a full hybrid retriever — vector + BM25 + RRF + reranking — with **externally configurable fusion weights and reranker strategy**, and formally retract RFC-0018's deferral of this work.
- **Recommended outcome:** accept.
- **Change if accepted:** a `[recall_fusion]` config contract (RRF `k`, per-lane `source_weights`, `rerank` strategy); lane source-tag normalization; `SqlUnifiedRecall` honors configured weights; an `MmrReranker` adapter; cross-encoder wiring; MCP `search` routes through hybrid recall. Vector stays an opt-in build feature.
- **Affected surface:** `engram-retrieval` (config types), `engram-integration` (`EngramConfig` + `SqlUnifiedRecall` + bootstrap), new `adapters/retrieval/mmr-rerank`, `engram-mcp` (search), `contracts/v1/schemas/recall-fusion.schema.json`. Engine-neutral; no storage/domain change.
- **Stakes:** reversible (config-driven; absent config ⇒ today's behavior) except the lane-tag normalization (a one-time rename consumed by traces/tests).
- **Review focus:** (1) the lane-tag normalization is real work, not wiring — does the chosen vocabulary hold? (2) MMR via an injected `EmbeddingProvider` (not the `RetrievalReranker` port) — is the cost acceptable?
- **Not in scope:** zbot product-policy techniques (category weights, contradiction penalty, KG decay, episodes, intent boost) — these stay in gateway-memory; porting them into engram would violate engine neutrality + no-god-module.

## The ask

- **Recommendation (BLUF):** Approve full hybrid recall for engram — wire the dormant weighted-fusion + reranker machinery that engram already half-built, expose it through an external `[recall_fusion]` config, and retract RFC-0018's "vector+reranking is out of scope" deferral. This is mostly wiring existing code; the genuinely new bits are one config contract, one ~45-line MMR adapter (ported from zbot), and a lane-tag normalization.

- **Why now (SCQA):**
  - *Situation:* RFC-0018 deliberately deferred vector+reranking as "a larger, separate initiative." A subsequent zbot-vs-engram gap analysis showed engram's recall is, by default, BM25+graph+PPR+facts+temporal+beliefs with **no vector, no rerank, equal-weight fusion** — while the weighted-fusion API (`ReciprocalFusionConfig`), the cross-encoder adapter, and the vector lane all exist but lie dormant.
  - *Complication:* the machinery is ~80% built but disconnected; `SqlUnifiedRecall` hard-codes `ReciprocalRankFusion::default()` and `reranker=None`; vector is feature-gated off; lane source tags are inconsistent (`vector.semantic` vs `vector`).
  - *Question:* how do we make engram a competent generic full-hybrid retriever with operator-tunable ranking, without pulling zbot's application-specific policy into neutral layers?

- **Decisions requested:**

  | ID | Question | Recommendation | Why | Decide by | Reviewer action |
  | --- | --- | --- | --- | --- | --- |
  | D1 | Config contract for ranking? | A serde `[recall_fusion]` section (`rrf_k`, `default_source_weight`, per-lane `source_weights`, `rerank`) on `EngramConfig`, discovered as `[recall_fusion]` or `.engram/recall.json`. | Operators tune vector/BM25/graph weights without code changes (zbot's `vector_weight`/`bm25_weight` pattern, engram-neutral). | This review | Confirm the contract surface + JSON-schema artifact. |
  | D2 | Reranker selection? | A `rerank.strategy` enum: `none` \| `mmr` \| `cross_encoder`, dispatched in bootstrap. | Reuses the `RetrievalReranker` port + composer's existing rerank slot. | This review | Confirm. |
  | D3 | Vector activation? | Opt-in Cargo feature (`fastembed`), not default-on; the config sets its *weight*. | Model download + embed cost is a deployment decision. | This review | Confirm opt-in (not default-on). |
  | D4 | MMR's embedding source? | `MmrReranker` **injects an `EmbeddingProvider`** and embeds candidate texts per call (the `RetrievalReranker` port exposes no embeddings). | Keeps the port unchanged; candidate sets are top-K (bounded cost). | This review | Confirm injected-embedder over extending the port/`RetrievalResult`. |
  | D5 | Lane source-tag vocabulary? | Normalize to stable short names (`vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`) — rename the stamped strings. | Weighted config is keyed by these; the current mixed strings (`vector.semantic`, `unknown`, …) would make weights silently inert. | This review | Confirm the vocabulary + one-time rename. |
  | D6 | App-policy fence? | zbot product-policy (category weights, contradiction penalty, KG decay, episodes, intent boost) stays in gateway-memory. | Engine neutrality + no-god-module; those are application epistemics, not generic IR. | This review | Confirm the fence. |

## Problem & goals

Out-of-box engram recall is not full hybrid: vector off, reranker unwired, equal-weight fusion. zbot's gateway-memory *is* full hybrid but is a separate, application-specific pipeline over its own store — not reusable as engram's generic recall. The goal: engram's own recall becomes a competent generic full-hybrid retriever with operator-tunable ranking, closing the IR gap while leaving application policy to consumers.

**Goals.** (1) Weighted vector+BM25+graph fusion via external config; (2) pluggable MMR + cross-encoder reranking; (3) opt-in vector; (4) backward compatible (absent config ⇒ today's behavior).

**Non-goals.** Porting zbot's category/contradiction/KG-decay/episode/intent policy into engram; making vector default-on; changing the storage schema or domain types; a new fusion algorithm (reuse weighted RRF).

## Proposal

Cascade per decision (see `docs/specs/recall-fusion-config/plan.md` for the task DAG): keystone is the config contract (D1) + lane-tag normalization (D5); then `SqlUnifiedRecall` honors the config (extend `with_reranker` to take the `ReciprocalFusionConfig`); then MMR (D4) + cross-encoder wiring (D2); then MCP `search` routes through hybrid recall (superseding RFC-0018's BM25-only D2).

## Options considered

- **Fusion:** weighted RRF (reuse `ReciprocalFusionConfig`) — recommended; vs weighted-sum (`WeightedFusionConfig`, also exists) — rejected (RRF is rank-based, less score-scale-sensitive); vs new algorithm — rejected (YAGNI).
- **MMR embeddings:** injected `EmbeddingProvider` — recommended; vs extending `RetrievalReranker`/`RetrievalResult` to carry vectors — rejected (domain-truth change for one adapter); vs text-overlap "diversity" — rejected (not real MMR).
- **Vector:** opt-in feature — recommended; vs default-on — rejected (heavy dep); vs external provider only — deferred (fastembed suffices for now).
- **Do-nothing:** engram recall stays non-hybrid — cost = the gap the user flagged persists.

## Risks & what would make this wrong

- *Weight tuning is deployment-specific.* Engram ships sane defaults + the mechanism; deployments tune. Mitigation: documented vocabulary + example config.
- *Lane-tag rename breaks trace/test consumers.* Mitigation: D5 is a flagged one-time migration with a note; traces/logs updated.
- *MMR re-embed cost.* Mitigation: top-K only; cache embeddings where feasible.
- *fastembed-off surprises users.* Mitigation: `capability_report` + README make vector presence explicit.
- Drawback: this is more surface than RFC-0018's narrow D2 — accepted, because the narrow fix wouldn't deliver hybrid retrieval.

## Evidence & prior art

- **Gap analysis (in-session, four-agent + zbot mapping):** engram recall wiring (`core/integration/src/sqlite/{recall,bootstrap}.rs`); dormant weighted-fusion (`core/retrieval/src/{config,reciprocal,weighted}.rs`); built-but-unwired cross-encoder (`adapters/retrieval/cross-encoder-rerank`); zbot's own recall (`agentzero/gateway/gateway-memory/src/recall/{mod,mmr}.rs`) + its template (`zbot-recommended-v1-recall.json`).
- **Repo precedent:** RFC-0018 (the deferral this retracts), ADR-0022 (neutrality), RFC-0005 (backend-agnostic retrieval composition — the lanes this builds on).
- **External prior art:** weighted RRF and MMR are standard hybrid-IR techniques (zbot's implementation is the in-repo reference). No unfetched citations.

## Open questions

1. **Schema artifact path** — `contracts/v1/schemas/recall-fusion.schema.json`; landed with D1 or deferred with a backlog anchor. · recommended default: land a minimal schema with D1. · owner: implementer · decide-by: spec.
2. **Embedding cache for MMR** — per-call re-embed first, cache as a follow-on if profiling shows cost. · owner: implementer · decide-by: T6.
3. **`.engram/recall.json` discovery vs `[recall_fusion]` only** — recommended default: support both (same ladder as `scan.json`). · owner: implementer · decide-by: spec.

## Follow-on artifacts

- Spec: `docs/specs/recall-fusion-config/` (D2.1a/b/c + reframed D2).
- ADR: the `[recall_fusion]` config contract (filed when the schema lands).
- Erratum on RFC-0018 recording that its §6.2 deferral + D2 are superseded by this RFC.
