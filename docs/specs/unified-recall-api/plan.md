# Plan: Unified recall API (S4)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn.

## Approach

S4 exposes the existing retrieval-composition machinery behind one provider handle.
Four sequential moves:

1. **Port.** `UnifiedRecall` in `core/integration/src/recall.rs` — `async fn recall(RetrievalRequest) -> CoreResult<ContextPayload>`, reusing existing types.
2. **SQLite impl.** `SqlUnifiedRecall` in `adapters/integration/src/recall.rs`, composing `MemoryService` + the `RetrievalIndex` lanes + `BeliefRepository`. Per request: collect `RetrievalResult` candidates from each available lane (facts via `retrieve`; graph/vector/lexical via `RetrievalIndex`; beliefs via `BeliefQuery` wrapped as a result), record lane failures as `RetrievalSourceFailure`, then `compose_context(ReciprocalRankFusion, candidates, source_failures)` → `ContextPayload`.
3. **Provider + capability.** `EngramProvider` gains a `recall: Option<Arc<dyn UnifiedRecall>>` handle + `recall()` accessor; `bootstrap_provider` constructs `SqlUnifiedRecall`, runs the fixture, flips `unified_recall` to `Supported` on pass.
4. **Conformance fixture + gate.** Multi-lane recall → fused payload + trace; forced lane failure degrades; adds `core/integration/src/recall.rs` to the neutrality gate.

Strictly sequential (T1→T2→T3→T4). Riskiest part is wiring beliefs into the candidate shape + keeping the port engine-neutral while the impl composes `Sql*`/`RetrievalIndex` — mitigated by reusing `compose_context` + the gate (covering `recall.rs` after T1).

## Constraints

- **ADR-0022** — the `UnifiedRecall` port (`core/integration/src/recall.rs`) must not name an engine type or hold SQL; the SQLite impl lives in `adapters/integration`.
- **ADR-0009** — the retrieval-composition seam: reuse `RetrievalIndex` (per source) → `RetrievalFusion` (`ReciprocalRankFusion`) → `ContextComposer`; do not reimplement fusion.
- **`rust-crate-integration` (Implementing)** — owns the facade; S4 extends it with one new handle + capability flip, additive only.
- **S1** — owns the `unified_recall` capability key (`Unsupported { FeatureDisabled }`); S4 flips it.
- **`docs/architecture/reference.md`** — typed errors (`CoreResult`); the read path is backend-agnostic behind ports (ADR-0009).
- **No new result type** — reuse `ContextPayload` / `RetrievalResult` / `FusionTrace` / `RetrievalSourceFailure`.

## Construction tests

**Integration tests:** the conformance fixture (T4) is the cross-cutting integration test — multi-lane recall fuses into one payload with trace; a forced lane failure is recorded in `source_failures` without aborting. Per-task unit tests cover the port contract (T1) and the lane-fan-out + degraded aggregation (T2).

**Manual verification:** from a bootstrapped provider, issue a recall across stores seeded with facts + graph + beliefs and confirm a single fused `ContextPayload`.

## Design (LLD)

Conforms to `docs/architecture/reference.md` (retrieval-composition seam, ADR-0009). `Shape: service` → `Interfaces & contracts`, `Failure, edge cases & resilience`.

### Design decisions

- **Expose, don't rebuild.** The fusion (RRF), composer, lanes, trace, and degraded-mode (`source_failures`) all exist; S4 unifies them behind one handle. Traces to: AC1, AC2.
- **Beliefs as a 0-or-1 lane.** `BeliefRepository::get_belief(query) -> Option<Belief>` returns at most one belief per query; it wraps into a single `RetrievalResult` candidate (subject-scoped) and fuses on rank alongside the others (RRF over a singleton is degenerate but well-defined). Traces to: AC2.
- **Facts lane = already-composed reuse.** `MemoryService::retrieve` returns a fully-composed `ContextPayload`, not candidates. The impl feeds that payload's `items` as the facts lane's candidates and **merges its `source_failures`/`omitted` into the outer payload**; `compose_context` keeps all candidates (truncation disabled), so there is no double-budget. Traces to: AC2.
- **Degraded, not aborted.** A lane that errors or is unavailable is recorded in `source_failures`; the recall still returns the other lanes fused. **All lanes failing → `Ok` with empty items + one `source_failures` per lane** (degraded success, not `Err`). Traces to: AC2, AC4.
- **Reuse ContextPayload.** No new result type; the existing `ContextPayload` already carries items + trace + source_failures. Traces to: AC1.
- **Taxonomy + episodes deferred.** No `expand_terms` port; S2 `ProvenanceQuery` is a provenance read of a different shape — both deferred (backlog anchor). Traces to: AC6.

### Interfaces & contracts

`UnifiedRecall` trait (`core/integration/src/recall.rs`):
- `async fn recall(&self, request: RetrievalRequest) -> CoreResult<ContextPayload>;`

Reuses `RetrievalRequest` / `ContextPayload` from `engram_domain`/`engram_retrieval`. Traces to: AC1.

### Failure, edge cases & resilience

- A lane `Err` → push a `RetrievalSourceFailure { source, reason }`, continue to the next lane; the recall returns `Ok(ContextPayload)` with the surviving lanes fused.
- A lane with no candidates contributes nothing (not a failure).
- All lanes failing still returns `Ok` with an empty items list + populated `source_failures` (degraded, not an error) — the host reads `source_failures` to see why.

## Tasks

### T1: UnifiedRecall port

