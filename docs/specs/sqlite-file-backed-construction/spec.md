# Spec: SQLite File-Backed Construction

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0005, ADR-0006
- **Brief:** none
- **Contract:** none
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

`engram-store-sql` supports opening a file-backed SQLite store and service with
the same schema initialization and behavior as the existing in-memory
constructors, so local durable smoke tests can persist memory records across
service instances without introducing a server database.

## Boundaries

### Always do

- Keep SQLite as the first SQL adapter target.
- Initialize the existing schema on every open.
- Preserve the same repository and service behavior as in-memory construction.
- Keep file-backed construction behind the SQL adapter boundary.
- Use standard library temporary paths in tests; do not add test-only crates.

### Ask first

- Add PostgreSQL, connection pools, migrations, or async SQL runtimes.
- Change SQL schema semantics beyond initialization on open.
- Add file-backed paths to portable contracts or domain types.

### Never do

- Leak filesystem paths into memory records, events, or v1 contracts.
- Make file-backed SQLite the only constructor.
- Move SQL behavior into `engram-core` or `engram-domain`.
- Add destructive cleanup behavior to constructors.

## Testing Strategy

- TDD: a SQL service test writes through one file-backed service, reopens the
  same path, and retrieves the persisted memory.
- Regression: existing in-memory SQL repository and service tests continue to
  pass unchanged.
- Goal-based: `cargo check -p engram-store-sql --tests` proves the public
  constructors compile.

## Acceptance Criteria

- [x] `SqlMemoryStore` exposes a file-backed constructor.
- [x] `SqlMemoryService` exposes a file-backed constructor using default local
  dependencies.
- [x] Reopening the same SQLite path preserves written memory records.
- [x] Existing in-memory constructors and tests continue to pass.
- [x] No public v1 schema, generated contract, or portable domain type changes.

## Assumptions

- Technical: ADR-0006 explicitly allows file-backed SQLite behind the same SQL
  adapter boundary.
- Technical: `rusqlite::Connection::open` initializes or opens a SQLite file
  without external services.
- Technical: the existing `initialize_schema` function is idempotent for an
  existing SQLite database file.
