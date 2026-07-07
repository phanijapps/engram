# Spec: Local Benchmark Smoke

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0002, ADR-0003
- **Brief:** none
- **Contract:** none
- **Shape:** repository hygiene

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has a runnable local benchmark smoke path that measures in-memory write
and retrieval wall-clock timing through the public `MemoryService` API. The
benchmark records local observations only; it does not define performance
targets or support performance claims.

## Boundaries

### Always do

- Exercise the public memory service API.
- Keep benchmark output explicit about local, non-comparative timing.
- Avoid hard-coded pass/fail latency thresholds.
- Keep benchmark documentation separate from release claims.

### Ask first

- Add Criterion, IAI, or another benchmark dependency.
- Add CI performance thresholds.
- Publish comparative performance claims.
- Benchmark hosted embedding providers or model downloads.

### Never do

- Optimize implementation code before correctness fixtures require it.
- Treat one local run as production evidence.
- Add benchmark-only fields to domain contracts.
- Hide policy or provenance fields in benchmark requests.

## Testing Strategy

- Goal-based: `cargo run -p engram-store-memory --example benchmark_local`
  exercises the benchmark path end to end and prints observed write/retrieval
  timings.
- Regression: `cargo check -p engram-store-memory --examples` proves the
  benchmark continues to compile with the public API.

## Acceptance Criteria

- [x] A runnable in-memory benchmark example exists.
- [x] The benchmark writes multiple memories and performs retrieval through
  `MemoryService`.
- [x] Benchmark documentation explains how to run it and what claims are not
  supported.
- [x] No benchmark dependency or runtime optimization is introduced.
- [x] No public contract, schema, or generated TypeScript changes.

## Assumptions

- Technical: `InMemoryMemoryService` is the public local process adapter entry
  point.
- Technical: `cargo run -p engram-store-memory --example ...` is already used
  for local examples.
- Process: `AGENTS.md` says not to optimize before correctness fixtures and
  basic benchmarks exist.
