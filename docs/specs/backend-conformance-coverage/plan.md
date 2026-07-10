# Plan: Backend-parametric conformance proof (S7)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

S7 proves the ADR-0022 contract by implementing a **non-SQLite stub backend** (HashMap-backed `MemoryService`) that passes the same lifecycle operations the SQLite fixtures exercise. The neutrality gate (S1) already enforces the port layer; the stub proves the traits are satisfiable without SQLite.

## Tasks

### T1: StubMemoryService + parametric conformance test
**Depends on:** none · **Mode:** TDD
Implement `StubMemoryService` (HashMap-backed, implements `MemoryService`) in `adapters/integration/tests/stub_backend.rs` (or `core/integration/tests/`). Round-trip write → retrieve → forget with scope isolation, through the `MemoryService` trait. Assert the stub's file names zero `Sql*` types. The test crate must depend on `engram-memory` but NOT on `engram-store-sql` (compile-time guarantee the stub is engine-neutral).

### T2: Document the contract proof
**Depends:** T1 · **Mode:** goal-based
Add a test-level doc comment (or `docs/` note) documenting the full ADR-0022 proof: neutrality gate (S1) + stub backend (S7) = the port layer is engine-symbol-free (gate-enforced) + a non-SQLite backend satisfies the traits (stub-proven). Gate remains green.

## Changelog
- 2026-07-10: initial plan (S7 capstone — stub backend proves the ADR-0022 parametric contract).
