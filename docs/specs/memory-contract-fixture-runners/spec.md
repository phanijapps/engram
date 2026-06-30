# Spec: Memory Contract Fixture Runners

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0002, ADR-0003, ADR-0005
- **Brief:** none
- **Contract:** `contracts/v1/examples/*.json`
- **Shape:** evaluation utility

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has reusable Rust fixture-runner utilities for accepted write and
retrieval contract examples so in-memory, SQL, native binding, and future
adapters can exercise the same portable behavior without copying one-off test
logic.

## Boundaries

### Always do

- Keep fixture execution adapter-neutral and expressed through `MemoryService`.
- Keep accepted example loading in `engram-eval`, not in concrete store crates.
- Preserve existing in-memory and SQL behavior assertions.
- Keep invalid fixture deserialization visible to tests without calling stores.

### Ask first

- Change JSON schemas or accepted fixture payloads.
- Add TypeScript fixture runners in this slice.
- Add async runtime dependencies to store crates for tests.
- Replace adapter-specific persistence or event assertions.

### Never do

- Move storage behavior into `engram-eval`.
- Make `engram-eval` depend on concrete stores, SQL, vector, Node, or TypeScript.
- Hide policy, provenance, or lifecycle assertions behind generic success.
- Create a shared god test helper that owns construction, persistence,
  evaluation, and adapter internals.

## Testing Strategy

- TDD: migrate representative in-memory and SQL accepted write/retrieval tests
  to use the shared runner.
- Regression: existing `MemoryFixtureRunner` evaluation tests continue to pass.
- Goal-based: `cargo test -p engram-eval`, `cargo test -p engram-store-memory
  --test write_memory_fixtures --test retrieve_context_fixtures`, and
  `cargo test -p engram-store-sql --test service` prove reusable execution
  across adapters.

## Acceptance Criteria

- [x] `engram-eval` exposes accepted write and retrieval fixture loaders.
- [x] `engram-eval` exposes a focused contract runner over `MemoryService`.
- [x] In-memory accepted write/retrieval fixture tests use the shared runner.
- [x] SQL accepted write/retrieval service tests use the shared runner.
- [x] Invalid fixture tests still fail before service execution.
- [x] `engram-eval` does not depend on concrete store crates outside dev tests.
- [x] No public v1 contract, schema, or generated TypeScript change.

## Assumptions

- Technical: `MemoryService` is the stable adapter-neutral contract for write
  and retrieval behavior.
- Technical: accepted examples under `contracts/v1/examples/` are portable
  enough to seed both in-memory and SQL adapters.
- Process: future fixture files for positive/forbidden/budget/no-result cases
  will build on these utilities rather than duplicating adapter tests.
