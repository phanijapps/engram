# Plan: In-Memory Consolidation Decay

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Extend the focused in-memory consolidation operation module with a decay task
that marks scoped active records expired when `policy.expires_at <= started_at`.
Legal-hold records are skipped. Unsupported planned tasks remain explicit
skipped task results.

Tempted to introduce a generic task registry; declining because the executor
has two deterministic tasks and the match remains clearer. Tempted to combine
decay with pruning; declining because expiration and deletion are different
lifecycle semantics.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` free of concrete task algorithms.
- Keep decay separate from compaction, forgetting, redaction, and pruning.

## Construction tests

**Unit/integration tests:**

- Due scoped active memories become expired and emit expired events.
- Future-expiring and legal-hold memories remain active.
- Out-of-scope due memories remain active.
- Unsupported planned tasks are skipped and counted.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

`InMemoryConsolidationExecutor` continues to implement the existing
`ConsolidationMutationExecutor` port. No new public JSON schema or TypeScript
contract is introduced.

### Component / module decomposition

- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) owns decay execution,
  policy-expiry checks, event construction, and audit counters.
- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) exercises the real
  gated mutating consolidation service.

### Failure, edge cases & resilience

Decay uses the consolidation run start timestamp rather than wall-clock reads
inside the scan for due decisions. The event recorded time uses the adapter
clock. Legal hold wins over expiry.

## Tasks

### T1: Add focused in-memory decay behavior

**Depends on:** PHASE23 `InMemoryConsolidationExecutor`.

**Tests:**

- Decay expires only due scoped active memories.
- Events and task stats match expired records.

**Approach:**

- Extend the executor's planned-task match with `Decay`.
- Reuse `scope_allows` and existing service state lock.
- Keep unsupported tasks as skipped task results.

**Done when:** adapter decay tests pass.

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

Library code only. Production pruning, hierarchy rebuild, belief synthesis, and
schedulers remain future phases.

## Risks

- Decay changes default retrieval by making expired records inactive; tests must
  prove the event audit and legal-hold skip behavior.
- Expiry uses policy timestamps only; broader confidence/time-decay semantics
  need a separate spec.

## Changelog

- 2026-06-30: initial plan for a concrete in-memory decay executor.
- 2026-06-30: shipped policy-expiry decay with legal-hold skip behavior,
  expired lifecycle events, and scoped adapter tests.
