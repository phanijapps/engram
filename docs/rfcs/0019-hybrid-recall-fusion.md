# RFC-0019: Hybrid recall fusion + externally configurable ranking

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-08-01
- **Date closed:**
- **Decision weight:** standard
- **Related:** supersedes RFC-0018 §"Why BM25 not vector+reranking?" + its D2; ADR-0022 (engine neutrality); [ADR-0026](../adr/0026-recall-fusion-config-contract.md) (the `[recall_fusion]` config contract). Evidence: the zbot-vs-engram recall gap analysis (in-session; summarized in `docs/specs/recall-fusion-config/plan.md`).

## Reviewer brief

- **Decision:** Approve making engram's unified recall a full hybrid retriever — vector + BM25 + RRF + reranking — with **externally configurable fusion weights and reranker strategy**, and formally retract RFC-0018's deferral of this work.
- **Recommended outcome:** accept.
- **Change if accepted:** a `[recall_fusion]` config contract (RRF `k`, per-lane `source_weights`, `rerank` strategy); lane source-tag normalization; `SqlUnifiedRecall` honors configured weights; an `MmrReranker` adapter; cross-encoder wiring; MCP `search` routes through hybrid recall. Vector is **default-on for the MCP** (D3 reversed: operators get a working vector lane out-of-the-box) with a runtime `--no-vector` disable and a build-time `--no-default-features` disable.
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
  | D3 | Vector activation? | **Default-on for the MCP** (the `fastembed` cargo feature is in `engram-mcp`'s `default`), with a **runtime disable** (`--no-vector` / `enable_vector = false` skips wiring the vector lane + embedding model at boot) and a **build disable** (`--no-default-features`). The config sets the lane's *weight*. | Reverses the original opt-in choice — operator preference is "works out-of-the-box"; the runtime disable lets a fastembed build still skip the model download/load without a rebuild. | This review | Confirm default-on + runtime/build disable (reverses prior opt-in). |
  | D4 | MMR's embedding source? | `MmrReranker` **injects an `EmbeddingProvider`** and embeds candidate texts per call (the `RetrievalReranker` port exposes no embeddings). | Keeps the port unchanged; candidate sets are top-K (bounded cost). | This review | Confirm injected-embedder over extending the port/`RetrievalResult`. |
  | D5 | Lane source-tag vocabulary? | Normalize to stable short names (`vector`, `lexical`, `graph`, `associative_graph`, `community_summary`, `temporal`, `facts`, `belief`) — rename the stamped strings. | Weighted config is keyed by these; the current mixed strings (`vector.semantic`, `unknown`, …) would make weights silently inert. | This review | Confirm the vocabulary + one-time rename. |
  | D6 | App-policy fence? | zbot product-policy (category weights, contradiction penalty, KG decay, episodes, intent boost) stays in gateway-memory. | Engine neutrality + no-god-module; those are application epistemics, not generic IR. | This review | Confirm the fence. |

## Problem & goals

Out-of-box engram recall is not full hybrid: vector off, reranker unwired, equal-weight fusion. zbot's gateway-memory *is* full hybrid but is a separate, application-specific pipeline over its own store — not reusable as engram's generic recall. The goal: engram's own recall becomes a competent generic full-hybrid retriever with operator-tunable ranking, closing the IR gap while leaving application policy to consumers.

**Goals.** (1) Weighted vector+BM25+graph fusion via external config; (2) pluggable MMR + cross-encoder reranking; (3) vector default-on for the MCP with runtime + build disable (D3 reversed); (4) backward compatible (absent config ⇒ today's behavior).

**Non-goals.** Porting zbot's category/contradiction/KG-decay/episode/intent policy into engram; making vector unconditional (the runtime + build disables remain escape hatches); changing the storage schema or domain types; a new fusion algorithm (reuse weighted RRF).

## Proposal

Cascade per decision (see `docs/specs/recall-fusion-config/plan.md` for the task DAG): keystone is the config contract (D1) + lane-tag normalization (D5); then `SqlUnifiedRecall` honors the config (extend `with_reranker` to take the `ReciprocalFusionConfig`); then MMR (D4) + cross-encoder wiring (D2); then MCP `search` routes through hybrid recall (superseding RFC-0018's BM25-only D2).

## Options considered

- **Fusion:** weighted RRF (reuse `ReciprocalFusionConfig`) — recommended; vs weighted-sum (`WeightedFusionConfig`, also exists) — rejected (RRF is rank-based, less score-scale-sensitive); vs new algorithm — rejected (YAGNI).
- **MMR embeddings:** injected `EmbeddingProvider` — recommended; vs extending `RetrievalReranker`/`RetrievalResult` to carry vectors — rejected (domain-truth change for one adapter); vs text-overlap "diversity" — rejected (not real MMR).
- **Vector:** default-on for the MCP with runtime + build disable — recommended (D3 reversed; operator preference: works out-of-the-box); vs opt-in feature (original D3) — superseded; vs external provider only — deferred (fastembed suffices for now).
- **Do-nothing:** engram recall stays non-hybrid — cost = the gap the user flagged persists.

## Risks & what would make this wrong

- *Weight tuning is deployment-specific.* Engram ships sane defaults + the mechanism; deployments tune. Mitigation: documented vocabulary + example config.
- *Lane-tag rename breaks trace/test consumers.* Mitigation: D5 is a flagged one-time migration with a note; traces/logs updated.
- *MMR re-embed cost.* Mitigation: top-K only; cache embeddings where feasible.
- *Default-on model download surprises users.* Mitigation: the runtime `--no-vector` disable (and build-time `--no-default-features`) let a deployment skip the model load without a rebuild; `capability_report` makes vector presence explicit.
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
- ADR: [ADR-0026](../adr/0026-recall-fusion-config-contract.md) — the `[recall_fusion]` config contract (filed).
- Erratum on RFC-0018 recording that its §6.2 deferral + D2 are superseded by this RFC.
