# Plan: reflection-operator

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** the implementation strategy. May change as you learn; note
> substantive changes in the changelog.

## Approach

Ship reflection as a new engine-neutral **behavior** crate `engram-reflection`
at `core/reflection/` (not `adapters/` — it owns no infrastructure; it depends
only on port crates, like `core/consolidation`). It fills two empty slots: a
production `BeliefSynthesizer` (the reflection synthesizer, deterministic
baseline) and a production `ConsolidationMutationExecutor` (dispatches
`BeliefSynthesis` → synthesizer → `BeliefRepository::put_belief`). Zero contract
change — reuses `BeliefSynthesizer`, `BeliefSynthesis`, and free-form
`provenance.method = "reflection"`. The real LLM impl is deferred behind the
trait (deterministic baseline, mirroring `OverlapScorer`). Production wiring is
a follow-up — it needs a composite-executor pattern (the `Hybrid` strategy
bundles 8 task kinds; a single-purpose executor alone would skip 7).

## Constraints

- AGENTS.md engine-neutrality — no LLM provider/model/store/SQL in `core/` or
  this crate; synthesis behind the `BeliefSynthesizer` trait.
- Zero v1 contract change — reuse the trait / task kind / free-form provenance.
- `Belief` tagging via existing fields only.

## Construction tests

**Unit tests:** `ReflectionSynthesizer` over a stub memory port (active-only,
deterministic, tagged, distinguishable); `ReflectionExecutor` over a stub
`BeliefRepository` (Completed for `BeliefSynthesis`, Skipped w/ reason for
others; task-result count pinned).

## Design (LLD)

### Design decisions

- **Option A — reuse `BeliefSynthesis` + `BeliefSynthesizer` (zero contract
  change).** No competing bottom-up synthesis exists, so reflection defines both
  slots without conflation; distinguished via `provenance.method = "reflection"`.
  First-class `ConsolidationTaskKind::Reflection` is an additive follow-up. Traces
  to: all ACs. *Rejected (follow-up):* the enum variant.
- **Components-only slice (no production wiring).** Wiring needs a composite-
  executor (Hybrid emits 8 task kinds; no executor-chain mechanism today); a
  single-purpose executor alone is unsafe for a Hybrid run. Traces to: AC2.
- **Active-only read.** Reflection abstracts current state, not forgotten/
  expired history: `list_events_for_scope` → `get_memory` → filter
  `MemoryStatus::Active`. Traces to: AC1.
- **Deterministic baseline + deferred LLM** (mirrors `OverlapScorer`). Traces
  to: AC1, AC4.
- **Executor skip-semantics.** One `Completed` task result per `BeliefSynthesis`
  task; one `Skipped` (with reason) per other planned kind. Traces to: AC2.

### Component / module decomposition

- `core/reflection/` (new crate `engram-reflection`):
  - `src/lib.rs` — facade.
  - `src/synthesizer.rs` — `ReflectionSynthesizer` (impl `BeliefSynthesizer`);
    holds `Arc<dyn MemoryEventRepository>` + `Arc<dyn MemoryRepository>`; reads
    active scoped memories; deterministic baseline.
  - `src/executor.rs` — `ReflectionExecutor` (impl
    `ConsolidationMutationExecutor`); holds `Arc<dyn BeliefSynthesizer>` +
    `Arc<dyn BeliefRepository>`.
  - `src/belief_build.rs` — pure helper building a reflection-derived `Belief`
    (mirror `core/belief/src/reconcile.rs::build_belief`).

### Failure, edge cases & resilience

- No active scoped memories → synthesizer returns empty; executor reports
  `beliefs_synthesized = 0` (a `Completed` no-op, not an error).
- `put_belief` failure → recorded in the outcome's `errors`; the task result
  still emits (the gated service downgrades to `CompletedWithErrors` when wired).
- Non-`BeliefSynthesis` planned tasks → one `Skipped` result each (auditable,
  never silently dropped).

## Tasks

### T1: `ReflectionSynthesizer` + active-only read + belief-build (TDD)

