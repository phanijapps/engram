# Spec: Unified recall API (S4)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality), ADR-0009 (retrieval composition seam), [`rust-crate-integration`](../rust-crate-integration/spec.md) (the Implementing facade S4 extends), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — owns the `unified_recall` capability key S4 flips)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S4, capability #12)
- **Contract:** none — a Rust port trait (`UnifiedRecall`) + provider handle, reusing existing retrieval types.
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A host issues **one recall query** through `EngramProvider` that fans across the
available semantic lanes — facts (memory), graph, vector, and lexical (via the
existing `RetrievalIndex` lanes), and beliefs (via `BeliefQuery`) — and fuses them
through the existing `ReciprocalRankFusion` + `ContextComposer` into one
`ContextPayload`, carrying per-result ranking trace (`FusionTrace`) and per-lane
degraded-mode reporting (`source_failures`). The `unified_recall` capability is
reported `Unsupported { FeatureDisabled }` today; S4 ships the machinery to make
it `Supported` behind a `UnifiedRecall` provider handle, but the production
handle attachment + flip are deferred (see below).

v1 lanes are facts, graph, vector, lexical, and beliefs. **Taxonomy-expanded
terms** (no `expand_terms` port exists) and **episodes/evidence** (the S2
`ProvenanceQuery`, a provenance read of a different shape) are **deferred** —
reported as not-yet-wired lanes, not silently absent. The result reuses the
existing `ContextPayload`; no new result type is introduced.

S4 ships the **machinery** — the `UnifiedRecall` port, the `SqlUnifiedRecall`
implementation, and a conformance fixture, all tested — but the **production
handle attachment + `Supported` flip are deferred**: the vector lane needs an
embedding provider constructed in `bootstrap_provider` (the default build has
none) and the lexical lane needs `engram-conformance` to depend on
`engram-store-lexical` (it does not today). Until both lanes are wirable, the
bootstrapped provider keeps `recall() == None` and `unified_recall`
`Unsupported { FeatureDisabled }` — a partial unified recall missing the two
core retrieval lanes would overclaim, so none is shipped.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Fan the query across the v1 lanes — facts via `MemoryService::retrieve`, graph/vector/lexical via the existing `RetrievalIndex` lanes, beliefs via `BeliefQuery` — and fuse via the existing `ReciprocalRankFusion` + `compose_context`.
- Reuse `RetrievalRequest`, `ContextPayload`, `RetrievalResult`, `FusionTrace`, `RetrievalSourceFailure` — do not invent a new result type.
- On a lane error or unavailability, record a `RetrievalSourceFailure` and continue with the remaining lanes (degraded); never fail the whole recall for one lane.
- Surface the ranking trace (`FusionTrace` on each `RetrievalResult`).
- Keep the `UnifiedRecall` port in `core/integration` (engine-neutral); put the SQLite impl in `adapters/integration`.
- Flip `unified_recall` to `Supported` only when the conformance fixture passes; attach the handle only then.

### Ask first

- Add a taxonomy-expansion lane (requires a new `expand_terms` port).
- Add an episodes/evidence lane (S2 `ProvenanceQuery`, a different shape).
- Introduce a new result type over `ContextPayload`.

### Never do

- Reimplement RRF/fusion or the composer — reuse `ReciprocalRankFusion` + `compose_context`. *(structural)*
- Name an engine type (`Sql*`, …) or hold SQL in the `UnifiedRecall` port or `core/integration`. *(structural, ADR-0022)*
- Fail the whole recall when one lane is unavailable — degrade via `source_failures`.
- Bypass the existing `RetrievalIndex` lanes or `MemoryService::retrieve`.

## Testing Strategy

- **UnifiedRecall port — TDD.** An in-memory stub `recall()` returns a `ContextPayload` fusing the lane candidates it is given; when a lane is marked failed, the payload carries a `source_failures` entry and still returns the other lanes' items. Invariant: "one fused payload; failed lanes reported, not aborted."
- **SqlUnifiedRecall impl — TDD.** Against in-memory stores with facts + graph + vector + lexical + belief records: a recall returns a `ContextPayload` whose items span the lanes and carry `FusionTrace`; forcing one lane to fail yields a `source_failures` entry while the other lanes' items still appear (degraded, not error).
- **Provider handle + capability flip — TDD.** A bootstrapped provider exposes `recall()` and reports `unified_recall` `Supported`; an unwired provider reports it `Unsupported` with no handle.
- **Conformance fixture — goal-based.** A multi-lane recall returns a fused `ContextPayload` with trace; a forced lane failure degrades (recorded) without aborting.
- **Engine neutrality — goal-based check.** `core/integration/src/recall.rs` stays green under `.codex/hooks/check-engine-neutrality.sh` (added to `GATED_PATHS`).
- **No regression — goal-based check.** Existing workspace tests stay green; no schema change.

