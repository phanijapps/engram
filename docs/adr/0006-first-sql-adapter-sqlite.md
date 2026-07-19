# ADR 0006: First SQL Adapter Uses SQLite

## Status

Accepted

## Context

The SQL adapter needs to prove durable semantics before Engram commits to a
server database. The first adapter should run in CI, require no external
service, and exercise transactions, idempotency, event persistence, and replay
against the same contract fixtures as the in-memory adapter.

## Decision

Use SQLite as the first SQL adapter target through `engram-store-sql`.

The adapter will start with an in-memory SQLite connection for tests and local
fixtures. File-backed SQLite can be added behind the same module boundary. A
server database adapter can follow after the repository and fixture contracts
are stable.

## Consequences

- CI can run SQL conformance without provisioning infrastructure.
- The adapter still exercises real SQL transactions and uniqueness constraints.
- SQLite is not the final production database decision for every deployment.
- Future PostgreSQL or other adapters must pass the same behavior fixtures.
