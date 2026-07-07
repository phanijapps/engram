# RFC-0006: Split behavior ports from engram-core

- **Status:** Accepted
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-01
- **Date closed:** 2026-07-01
- **Decision weight:** standard
- **Related:** [ADR-0001](../adr/0001-workspace-boundaries.md), [docs/arch_divergence.md](../arch_divergence.md) (Area 1 = the "Memory/knowledge separation" row; Area 2 = the "Rust crate modularity" row), implementation-roadmap.md Phases 10-12

## Reviewer brief

- **Decision:** Move the behavior ports canonically owned by `core/orchestration` (`engram-core`) into focused crates, retaining `engram-core` as a re-export facade.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - New crates `engram-belief`, `engram-hierarchy`, `engram-consolidation`; eval ports move into the existing `engram-eval`.
  - `engram-core` re-exports them (as it already does for memory/knowledge/retrieval).
- **Affected surface:** `core/orchestration` (+ its `consolidation/` subdir), three new `core/*` crates, `core/eval`; the workspace `Cargo.toml` member list. No public API change; no TypeScript contract change.
- **Stakes:** reversible structural move, no behavior change — **except the eval slice, which inverts a dependency edge** (see Proposal). Gated by `cargo check/clippy/test` + `pnpm typecheck/build`.
- **Review focus:** (1) the 4-way boundary (esp. consolidation earning its own crate), (2) the facade-fate deferral in D2 vs `arch_divergence.md` Area 1, (3) the eval dependency inversion (the riskiest slice).
- **Not in scope:** renaming adapter packages (`engram-store-sql` → memory-sqlite etc.), the durable hierarchy backend, governed taxonomy pipeline, retrieval-mode dispatch — all tracked separately in `arch_divergence.md`.

## The ask

**Recommendation (BLUF):** Approve splitting the behavior ports and structs currently defined in `core/orchestration` into focused crates (`engram-belief`, `engram-hierarchy`, `engram-consolidation`, and the eval ports into the existing `engram-eval`), retaining `engram-core` as a re-export facade. This completes the split pattern `engram-memory` and `engram-knowledge` already established. The belief, hierarchy, and consolidation slices change **zero import paths**; the eval slice is a dependency inversion (no public-API break, but a real structural change).

**Why now (SCQA):** `engram-memory` and `engram-knowledge` ports already live in their own crates and are re-exported by the `engram-core` facade — but `engram-core` still *canonically owns* the belief, hierarchy, consolidation, and evaluation ports (`core/orchestration/src/lib.rs:28-143` and `core/orchestration/src/consolidation/`). That makes `core/orchestration` a god-module in violation of `AGENTS.md`'s "no god modules" rule, leaves the `engram-belief`/`engram-hierarchy`/`engram-consolidation` crates that the roadmap (Phases 10-12) names as aspirational-only, and keeps `arch_divergence.md` Area 2 stuck at 80%. The facade already exists, so the move is low-risk. The question: do we finish the pattern now, and what does `engram-core` become afterward?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Where do the ports/structs/behavior live? | 4-way split — `engram-belief` (BeliefRepository, BeliefSynthesizer, ContradictionDetector); `engram-hierarchy` (HierarchyRepository, HierarchyBuilder); `engram-consolidation` (ConsolidationService port + ConsolidationMutationExecutor/Outcome + DryRun/Gated service impls + planner/evaluation_gate/validation helpers); eval ports (EvaluationRunner, EvaluationReport, EvaluationCaseReport) into the existing `engram-eval`. | Each concern owns one reason to change; completes the memory/knowledge precedent; `engram-eval` already exists. | this review | Confirm the four boundaries; rule on whether consolidation earns its own crate (see Options). |
| D2 | What happens to `engram-core`? | Retain it as the re-export facade **for this RFC**; do not deprecate here. The facade's ultimate fate (permanent vs eventual thinning/deprecation) is a separate decision tracked against `arch_divergence.md` Area 1 — this RFC neither mandates nor precludes deprecation. | `AGENTS.md` sanctions the facade as "a compatibility re-export layer"; retaining it means zero import-path churn for every `engram_core::` consumer now, while leaving Area 1's closure free to decide later. | this review | Confirm the facade is retained and its fate deferred (reconciles with Area 1). |
| D3 | How is the move sequenced? | Per-concern slices, one PR each, in order **belief → hierarchy → consolidation → eval**. | The first three are mechanically identical (new crate + facade re-export, zero import-path changes). **Eval is the riskiest** — it inverts a dependency edge — so it goes last, once the pattern is proven. | this review | Confirm slice order; note eval is the riskiest, not a trivial de-risk. |

