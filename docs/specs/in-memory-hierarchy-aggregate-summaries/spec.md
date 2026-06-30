# Spec: In-Memory Hierarchy Aggregate Summaries

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** behavior

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Entity aggregate hierarchy nodes in the in-memory adapter expose deterministic
member-derived summaries instead of only generic member counts. The summary is
built from scoped member memory summaries or normalized text excerpts, remains
model-free, and does not change hierarchy contracts.

## Boundaries

### Always do

- Derive aggregate summaries from the eligible member memories already used to
  build the aggregate.
- Prefer explicit memory summaries over raw text excerpts.
- Keep output deterministic and bounded.
- Preserve existing aggregate idempotency and membership behavior.
- Keep model-assisted summaries as a future provider-backed task.

### Ask first

- Add LLM or embedding-backed summarization.
- Add new hierarchy schema fields.
- Change aggregate grouping from first explicit entity.
- Rewrite hierarchy retrieval expansion.

### Never do

- Mutate source memory content.
- Persist generated summary text as source truth.
- Pull model providers into `engram-domain`, `engram-core`, or the in-memory
  adapter.
- Combine clustering, ranking, retrieval expansion, and summary generation in
  one module.

## Testing Strategy

- TDD: extend hierarchy aggregate tests to assert the deterministic summary.
- Regression: existing aggregate idempotency and membership tests remain green.
- Goal-based: hierarchy aggregate construction stays model-free with zero model
  calls.

## Acceptance Criteria

- [x] New aggregate nodes include a summary derived from member memory summaries
  or text excerpts.
- [x] The summary is deterministic for a stable set of eligible members.
- [x] Existing aggregate membership and idempotency behavior is unchanged.
- [x] No public domain contract, schema, or generated TypeScript changes.
- [x] Model call counters remain zero for hierarchy aggregate construction.

## Assumptions

- Technical: base hierarchy nodes are created before aggregate nodes in the
  in-memory hierarchy build task.
- Technical: member memories already carry summaries or text suitable for a
  bounded local aggregate summary.
