# Plan: SQL Service Conformance

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Build the SQL slice as a service adapter, not as a second domain engine.
`engram-store-sql` owns SQLite persistence and adapter orchestration while
delegating the public behavior shape to `engram-core` traits and accepted
domain types. The implementation uses small modules for schema, validation,
write, retrieval, forget, scope checks, dependency defaults, and the public
engine facade, with tests exercising the service through trait boundaries.

Tempted to add PostgreSQL; declining because ADR-0006 intentionally proves
SQLite first. Tempted to add pooling and migration tooling; declining because
the current acceptance target is an in-memory SQLite conformance adapter.
Tempted to start native bindings in the same change; declining because Phase 7
owns that boundary.

## Constraints

- ADR-0005 requires storage adapters to preserve write transactions,
  idempotency, event ordering, scope isolation, retrieval baseline behavior, and
  contract payloads.
- ADR-0006 selects SQLite as the first SQL adapter and defers file-backed and
  server database work.
- `AGENTS.md` prohibits god modules and requires crate roots to remain facades.
- `docs/implementation-roadmap.md` requires full repository checks before a
  slice is considered done.

## Construction tests

**Integration tests:** SQL service tests cover write/idempotency, retrieval,
forget, and evaluation fixture conformance.

**Manual verification:** none. This is a Rust service adapter with deterministic
automated checks.

## Design (LLD)

### Data & schema

SQLite stores memories, lifecycle events, and write idempotency responses in
adapter-owned tables. Portable memory and event payloads are serialized as v1
domain JSON at the port boundary so SQL table design does not leak into
contracts.

### Interfaces & contracts

`SqlMemoryService` implements `MemoryService`, `MemoryRepository`, and
`MemoryEventRepository`. Its public surface is the Rust trait contract from
`engram-core`; its payload contract remains `contracts/v1/memory.schema.json`.

### Component / module decomposition

- `dependencies.rs` owns deterministic default collaborators for tests and
  local fixtures.
- `schema.rs` owns SQLite schema creation and SQLite-to-core error helpers.
- `validation.rs` owns request validation for SQL service operations.
- `write.rs` owns SQL-backed write orchestration and idempotency.
- `retrieval.rs` owns exact and keyword retrieval baseline behavior.
- `forget.rs` owns SQL-backed lifecycle mutation behavior.
- `scope.rs` owns scope matching helpers.
- `service.rs` owns repository-level SQLite persistence helpers.
- `engine.rs` composes the service facade and implements public core traits.

### Failure, edge cases & resilience

Idempotent retries use a scope-qualified idempotency key and return the original
response with `deduplicated: true`. SQL write transactions persist the memory,
written event, and idempotency response atomically so concurrent retries cannot
create duplicate records or events. Failed validation, denied policy, or SQL
errors surface as typed `engram-core` errors without partially accepted public
responses.

### Quality attributes

The slice optimizes for correctness, reproducibility, and clean module
boundaries. Performance tuning, indexes beyond the baseline, pooling, and
benchmarks wait until correctness fixtures and the adapter contract stabilize.

## Tasks

### T1: SQLite repository stores contract payloads and events

**Depends on:** none

**Tests:**
- Repository tests insert and read memory payloads without contract-shape drift.
- Repository tests list lifecycle events by memory scope.

**Approach:**
- Keep SQLite schema creation in `schema.rs`.
- Keep low-level persistence helpers in `service.rs`.
- Store portable domain payloads as JSON at the adapter boundary.

**Done when:** repository tests pass through `cargo test -p engram-store-sql`.

### T2: SQL service write semantics match the adapter contract

**Depends on:** T1

**Tests:**
- A valid write creates one memory and one written event.
- An idempotent retry returns `deduplicated: true` and does not append a second
  written event.
- Concurrent writes with the same scoped idempotency key create one memory and
  one written event.

**Approach:**
- Add `SqlMemoryService` in `engine.rs` as the public service facade.
- Keep validation in `validation.rs` and write orchestration in `write.rs`.
- Keep the SQL write transaction in a focused storage module.
- Use injected clock, ID generator, and policy authorizer collaborators.

**Done when:** SQL service write/idempotency tests pass.

### T3: SQL retrieval and forget behavior match the baseline

**Depends on:** T2

**Tests:**
- Keyword retrieval returns the expected SQL-backed memory and explanation.
- Delete forget behavior removes the memory from active retrieval.

**Approach:**
- Add retrieval baseline behavior in `retrieval.rs`.
- Add delete behavior in `forget.rs`.
- Reuse `scope.rs` to keep tenant, subject, and workspace boundaries visible.

**Done when:** SQL service retrieval and forget tests pass.

### T4: SQL adapter passes evaluation fixtures and roadmap bookkeeping

**Depends on:** T3

**Tests:**
- `engram-eval::MemoryFixtureRunner` passes against `SqlMemoryService`.
- Repository validation commands pass after documentation updates.

**Approach:**
- Add a SQL-backed evaluation fixture test.
- Mark Phase 6 done and Phase 7 in progress in roadmap artifacts.
- Update changelog, roadmap, README, and technical debt notes.

**Done when:** full repository gates pass, excluding only unrelated local hook
failures from untracked Codex assets.

## Rollout

This ships as Rust workspace code and documentation on the implementation
branch. There is no runtime rollout, migration, external service dependency, or
irreversible production operation in this slice.

## Risks

- The SQL retrieval baseline intentionally scans persisted memories and is not a
  production ranking/indexing design.
- File-backed SQLite construction remains technical debt because ADR-0006 only
  required in-memory SQLite for the first conformance slice.
- Native bindings are not covered by this slice, so TypeScript consumers cannot
  call the SQL service yet.

## Changelog

- 2026-06-29: initial plan for the Phase 6 SQL service conformance slice.
