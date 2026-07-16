# Spec: reflection-operator

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** AGENTS.md (engine-neutrality, surface-parity, no god-modules), the consolidation + belief architecture
- **Brief:** none
- **Contract:** none — reuses the already-declared `BeliefSynthesizer` trait + `BeliefSynthesis` task kind + free-form provenance; no v1 change
- **Reuses:** `engram-belief::BeliefSynthesizer`, `engram-domain::ConsolidationTaskKind::BeliefSynthesis`, `engram-memory::{MemoryEventRepository, MemoryRepository}`, `engram-belief::BeliefRepository`, free-form `Belief.provenance.method`
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it.

## Objective

Slice 1 ships the **reflection components** — a production `BeliefSynthesizer`
(the reflection synthesizer) and a production `ConsolidationMutationExecutor`
that runs it — as tested, engine-neutral units in a new `core/reflection/` crate.
The synthesizer reads a scope's **active** memories and abstracts them into
reflection-derived beliefs (the memory dimension engram covers least — reflection
/ abstraction; today consolidation *compresses* but does not *abstract*). The
executor persists those beliefs via `BeliefRepository::put_belief`. Success: the
synthesizer produces deterministic reflection-derived beliefs from fixture
active memories, the executor dispatches + persists them with the right
task-result shape, and there is zero v1 contract change.

Production wiring into `EngramProvider` / a live `GatedConsolidationService` is
**out of scope** for this slice (see Boundaries): the `Hybrid` consolidation
strategy bundles `BeliefSynthesis` with seven other task kinds, and
`ConsolidationMutationExecutor` has no composition/chain mechanism, so a
single-purpose executor is unsafe as the sole executor for a `Hybrid` run.
Wiring requires a composite-executor pattern — a follow-up slice.

## Boundaries

### Always do

- Implement `BeliefSynthesizer` (the empty `core/belief` slot) as the reflection
  synthesizer: read a scope's **`MemoryStatus::Active`** memories (via injected
  memory ports) and produce ≥1 derived belief; keep the LLM behind the trait
  (deterministic baseline in-tree; real LLM feature-gated / deferred).
- Implement a production `ConsolidationMutationExecutor` that, for each planned
  `BeliefSynthesis` task, calls the synthesizer + persists each returned belief
  via `BeliefRepository::put_belief` (one `Completed` task result); every other
  planned task kind gets one `Skipped` task result with a documented reason.
- Tag reflection-derived beliefs distinguishably without a contract change:
  `status = Active`, `synthesizer.kind = DerivationKind::Consolidation`,
  `provenance.method = "reflection"`, `provenance.source =
  "reflection-synthesizer"`, `reasoning` set.
- Keep the crate engine-neutral: depend only on `engram-domain`, `engram-belief`,
  `engram-memory`, `engram-consolidation`, `engram-runtime`; no LLM provider,
  model, store, or SQL in the crate.

### Ask first

- Production wiring into `EngramProvider` / a live `GatedConsolidationService`
  — deferred; requires a composite-executor pattern (the `Hybrid` strategy emits
  8 task kinds; a single-purpose executor alone would skip 7).
- Adding a first-class `ConsolidationTaskKind::Reflection` + planner branch +
  `DerivationKind::Reflection` (additive follow-up making reflection queryable
  as a distinct task).
- The real LLM reflection synthesizer impl (feature-gated adapter).

### Never do

- Change any v1 contract — reuse `BeliefSynthesizer`, `BeliefSynthesis`, and
  free-form `provenance.method`; no new enum variant, domain field, or schema.
- Put an LLM provider/model, a store, or SQL in `core/` or this crate.
- Mutate memories — reflection READS active memories and WRITES beliefs only.
- Wire the executor as the sole executor for a `Hybrid` run in this slice (it
  would silently skip compaction / fact-extraction / contradiction-detection /
  etc. — that needs the composite-executor follow-up).

## Testing Strategy