## Acceptance Criteria

- [x] The `UnifiedRecall` port in `core/integration/src/recall.rs` exposes `async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload>`, reusing the existing retrieval types (no new result type).
- [x] A SQLite `UnifiedRecall` implementation fans the query across facts (memory `retrieve`), graph/vector/lexical (the existing `RetrievalIndex` lanes), and beliefs (`BeliefRepository::get_belief`, which returns **at most one** belief per query). It fuses the lane candidates via `ReciprocalRankFusion` + `compose_context` into one `ContextPayload`. The facts lane reuses memory `retrieve`'s already-composed `items` as its candidates and **merges that payload's `source_failures`/`omitted` into the outer payload** (`compose_context` keeps all candidates — no double-budget). On a lane failure, a `RetrievalSourceFailure` is recorded and the recall continues (degraded, not aborted). Items carry `fusion_trace` **where RRF produces it**; presence is not asserted for single-source survivors (RRF's contract, which this spec reuses and must not reimplement).
- [x] When **all** v1 lanes fail, `recall` returns `Ok(ContextPayload { items: [], source_failures: <one entry per lane> })` — degraded success, never `Err`.
- [x] `EngramProvider` exposes a `recall()` handle; the `unified_recall` capability flips to `Supported` only when the conformance fixture passes. ~~Deferred~~ **Met by PR #22** (production wiring: graph+lexical+beliefs lanes wired; vector behind fastembed). the production handle is not attached (`recall() == None`) because the vector lane needs an embedding provider in `bootstrap_provider` and the lexical lane needs `engram-conformance` to depend on `engram-store-lexical`; the port + impl + fixture are shipped, the lane wiring + flip are the follow-up. (deferred: `unified-recall-production-wiring`)
- [x] A conformance fixture runs a multi-lane recall → a fused `ContextPayload` with items spanning lanes; forces one lane to fail → a `source_failures` entry with the other lanes' items still present; forces **all** lanes to fail → `Ok` with empty items + one `source_failures` entry per lane (degraded success, not `Err`).
- [x] `.codex/hooks/check-engine-neutrality.sh` covers `core/integration/src/recall.rs` (added to `GATED_PATHS`); the port layer is engine-symbol-free.
- [ ] v1 lanes are facts, graph, vector, lexical, and beliefs; taxonomy-expansion (no `expand_terms` port) and episodes/evidence (different shape) are deferred. (deferred: `unified-recall-taxonomy-episodes`)
- [x] SQLite behavior for existing operations is unchanged; existing workspace tests green.

## Assumptions

- Technical: retrieval composition machinery exists — `RetrievalIndex` (lane), `RetrievalFusion`/`ReciprocalRankFusion`, `ContextComposer`/`compose_context(RetrievalCompositionInput { request, fusion, candidates: Vec<RetrievalResult>, source_failures, … }) -> ContextPayload` (source: `core/retrieval/src/{ports,reciprocal,composer}.rs`).
- Technical: `MemoryService::retrieve(RetrievalRequest) -> ContextPayload` is the facts lane (source: `core/memory/src/lib.rs:76`).
- Technical: ranking trace + degraded mode already modeled — `RetrievalResult.fusion_trace: Option<FusionTrace>`, `ContextPayload.source_failures` (source: `core/domain/src/retrieval.rs`).
- Technical: beliefs are queryable — `BeliefQuery` + `BeliefRepository::get_belief -> Option<Belief>` (source: `core/belief/src/{query,lib}.rs`).
- Technical: no taxonomy `expand_terms` port exists; `EngramProvider` has the `unified_recall` capability key (`Unsupported { FeatureDisabled }`) but no handle (source: grep; `core/integration/src/provider.rs`).
- Product: v1 lanes = {facts, graph, vector, lexical, beliefs}; taxonomy-expansion + episodes/evidence deferred. (source: user confirmation 2026-07-10)
- Design: degraded mode = skip-and-report (`source_failures`), not fail-whole; reuse `ContextPayload`; port in `core/integration`, impl in `adapters/integration`. (source: user confirmation 2026-07-10; my recommendation accepted)
- Process: SQLite only; the port stays engine-neutral (ADR-0022); additive only; reuse the existing fusion/composer/lanes (ADR-0009).