## Problem & goals

**Problem.** `core/orchestration/src/lib.rs` and its `consolidation/` subdirectory define 8 `pub trait`s, 5 `pub struct`s, and ~280 lines of consolidation *behavior* (planner, evaluation gate, validation) spanning four unrelated concerns (belief, hierarchy, consolidation, evaluation) — all alongside `engram-core`'s role as the facade. This is the god-module `AGENTS.md` forbids, and it blocks the divergence tracker's crate-modularity area from closing.

**Goals.**
- Each behavior concern canonically owned by a focused crate with one reason to change.
- `engram-core` reduced to a pure facade (module declarations + narrow re-exports + top-level docs), matching the boundary rule for crate roots.
- No public API change: every existing `engram_core::…` import path keeps resolving.
- The `engram-belief` / `engram-hierarchy` / `engram-consolidation` crate names the roadmap already references become real.

**Non-goals** (could-have-been-goals, deliberately dropped).
- Renaming adapter packages to post-move names (`engram-store-sql` → memory-sqlite, `engram-store-vector` → retrieval-sqlite-vec). Tracked separately in `arch_divergence.md` closure plan item 7; bundles unrelated churn.
- Migrating the 35 downstream import sites from `engram_core::` to the focused-crate paths. The facade makes this optional and incremental (and is itself deferred by D2); forcing it would add review burden for no behavioral gain.
- Deciding the facade's ultimate fate — deferred to Area 1 (see D2).
- Any durable backend, retrieval-mode, or taxonomy-pipeline work — separate divergence areas.

## Proposal

The belief, hierarchy, and consolidation slices share one mechanical shape, proven by how `engram-memory`/`engram-knowledge` are already wired:

1. **Create the focused crate** (`core/<concern>/`): `Cargo.toml` depending on `engram-domain` (+ `engram-runtime` for `CoreError`/`CoreResult`); `src/lib.rs` with a module-level doc and the moved definitions.
2. **Delete the definitions from `core/orchestration/`** and replace them with a re-export (`pub use engram_<concern>::*;`), mirroring `pub use engram_memory::*;` at `lib.rs:18`.
3. **Add the crate to the workspace** `Cargo.toml` member list and to `engram-core`'s dependencies.

**Slice specifics:**

- **`engram-consolidation`** receives the `ConsolidationService` port trait, `ConsolidationMutationExecutor` + `ConsolidationMutationOutcome`, the `DryRunConsolidationService` / `GatedConsolidationService` default impls, **and** the `consolidation/{planner,evaluation_gate,validation}.rs` helper modules — ports and impls move together. `evaluation_gate.rs:12` currently does `use crate::EvaluationReport;`; after the move it imports `EvaluationReport` from `engram-core`'s re-export (still valid whether or not the eval slice has landed). Adapters that implement the executor port (e.g. `engram-store-memory`) add `engram-consolidation` as a dependency, exactly as they depend on `engram-belief` for `BeliefRepository`.

- **`engram-eval` (dependency inversion — the riskiest slice).** Today `engram-eval` depends on `engram-core` (`core/eval/Cargo.toml`) and imports `EvaluationRunner`/`EvaluationReport`/`EvaluationCaseReport` plus `CoreError`/`CoreResult`/`MemoryService` from it. The move:
  1. Relocate `lib.rs:125-143` (the three eval items) into `engram-eval/src/lib.rs` — `engram-eval` now *owns* them.
  2. **Repoint `engram-eval`'s other `engram-core` imports** to their canonical crates: `CoreError`/`CoreResult` ← `engram-runtime`; `MemoryService` ← `engram-memory`. (`contract_runner.rs` is the consumer.)
  3. `engram-eval` **drops its `engram-core` dependency**; `engram-core` **adds** `engram-eval` to `[dependencies]` and re-exports the three eval types.
  4. The edge flips: `engram-eval → engram-core` becomes `engram-core → engram-eval`. This is a cycle **iff** `engram-eval` retains any `engram-core` import — step 2 removes them, and `cargo check --workspace` fails loudly if any remain.

Dependency direction after the move:

```
engram-domain ── ◀── engram-belief
                ◀── engram-hierarchy
                ◀── engram-eval ── ◀── engram-runtime, engram-memory
                ◀── engram-consolidation ── ◀── (belief, hierarchy, memory, domain; + EvaluationReport via engram-core re-export)
engram-core ── re-exports ── all of the above (facade; owns nothing canonical)
```

