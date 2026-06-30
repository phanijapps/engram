# SQL Adapter Design

`engram-store-sql` is the first durable memory storage adapter. It implements
the `MemoryService`, `MemoryRepository`, and `MemoryEventRepository` ports over
SQLite while preserving accepted v1 domain payloads at the boundary.

## Decisions

- ADR-0005 defines storage adapter semantics: atomic writes, scoped
  idempotency, event ordering, scope isolation, retrieval baseline conformance,
  and contract preservation.
- ADR-0006 selects SQLite as the first SQL target so conformance can run without
  external infrastructure.
- SQLite is a local durable baseline, not the final production database decision
  for every deployment.

## Current Scope

- In-memory SQLite service construction for conformance tests and examples.
- File-backed SQLite service construction for local durable smoke tests.
- Schema initialization on open.
- Memory records stored as accepted contract JSON plus scope/index columns.
- Lifecycle events stored as accepted contract JSON plus scope/index columns.
- Scoped idempotency keys enforced through SQL uniqueness.
- SQL-backed write, retrieve, forget, event lookup, and evaluation fixture
  behavior.

## Module Boundaries

- `engine.rs` composes the SQL store with policy, clock, and ID dependencies.
- `service.rs` owns repository-level SQLite persistence helpers.
- `schema.rs` owns table creation and SQLite error translation.
- `transactional_write.rs` owns the atomic write transaction.
- `write.rs`, `retrieval.rs`, and `forget.rs` own operation orchestration.
- `validation.rs` mirrors behavior-level request validation from the
  in-memory baseline.
- `scope.rs` owns scope matching and SQL scope binding helpers.

Crate roots stay as facades. SQL table design, SQLite errors, and transaction
details must not leak into `engram-domain`, `engram-core`, TypeScript bindings,
or portable v1 schemas.

## Conformance

The SQL adapter must continue to pass the same observable behavior expectations
as the in-memory adapter before adding richer database features:

- valid writes persist one memory and one written event
- idempotent retries return the original response without duplicate events
- concurrent scoped idempotent writes stay atomic
- retrieval applies scope and policy before ranking
- forget lifecycle behavior appends audit events and prevents normal leakage
- evaluation fixtures run through `engram-eval`

## Deferred Work

- PostgreSQL or other server database adapters.
- Connection pools and async SQL runtimes.
- Versioned migration tooling for deployed databases.
- SQL-native ranking indexes beyond the deterministic baseline.
- Knowledge-source persistence through SQL tables.

Each deferred item needs a new spec or ADR before implementation.
