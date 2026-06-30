# Plan: In-Memory Consolidation Compaction

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused `consolidation` operation module to `engram-store-memory` with an
`InMemoryConsolidationExecutor` that implements
`ConsolidationMutationExecutor`. The executor scans scoped active memories,
groups exact normalized text duplicates, archives later records in each group,
and emits `Consolidated` lifecycle events. It reports unsupported planned tasks
as skipped rather than pretending they ran.

Tempted to add fuzzy matching or summary synthesis; declining because this
slice proves the mutation audit path first. Tempted to add a generic
consolidation framework inside the store crate; declining because one focused
executor is enough until more algorithms exist.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` free of concrete task algorithms.
- Keep `lib.rs` and `service.rs` as facades/composition surfaces.

## Construction tests

**Unit/integration tests:**

- Duplicate scoped memories archive later records and emit consolidated events.
- Cross-scope duplicates remain active.
- Non-duplicate memories complete compaction with zero updates.
- Unsupported planned tasks are skipped and counted.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryConsolidationExecutor` implements the existing
`ConsolidationMutationExecutor` port. No new public JSON schema or TypeScript
contract is introduced.

### Component / module decomposition

- `crates/engram-store-memory/src/consolidation.rs` owns compaction execution,
  duplicate grouping, event construction, and audit counters.
- `crates/engram-store-memory/src/lib.rs` re-exports only the executor type.
- Tests live in `crates/engram-store-memory/tests/consolidation_compaction.rs`
  and use `GatedConsolidationService` to exercise the real mutating envelope.

### Failure, edge cases & resilience

Archived, redacted, forgotten, expired, and already archived records are not
compaction candidates. Empty or whitespace-only content is skipped. Duplicate
groups are deterministic: earliest `created_at`, then memory ID, is preserved.

## Tasks

### T1: Add focused in-memory compaction executor

**Depends on:** `GatedConsolidationService` and in-memory memory storage.

**Tests:**

- Compaction archives only later scoped duplicates.
- Events and task stats match archived duplicates.

**Approach:**

- Implement a narrow executor in a new operation module.
- Reuse `scope_allows` and existing service state lock.
- Keep unsupported tasks as skipped task results.

**Done when:** adapter consolidation tests pass.

### T2: Document and wire the phase status

**Depends on:** T1.

**Tests:**

- Full repository gates pass.
- `docs/implementation/phases.json` marks the phase done after gates pass.

**Approach:**

- Update roadmap status documents and changelog.
- Run the repository validation suite before commit.

**Done when:** docs, phase JSON, and code are committed together.

## Rollout

Library code only. Production summarization, decay, hierarchy, belief synthesis,
and schedulers remain future phases.

## Risks

- Exact-text duplicate compaction is intentionally conservative and will miss
  semantic duplicates; this is acceptable until embedding/model-backed
  algorithms are specified.
- Archiving duplicates changes default retrieval behavior, so scope and audit
  tests must prove the mutation is bounded and explainable.

## Changelog

- 2026-06-30: initial plan for a concrete in-memory compaction executor.
- 2026-06-30: shipped exact-text in-memory compaction with archive events,
  skipped unsupported task reporting, and scoped adapter tests.