**Depends on:** none · **Mode:** goal-based (the trait shape is compiler-verified; the real invariants — degraded mode, fusion reuse, lane fan-out — are exercised in T2/T4 against the actual impl)

**Tests:**
- no stub (mode): port compiles, is re-exported from `lib.rs`, and `core/integration/src/recall.rs` passes the neutrality gate. (AC1, AC5)

**Approach:**
- Add `core/integration/src/recall.rs` with the `UnifiedRecall` trait; re-export from `lib.rs`.
- Add `core/integration/src/recall.rs` to `.codex/hooks/check-engine-neutrality.sh` `GATED_PATHS`.

**Done when:** port compiles, re-exported, gate green (+ AC7).

### T2: SqlUnifiedRecall impl

**Depends on:** T1 · **Mode:** TDD

**Tests:**
- Against in-memory stores seeded with facts + graph + vector + lexical + beliefs: a recall returns a `ContextPayload` whose items span the lanes with `FusionTrace`. (AC2)
- Forcing one lane to fail yields a `source_failures` entry while the other lanes' items still appear; the recall returns `Ok` (degraded, not error). (AC2, AC4)

**Approach:**
- Add `adapters/integration/src/recall.rs` with `SqlUnifiedRecall { memory, retrieval_lanes, beliefs }`. Per request: collect `RetrievalResult` candidates from each lane (memory `retrieve`; `RetrievalIndex` lanes; `BeliefQuery` → wrapped result), push `RetrievalSourceFailure` on any `Err`, then `compose_context(ReciprocalRankFusion, candidates, source_failures)`.

**Done when:** impl tests green; `core/integration` still passes the neutrality gate (+ AC7).

### T3: Provider handle + capability flip + conformance fixture

**Depends on:** T2 · **Mode:** TDD

**Tests:**
- `EngramProvider` exposes `recall() -> Option<&Arc<dyn UnifiedRecall>>`; a provider with `SqlUnifiedRecall` reports `unified_recall` `Supported`, an unwired one `Unsupported { FeatureDisabled }`. (AC3)
- The conformance fixture `run_recall_fixture()` asserts: multi-lane recall → fused `ContextPayload` spanning lanes; one lane failing → `source_failures` + other lanes' items still present; **all lanes failing → `Ok` with empty items + one `source_failures` per lane** (not `Err`). (AC4)

**Approach:**
- Add the `recall` field + builder method + accessor to `EngramProvider`/`EngramProviderBuilder` (`core/integration/src/provider.rs`), mirroring existing handles.
- Add the conformance fixture (`adapters/integration/src/fixtures/recall.rs`) covering the three cases above; register in `fixtures/mod.rs`.
- In `bootstrap_provider`, construct `SqlUnifiedRecall`, gate the handle + capability flip on `fixtures::recall::run_recall_fixture().is_ok()`, and mark `unified_recall` `Supported` only on pass (start at `failed()`/ConformanceFailed). T3 is self-sufficient — the fixture it gates on is the one it lands.

**Done when:** handle + flip tests green; fixture green (+ AC7).

### T4: Neutrality gate + deferred-lanes backlog anchor

**Depends on:** T3 · **Mode:** goal-based check

**Tests:**
- `core/integration/src/{provider,capability,provenance,batch,recall}.rs` pass `.codex/hooks/check-engine-neutrality.sh`. (AC5)
- `docs/backlog.md` has the `## unified-recall-taxonomy-episodes` anchor resolving the spec's deferred marker. (AC6)

**Approach:**
- Confirm the gate covers `core/integration/src/recall.rs` (added in T1) and is green.
- Add `docs/backlog.md` section `## unified-recall-taxonomy-episodes` (taxonomy expansion needs an `expand_terms` port; episodes/evidence lane is a different shape).

**Done when:** gate green; backlog anchor present (+ AC7).

## Rollout

- **Delivery:** additive Rust API (one new port trait + handle + capability flip) + one conformance fixture. No flag, no migration, fully reversible.
- **Deployment sequencing:** T1→T2→T3→T4 strictly. Per-task regression bar (AC7).

## Risks

- **Port/impl boundary leakage.** The impl composes `Sql*`/`RetrievalIndex`; an accidental engine reference in the port trips the gate (covering `recall.rs` after T1).
- **Belief-lane candidate shape.** Wrapping `Belief` into a `RetrievalResult` may need a source tag + score convention; reuse the existing `RetrievalResult` fields, don't extend the type.
- **Degraded-mode ambiguity.** All-lanes-fail returns `Ok` with `source_failures`; ensure the fixture + docs make clear that's degraded success, not error.

## Changelog

- 2026-07-10: initial plan (S4 of engram-host-sdk brief; conforms to ADR-0009 + ADR-0022 + reference.md; v1 lanes + deferred taxonomy/episodes per user confirmation).
- 2026-07-10: spec-mode review fixes — pin the facts-lane reuse semantics (memory `retrieve` returns an already-composed `ContextPayload`; its `items` feed the outer fusion as candidates and its `source_failures`/`omitted` merge into the outer payload; `compose_context` keeps all so no double-budget) (Major 1); add an AC + fixture case for all-lanes-fail → `Ok` (Major 2); commit beliefs to the 0-or-1 `get_belief` shape (Major 3); re-mode T1 as goal-based (the trait shape is compiler-verified; drop the narcissistic stub) (Minor 4); weaken AC2's "every item carries fusion_trace" to "where RRF produces it" (Minor 5); fold the conformance fixture into T3 so T3 is self-sufficient and T4 is gate + backlog (Minor 6).
