# Spec: SQL Service Conformance

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0005, ADR-0006
- **Brief:** none
- **Contract:** contracts/v1/memory.schema.json
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

`engram-store-sql` provides a SQLite-backed `MemoryService` baseline that
preserves the accepted memory contract and the in-memory adapter's observable
semantics. Contributors can run write, retrieve, forget, event, idempotency,
scope, and evaluation fixtures against SQL without provisioning external
infrastructure or introducing production database commitments.

## Boundaries

### Always do

- Preserve accepted v1 memory payloads at the `engram-core` port boundary.
- Keep SQL behavior behind the `engram-store-sql` adapter crate.
- Keep crate roots as facades and place operation behavior in focused modules.
- Reuse the deterministic evaluation fixture runner where possible.

### Ask first

- Add a server database target such as PostgreSQL.
- Change the accepted domain model, JSON schema, or generated TypeScript
  contracts.
- Introduce connection pooling, async runtime coupling, migrations tooling, or a
  production deployment surface.

### Never do

- Put SQL, rusqlite, N-API, TypeScript, vector, or provider dependencies in
  `engram-domain` or `engram-core`.
- Reimplement domain truth in the SQL adapter.
- Create a god module that mixes construction, validation, orchestration,
  persistence, scoring, policy, and error translation.
- Return memories, events, or idempotent responses outside the requested scope.

## Testing Strategy

- Write behavior: TDD through SQL service tests. Valid writes create one memory
  and one event; idempotent retries, including concurrent retries, return the
  original response without duplicate events.
- Retrieval behavior: goal-based integration through the shared evaluation
  runner and exact/keyword fixture cases, because useful recall is only proven
  across write plus retrieve.
- Forget behavior: TDD through SQL service lifecycle tests. Delete mode removes
  active records and appends a forget event.
- Adapter hygiene: goal-based checks through `cargo fmt`, `cargo check`,
  `cargo clippy`, `cargo test`, contract hooks, and TypeScript workspace gates.

## Acceptance Criteria

- [x] `engram-store-sql` exposes a SQLite-backed service constructor for tests
  and local fixtures.
- [x] SQL writes validate input, enforce policy, persist the memory, append a
  written event, and preserve idempotency without duplicate lifecycle events,
  including concurrent writes with the same scoped idempotency key.
- [x] SQL retrieval applies scope and policy before ranking and returns exact or
  keyword baseline matches with explanations.
- [x] SQL forget handles delete behavior without returning deleted memories in
  retrieval.
- [x] SQL behavior passes at least one `engram-eval` recall fixture through the
  same public service traits used by the in-memory adapter.
- [x] `engram-store-sql` keeps `lib.rs` as a narrow facade and splits behavior by
  dependency construction, schema, validation, write, retrieval, forget, scope,
  service, and engine responsibilities.

## Assumptions

- Technical: the Rust workspace uses edition 2024 and includes
  `engram-store-sql` as a workspace crate (source: `Cargo.toml`).
- Technical: SQLite is the accepted first SQL target and server databases are
  deferred (source: `docs/adr/0006-first-sql-adapter-sqlite.md`).
- Technical: SQL adapters preserve in-memory adapter semantics for writes,
  idempotency, events, scope, retrieval baseline, and contract payloads (source:
  `docs/adr/0005-storage-adapter-semantics.md`).
- Process: roadmap slices run through spec-driven implementation and full
  repository checks (source: `docs/implementation-roadmap.md`).
- Process: crate roots stay facades and behavior lives in focused modules
  (source: `AGENTS.md`).
- Product: Phase 6 closes with SQLite conformance while production database
  targets remain future work (source: user confirmation 2026-06-29).