- **`ReflectionSynthesizer` (deterministic baseline): TDD** — over a stub memory
  port returning fixture memories (incl. a forgotten one), assert it produces ≥1
  reflection-derived belief from the **active** memories only, tagged
  `provenance.method = "reflection"`, deterministic; and that a non-reflection
  belief in the same scope is distinguishable by `provenance.method`.
- **`ReflectionExecutor`: TDD** — over a stub `BeliefRepository`, assert a
  `BeliefSynthesis` planned task → synthesizer called → each belief persisted →
  one `Completed` task result; a non-`BeliefSynthesis` planned task → one
  `Skipped` task result (with reason) and no belief write.
- **Engine-neutrality + no-contract-change: goal-based** — the crate's
  `Cargo.toml` lists only the allowed port crates (asserted by inspection/grep);
  no `engram-store-*` / `rusqlite` / `sqlx` / `tantivy` / LLM-provider import.

## Acceptance Criteria

- [x] `BeliefSynthesizer` is implemented (the previously-empty slot) as a
  reflection synthesizer that reads a scope's **`Active`** memories via injected
  ports and produces ≥1 derived belief, deterministically (a test asserts this on
  fixture memories and that a `Forgotten` memory is excluded).
- [x] A production `ConsolidationMutationExecutor` emits one `Completed` task
  result for each planned `BeliefSynthesis` task (synthesizer called + each belief
  persisted via `BeliefRepository::put_belief`) and one `Skipped` task result
  (with reason) for each other planned task kind; a test pins the task-result
  count + statuses.
- [x] Reflection-derived beliefs are tagged `status = Active`,
  `synthesizer.kind = DerivationKind::Consolidation`,
  `provenance.method = "reflection"`, `provenance.source =
  "reflection-synthesizer"`, carry `reasoning`, and are distinguishable from a
  non-reflection belief in the same scope by `provenance.method == "reflection"`
  (a test asserts the filter) — all with no contract change.
- [x] The crate is engine-neutral: `Cargo.toml` depends only on `engram-domain`,
  `engram-belief`, `engram-memory`, `engram-consolidation`, `engram-runtime`
  (+ `async-trait`); no LLM provider/model, store, or SQL; deterministic
  baseline, real LLM deferred.
- [x] No v1 contract change: `contracts/v1` regenerates with zero drift.
- [x] All repository gates are green: `cargo fmt --all`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm run typecheck`, `.codex/hooks/check-contracts.sh`, `.codex/hooks/check-docs.sh`,
  `.codex/hooks/check-engine-neutrality.sh`; AND a recursive grep over
  `core/reflection/src/` finding no `engram-store-*` / `rusqlite` / `sqlx` /
  `tantivy` / LLM-provider import.

## Assumptions

- Technical: `BeliefSynthesizer` (`core/belief/src/lib.rs:113-121`) is declared with zero impls — reflection implements it. (source: recon)
- Technical: `ConsolidationMutationExecutor` (`core/consolidation/src/mutating.rs:62-70`) has NO production impl; `ConsolidationMutationOutcome` expects one task result per attempted task; `ConsolidationTaskStatus::Skipped` exists. (source: recon + `docs/domain-data-model.md:2072-2080`)
- Technical: `BeliefRepository::put_belief` (`core/belief/src/lib.rs:41`); `Belief` has free-form `provenance.method`/`provenance.source` + `synthesizer` + `reasoning`; `DerivationKind::Consolidation` exists. (source: recon)
- Technical: the scoped-active-memory read path is `MemoryEventRepository::list_events_for_scope` (all events) → `MemoryRepository::get_memory` → filter `MemoryStatus::Active` (there is no direct list-active-memories port method). (source: recon + `core/memory/src/lib.rs:54-61`)
- Technical: `Belief`/`Consolidation*` are NOT in the v1 accepted slice, so `contracts:check-generated` is green by default for this crate — the load-bearing neutrality check is the `Cargo.toml` deps + grep (AC4/AC6), not the contract gate. (source: recon)
- Product: slice 1 ships the reflection components (synthesizer + executor) as tested units with zero contract change; production wiring (which needs a composite-executor because `Hybrid` bundles 8 task kinds) is a follow-up. (source: recon + review + user "lowest risk" direction)