**Depends on:** none

**Tests:**
- Over a stub memory port returning fixture memories (incl. a `Forgotten` one),
  `synthesize_beliefs` produces ≥1 belief from the **active** memories only,
  tagged `provenance.method = "reflection"`, `synthesizer.kind = Consolidation`,
  `status = Active`, `reasoning` set; deterministic; empty active set → empty vec.

**Approach:**
- Run `.codex/hooks/pre-implementation-check.sh`; scaffold `core/reflection/`
  (`Cargo.toml` deps: `engram-domain`, `engram-belief`, `engram-memory`,
  `engram-consolidation`, `engram-runtime`, `async-trait`; dev `futures`); add to
  the workspace + AGENTS.md `core/` shape.
- `synthesizer.rs`: inject `MemoryEventRepository` + `MemoryRepository`;
  `list_events_for_scope(scope)` → distinct memory ids → `get_memory` → filter
  `Active` → deterministic baseline (e.g. one summary/lesson belief over the
  active set) via the pure `belief_build` helper.

**Done when:** `cargo check -p engram-reflection` + `cargo test -p engram-reflection`
green; `Cargo.toml` deps match the allowed list (AC4).

### T2: `ReflectionExecutor` (TDD)

**Depends on:** T1

**Tests:**
- Over a stub `BeliefRepository`, a planned `[BeliefSynthesis]` → synthesizer
  called → each belief persisted → one `Completed` task result; a planned
  `[BeliefSynthesis, Compaction]` → one `Completed` (BeliefSynthesis) + one
  `Skipped` (Compaction, with reason), belief count correct; `beliefs_synthesized`
  stat set.

**Approach:**
- `executor.rs`: `ReflectionExecutor { synthesizer, beliefs }`; in `execute`,
  for each `planned_tasks` entry: `BeliefSynthesis` →
  `synthesizer.synthesize_beliefs(request)` → `put_belief` each (capture errors)
  → `Completed` task result with count; else → `Skipped` task result with reason.
  Build `ConsolidationMutationOutcome` with `beliefs_synthesized`.

**Done when:** executor tests pass; `cargo test -p engram-reflection` green.

### T3: Full gates + no-drift + engine-neutrality

**Depends on:** T2

**Tests:** goal-based — `.codex/hooks/pre-implementation-check.sh`;
`cargo fmt --all --check`; `cargo check --workspace`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
`pnpm run typecheck`; `pnpm run contracts:check-generated` (zero drift);
`.codex/hooks/check-contracts.sh`; `.codex/hooks/check-docs.sh`;
`.codex/hooks/check-engine-neutrality.sh`; AND a recursive grep over
`core/reflection/src/` finding no `engram-store-*` / `rusqlite` / `sqlx` /
`tantivy` / LLM-provider import.

**Done when:** every gate is green and the spec's ACs are all checked.

## Rollout

- **Delivery:** additive — a new engine-neutral behavior crate (no live path
  imports it yet; wiring is a follow-up). Fully reversible (remove the crate +
  workspace member). Zero runtime impact until wired.
- **Infrastructure:** none.
- **External-system integration:** none (LLM deferred).

## Risks

- **Active-only read correctness** — `list_events_for_scope` returns all events;
  the filter to `Active` records must exclude `Forgotten`/`Expired`. Mitigated by
  the T1 forgotten-memory fixture.
- **Deterministic baseline ≠ true reflection** — it is a summary placeholder
  proving the mechanism; the real LLM insight-synthesis is the deferred impl.
  Disclosed in the synthesizer doc.
- **Executor not safe as a sole Hybrid executor** — documented; production wiring
  needs the composite-executor follow-up.

## Changelog

- 2026-07-15: initial plan — Option A (zero contract change); relocated to
  `core/reflection/` (engine-neutral behavior, not `adapters/`); components-only
  (no production wiring — needs composite-executor follow-up); active-only read;
  executor skip-semantics pinned (Completed + Skipped w/ reason); deterministic
  baseline, LLM deferred.
