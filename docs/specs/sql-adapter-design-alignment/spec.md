# Spec: SQL Adapter Design Alignment

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0005, ADR-0006
- **Brief:** none
- **Contract:** none
- **Shape:** documentation alignment

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram's SQL adapter documentation accurately reflects the implemented SQLite
adapter design: ADR-0005 storage semantics, ADR-0006 SQLite selection,
in-memory and file-backed constructors, reusable fixture conformance, and
deferred server database work.

## Boundaries

### Always do

- Document the current SQL adapter module boundaries and behavior contracts.
- Keep server database adapters explicitly deferred.
- Correct stale documentation that says file-backed SQLite is still out of
  scope.
- Keep the roadmap queue aligned with shipped SQL work.

### Ask first

- Add PostgreSQL or another server database adapter.
- Add migration tooling or connection pooling.
- Change SQL schema or repository behavior.
- Change accepted v1 contracts.

### Never do

- Change SQL runtime behavior in this documentation slice.
- Move SQL-specific concerns into `engram-core` or `engram-domain`.
- Treat SQLite as a universal production database decision.
- Hide ADR-0005 conformance requirements behind implementation details.

## Testing Strategy

- Documentation checks confirm the new design reference is valid.
- Diff review confirms no Rust, TypeScript, contract schema, or generated file
  behavior changed.

## Acceptance Criteria

- [x] A SQL adapter design reference doc exists and cites ADR-0005/ADR-0006
  boundaries.
- [x] `engram-store-sql` README reflects file-backed SQLite as current scope.
- [x] The implementation roadmap shipped slice and near-term queue are current.
- [x] No runtime code, schema, generated TypeScript, or adapter behavior changes.

## Assumptions

- Technical: current SQL implementation already supports in-memory and
  file-backed SQLite construction.
- Technical: server database adapters remain future work until a new ADR or
  spec accepts them.
- Process: documentation alignment can close the stale roadmap queue item
  without changing code.
