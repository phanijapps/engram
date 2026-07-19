# Spec: Predictive retrieval (deterministic baseline)

- **Status:** Shipped
- **Mode:** light — greenfield contract + a deterministic baseline predictor in one crate. Model-assisted prediction and router wiring are deferred.
- **Contract:** `engram-retrieval::predict::{AgentState, RetrievalHints, PredictiveRetriever}` (new serializable public types + trait).
- **Gap:** `docs/arch_divergence.md` "Predictive retrieval (proactive loading)" — `15%`, entirely absent. Research: `architecture-design-v2.md:511-524` (`predict_context(state: AgentState) → RetrievalHints`).

## Objective

Establish the predictive-retrieval contract and a deterministic baseline predictor, so proactive context hints can be generated from agent state — the foundation the research's `predict_context` prescribes. A real expectation-model / prediction-error predictor is model-assisted and out of scope; this ships the deterministic, dependency-free baseline (the project's standard staging for research-heavy features — cf. dry-run consolidation, deterministic fusion).

## Acceptance Criteria

- [x] **AC1 — contract.** `AgentState` (`task`, `recent_queries`, `recent_target_ids`) and `RetrievalHints` (`queries`, `target_ids`) are serializable public types in `engram-retrieval`.
- [x] **AC2 — port.** `PredictiveRetriever` trait with `async fn predict_context(&self, state: &AgentState) -> CoreResult<RetrievalHints>`.
- [x] **AC3 — deterministic baseline.** `RecentActivityPredictor` implements the trait: predicted `queries` = tokenized `recent_queries` ∪ `task` terms (deduped, deterministically ordered); predicted `target_ids` = deduped `recent_target_ids`. No model/clock/storage dependency.
- [x] **AC4 — gates + tests.** `cargo fmt`/`clippy (--workspace --all-targets -D warnings)`/`test` + `pnpm typecheck` green; tests cover recent-activity prediction, empty state, and determinism.

## Non-goals

- Wiring `RetrievalHints` into the in-memory `retrieve()` path or a query router (follow-up slice).
- Expectation models, prediction-error / surprise signals, hierarchical multi-level prediction (model-assisted, deferred).
- Stopword filtering / NLP on predicted queries (the baseline mirrors the existing `query_terms` tokenizer).

**Scope binding (design note).** `RetrievalHints` and `AgentState` are deliberately scope-agnostic. At wiring time the query router combines hints with a `RetrievalRequest` (which already carries `Scope`); the router binds hints to that scope, so no contract amendment is required to wire predictive retrieval into the scoped retrieve path.

## Testing Strategy

- New tests under `core/retrieval/tests/predict.rs`. Single adversarial pass (user preference).

## Changelog

- 2026-07-01 — spec opened.
