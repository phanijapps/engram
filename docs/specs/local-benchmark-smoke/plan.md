# Plan: Local Benchmark Smoke

## Scope

Add a basic local benchmark smoke path for in-memory write and retrieval without
adding dependencies or performance thresholds.

## Steps

- [x] Add `crates/engram-store-memory/examples/benchmark_local.rs`.
- [x] Add `docs/benchmarks.md` with run commands and claim boundaries.
- [x] Update roadmap/changelog/phase status.
- [x] Run example, compile, docs, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo check -p engram-store-memory --examples
cargo run -p engram-store-memory --example benchmark_local
.codex/hooks/check-docs.sh
git diff --check
```
