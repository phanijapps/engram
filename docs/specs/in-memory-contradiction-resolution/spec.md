# Spec: In-Memory Contradiction Resolution

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The in-memory belief repository supports explicit contradiction review by
looking up scoped contradiction records and applying a reviewer-supplied
`ContradictionResolution` without mutating the target memories, chunks, or
beliefs in conflict.

## Boundaries

### Always do

- Keep contradiction records separate from source memories, knowledge chunks,
  and beliefs.
- Require scoped lookup before resolving a contradiction.
- Preserve contradiction targets, detection provenance, and detection time.
- Derive contradiction status from the resolution kind in a deterministic way.

### Ask first

- Add a public JSON operation schema for contradiction review.
- Mutate target memories, chunks, or beliefs as a side effect of resolution.
- Add semantic/model-assisted contradiction detection.

### Never do

- Resolve a contradiction across tenant or workspace boundaries.
- Treat resolution as proof that source truth changed.
- Add model, embedding, vector, SQL, scheduler, runtime, or TypeScript
  dependencies for this in-memory slice.
- Hide unresolved review outcomes as successful resolution.

## Testing Strategy

- TDD: repository tests cover scoped lookup, deterministic status updates, scope
  isolation, and target immutability.
- Regression: existing belief synthesis, contradiction detection, and belief
  retrieval tests continue to pass.
- Goal-based: full repository gates and contract drift checks continue to pass
  without public schema changes.

## Acceptance Criteria

- [x] `BeliefRepository` exposes scoped contradiction lookup.
- [x] `BeliefRepository` exposes scoped contradiction resolution.
- [x] Resolving a contradiction preserves targets, detection provenance, and
  detected-at timestamp.
- [x] Resolution status mapping is deterministic.
- [x] Cross-scope resolution returns not found and does not mutate the record.
- [x] Contradiction resolution does not mutate target beliefs, memories, or
  chunks.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `ContradictionResolution` and `ContradictionStatus` are already in
  the domain model (source: `crates/engram-domain/src/belief.rs`).
- Technical: `BeliefRepository` is the existing core port for belief and
  contradiction persistence (source: `crates/engram-core/src/lib.rs`).
- Technical: in-memory state already stores contradictions separately from
  beliefs and memories (source: `crates/engram-store-memory/src/state.rs`).
- Process: semantic contradiction detection remains future work until a quality
  spec exists (source: `docs/implementation-roadmap.md`).
