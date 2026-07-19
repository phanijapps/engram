# Spec: Backend-parametric conformance proof (S7)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine grid vs backend recipe — the contract this slice proves), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — the neutrality gate)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S7, capabilities #17 + #2 contract)
- **Contract:** none — a test artifact (stub backend) + documentation.
- **Shape:** integration

> **Spec contract:** this document defines what "done" means.

## Objective

A **non-SQLite stub backend** (HashMap-backed `MemoryService`) implements the port traits and passes a basic memory lifecycle check (write → retrieve → forget), proving the port abstraction is **backend-parametric**: a backend that names zero `Sql*` types can satisfy the contracts and exercise the same operations the SQLite fixtures do. Combined with the ADR-0022 rule-1 neutrality gate (S1, `check-engine-neutrality.sh`), this is the full proof that swapping storage is a config/crate change, not an application rewrite.

## Boundaries

### Always do
- Implement the port traits (`MemoryService` at minimum) using only stdlib types (`HashMap`, etc.) — name zero `Sql*` types, zero SQL.
- Exercise the same lifecycle operations the SQLite memory fixture does (write → retrieve → forget, scope isolation).
- Keep the stub in the test layer (`#[cfg(test)]` or `tests/`), not in production code.

### Ask first
- Extend the stub to cover `KnowledgeRepository` / `BeliefRepository` (more traits = stronger proof).
- Add a second non-SQLite backend (e.g., a Postgres mock) for broader proof.

### Never do
- Name an engine type (`Sql*`, `rusqlite`, …) in the stub. *(structural — defeats the proof)*
- Ship the stub as a production adapter (it's a proof artifact, not a real backend).

## Testing Strategy
- **Parametric conformance — TDD.** The stub `MemoryService` round-trips write → retrieve → forget with scope isolation, through the port trait interface. The test asserts: (a) the operations succeed against the non-SQLite backend, (b) the stub's source file contains zero `Sql*` references (grep assertion).
- **Neutrality gate — goal-based.** `check-engine-neutrality.sh` remains green (already enforces the port layer; S7 does not weaken it).

## Acceptance Criteria
- [x] A `StubMemoryService` (HashMap-backed, implements `MemoryService`) exists in the test layer and round-trips write → retrieve → forget with scope isolation, through the port trait — proving the abstraction is satisfiable without SQLite.
- [x] The stub's source file names zero `Sql*` / `rusqlite` / `sqlite` types (asserted by a test-level grep or a compile-time guarantee: the stub's crate depends on `engram-memory` but NOT on `engram-store-sql`).
- [x] The conformance harness + the neutrality gate + the stub backend together document the ADR-0022 contract proof (in a test doc comment or `docs/` note): the port layer is engine-symbol-free (gate-enforced) + a non-SQLite backend satisfies the traits (stub-proven).
- [x] SQLite behavior is unchanged; existing workspace tests green.

## Assumptions
- Technical: the neutrality gate (`check-engine-neutrality.sh`, S1) already enforces "zero `Sql*` symbols in the port crates" — S7 does NOT weaken or duplicate it.
- Technical: the conformance fixtures (S2–S6) construct `SqlMemoryService` directly — they are SQLite-coupled in construction but exercise the port traits. S7 adds a non-SQLite proof that the traits ARE satisfiable without SQLite.
- Design: the stub is a minimal test artifact (HashMap MemoryService); it proves the abstraction, not a production backend. Port-parametric proof, not a second real backend. (source: user confirmation 2026-07-10)
- Process: SQLite only in production; additive only; the stub lives in tests.
