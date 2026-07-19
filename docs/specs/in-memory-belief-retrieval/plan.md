# Plan: In-Memory Belief Retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused belief retrieval candidate module in `engram-store-memory`.
`retrieval.rs` will keep request orchestration, state snapshotting, fusion, and
final truncation. The new module will own belief-specific scope, lifecycle,
policy, filter, keyword scoring, explanation, omission, and result conversion
logic.

Tempted to add explicit include-beliefs request fields; declining because
`RetrievalTargetType::Belief` is already portable and this slice can prove
behavior without contract changes. Contradiction-aware ranking is now handled by
the focused in-memory contradiction-aware belief ranking spec.

## Constraints

- No public v1 contract or generated TypeScript changes.
- No model, embedding, vector, SQL, scheduler, or runtime dependency.
- Keep `engram-core` unchanged.
- Keep belief retrieval separate from belief synthesis, contradiction detection,
  and knowledge retrieval.

## Construction tests

**Unit/integration tests:**

- Matching active in-scope belief returns a belief result with explanation and
  `belief.keyword` trace.
- Stale, inactive, expired, low-confidence, and out-of-scope beliefs are not
  returned.
- Policy-denied beliefs are reported as omissions.
- Beliefs compose with memory candidates through the existing fusion and final
  budget truncation path.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

No trait, schema, or TypeScript API changes. `InMemoryMemoryService` continues
to implement `MemoryService::retrieve`.

### Component / module decomposition

- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) owns belief candidate
  construction.
- the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`) snapshots beliefs and appends
  their candidates before shared fusion.
- Tests live in
  the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`).

### Failure, edge cases & resilience

Policy denials become omissions. Non-policy authorizer failures return errors.
Expired and stale beliefs do not leak as results. Open contradictions can reduce
belief score but do not hide the belief. Final result truncation uses the
existing fused-result omission path.

## Tasks

### T1: Build belief candidate retrieval

**Depends on:** existing belief repository support and weighted retrieval
fusion.

**Tests:**

- Active matching belief retrieval.
- Scope, lifecycle, expiry, confidence, and policy omission behavior.

**Approach:**

- Create focused candidate builder.
- Snapshot beliefs from in-memory state.
- Route candidates through existing retrieval fusion.

**Done when:** focused adapter tests pass.

### T2: Document and wire phase status

**Depends on:** T1.

**Tests:**

- Full repository gates pass.
- `docs/implementation/phases.json` marks the phase done after gates pass.

**Approach:**

- Update roadmap status documents and changelog.
- Run the repository validation suite before commit.

**Done when:** docs, phase JSON, and code are committed together.

## Rollout

Library code only. Belief graph expansion, semantic belief retrieval, and manual
belief query APIs remain future phases.

## Risks

- Keyword belief retrieval may over-return derived claims; this is bounded by
  scope, status, policy, confidence, and explicit result type.
- There is no dedicated belief retrieval flag, so callers must inspect
  `targetType` when they want to distinguish source truth from derived stance.

## Changelog

- 2026-06-30: initial plan for in-memory belief retrieval.
- 2026-06-30: shipped focused in-memory belief retrieval candidates with scope,
  lifecycle, policy, confidence, time-filter, explanation, and fusion coverage.
- 2026-06-30: updated for shipped contradiction-aware belief ranking over
  explicit open contradiction records.