`engram-consolidation` is a leaf *consumer* of ports (it composes belief synthesis, hierarchy build, compaction, decay). It must not be imported by `engram-domain`, `engram-belief`, or `engram-hierarchy`, or the workspace fails to compile with a cycle.

**Migration path / compatibility.** Because every consumer imports via `engram_core::`, the re-exports preserve all `engram_core::` import sites (N-API binding at `bindings/node/src/lib.rs:7`, `engram-store-belief-sqlite` at `adapters/orchestration/belief-sqlite`, the `engram-store-memory` fixture, and orchestration's own tests) across the belief/hierarchy/consolidation slices. Only the eval slice changes a dependency edge (no import-path break).

## Options considered

Axis: *degree of relocation × fate of the `engram-core` facade* — collectively exhaustive over "do we move the ports, and what does the facade become."

- **O1 — Do-nothing.** Leave the ports in `core/orchestration`. Cost of delay: the god-module persists, `AGENTS.md` stays violated, Area 2 stays at 80%, and the next production belief/sleep-cycle work lands in the wrong place.
- **O2 — Move + retain facade (recommended). ★** Move canonical definitions to focused crates; `engram-core` re-exports. Zero import-path changes for belief/hierarchy/consolidation. The facade's ultimate fate is deferred (D2): a later, separate decision may move this toward O3.
- **O3 — Move + deprecate the facade now.** Relocate definitions *and* migrate every `engram_core::` import to the focused-crate path, then deprecate the shim in this RFC. More churn across adapters/bindings/tests; removes the ergonomic one-stop import; no behavioral gain over O2 — and pre-empts the Area-1 decision D2 deliberately leaves open.
- **O4 — Move into one new `engram-behavior` crate.** Single crate for all four concerns. Re-creates a god-module under a new name and violates the one-reason-to-change rule; rejected.

**Consolidation sub-decision (under D1).** Consolidation is orchestration-*flavored* (it composes other ports), so the alternative is to leave it in `engram-core` and move only belief/hierarchy/eval. Recommendation: still give it its own crate — it has a single clear responsibility (auditable `ConsolidationRun` cycles over task types), keeping it in `engram-core` perpetuates the god-module, and the roadmap (Phase 12) names `engram-consolidation` as the target.

## Risks & what would make this wrong

**Pre-mortem** (assume this shipped and failed):
- *Eval dependency cycle.* After the inversion, if `engram-eval` retains any `engram-core` import, `engram-core → engram-eval → engram-core` is a cycle. **Mitigation:** step 2 repoints `engram-eval`'s `CoreError`/`CoreResult`/`MemoryService` imports to `engram-runtime`/`engram-memory`; `cargo check --workspace` fails on any cycle. This is why eval is sequenced last (D3).
- *`engram-consolidation` cycle.* It depends on belief/hierarchy/memory; if any of those import consolidation back, the workspace won't compile. **Mitigation:** consolidation is a leaf consumer; the build is the guard.
- *Re-export collision.* `engram-core` re-exporting `*` from many crates could clash on a duplicate name. **Mitigation:** public names are distinct today (no overlaps across concerns); `cargo check` catches any collision.
- *Orphaned definition.* An item missed in the inventory stays stranded in `core/orchestration`. **Mitigation:** the inventory below is the checklist; each slice re-greps `pub trait`/`pub struct` in `core/orchestration/`.
- *Silent N-API break.* The binding uses `engram_core::{BeliefRepository, ContradictionDetector}`. **Mitigation:** the facade re-export preserves the path; gate includes `pnpm build` + binding typecheck.

**Inventory (the orphan/cycle checklist):**
- `core/orchestration/src/lib.rs` — 7 traits: `BeliefRepository` :28, `HierarchyRepository` :57, `ConsolidationService` :79, `BeliefSynthesizer` :89, `ContradictionDetector` :99, `HierarchyBuilder` :110, `EvaluationRunner` :125; 2 structs: `EvaluationReport` :132, `EvaluationCaseReport` :139.
- `core/orchestration/src/consolidation/` — 1 trait: `ConsolidationMutationExecutor` (`mutating.rs:62`); 3 structs: `ConsolidationMutationOutcome` (`mutating.rs:32`), `DryRunConsolidationService` (`service.rs:31`), `GatedConsolidationService` (`mutating.rs:78`); behavior modules `planner.rs`, `evaluation_gate.rs`, `validation.rs`.

**Key assumptions (falsifiable):**
- *Every consumer imports these ports only via `engram_core::`.* Verified by grep this session: 35 total `engram_core::` import lines across the workspace (`grep -rn 'engram_core::' --include=*.rs | grep -v core/orchestration`), of which 14 name one of the moved behavior ports (`grep -rnE 'engram_core::\{?[^}]*\b(Belief|Hierarchy|Consolidation|Evaluation)' --include=*.rs`). If wrong, those sites break at compile — caught by `cargo check`.
- *`engram-eval`'s only `engram-core` imports are the eval types + `CoreError`/`CoreResult`/`MemoryService`.* Verified by grep (`core/eval/src/{lib,contract_runner,report_summary}.rs`). If it imports more, step 2's repoint list is incomplete — caught by `cargo check` after the inversion.

**Drawbacks:** three new crates add workspace surface and a little compile-time cost; retaining the facade means two valid import paths (`engram_core::` and `engram_<concern>::`) coexist, which is mildly redundant — accepted as the cost of zero import-path churn now. The eval slice's dependency inversion is a real structural change, not a pure relocate.

## Evidence & prior art

- **In-repo precedent (load-bearing):** `engram-memory` (`core/memory/src/lib.rs:20`) and `engram-knowledge` already define their ports in dedicated crates and are re-exported by `engram-core` (`core/orchestration/src/lib.rs:17-19`). This RFC finishes that exact pattern for the remaining behavior ports.
- **Governing rules:** `AGENTS.md` — "engram-core is an orchestration facade and compatibility re-export layer above split behavior crates"; "Do not create god classes, god modules, or god packages"; "Crate roots … should be facades."
- **Decisions of record:** [ADR-0001](../adr/0001-workspace-boundaries.md) (workspace boundaries, Accepted).
- **Spike / de-risk:** Blast-radius grep this session — all `engram_core::` consumer sites (35 import lines, 14 naming the moved ports) and the N-API binding resolve through the facade, so canonical relocation (belief/hierarchy/consolidation) needs zero downstream edits; eval's inversion was traced through `core/eval/Cargo.toml` + `core/eval/src/`.
- **External:** Strangler-fig incremental migration (Martin Fowler, "Strangler Fig Application") — the named pattern for relocating definitions behind a compatibility facade. Cited by name only; the in-repo precedent is the stronger evidence.

## Open questions

1. **Release surface.** Are the three new crates published in the Rust release matrix, or workspace-internal only? **Default:** workspace-internal for now (no published-surface change); **owner:** author; **decide-by:** before the next release.

(Naming — singular `engram-belief`/`engram-hierarchy`/`engram-consolidation`, per the `engram-memory`/`engram-knowledge` precedent — is decided in D1, not open.)

## Follow-on artifacts

(Accepted 2026-07-01 — proceeding to spec + light work-loop.)

- ADR-0010 (next ordinal): record the behavior-port crate boundaries, the deferred facade fate, and the eval dependency inversion.
- Spec: `docs/specs/behavior-port-split/` — acceptance criteria for the four slices.
- Convention change: none — `AGENTS.md` already sanctions the facade; the new crate names match the existing boundary-rules section. `arch_divergence.md` Area 1's closing condition will need reconciling with D2 when the facade-fate decision is made.

<!-- Mode note: standard-weight RFC. Research (port inventory + consumer blast-radius
spike + eval dependency trace) completed in the same session; in-repo citations are
file:line and verified. One adversarial pass surfaced 3 Blockers (inventory
miscount, omitted consolidation behavior, under-specified eval inversion) and a
D2/Area-1 conflict — all folded into this revision. -->

## Errata

- **2026-07-01 — Implementation refined D3 ordering and the consolidation
  dependency edge (narrowing, not widening, the work).** Recorded in ADR-0010.
  Two details in this RFC's body did not survive implementation:
  1. **D3 ordering was infeasible as written.** `engram-consolidation` depends
     on eval types (`EvaluationReport` via `evaluation_gate`, `EvaluationRunner`
     via the gated service). Importing them from `engram-core` while
     `engram-core` re-exports `engram-consolidation` would cycle. **Eval
     therefore moved *before* consolidation** (actual order: belief → hierarchy
     → eval → consolidation), not "consolidation before eval" as D3 stated.
     Eval is still the riskiest slice (dependency inversion) — it just could
     not be last.
  2. **The consolidation dependency edge is narrower than the body implies.**
     `engram-consolidation` depends on `engram-eval` (not on
     `engram-belief`/`engram-hierarchy`/`engram-memory`); the composing mutation
     impls live in adapters, which depend on belief/hierarchy/memory there.
  (The "35 consumer sites" figure in Evidence is the total `engram_core::`
  import-line count; 14 of those name the moved ports.)
