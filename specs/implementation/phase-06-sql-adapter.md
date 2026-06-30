# Phase 06 Spec: SQL Persistence Adapter

## Status

Done for the SQLite in-memory service baseline.

## Scope

Add durable SQL persistence without changing accepted domain contracts.

## Acceptance

- SQL passes the same write/retrieve/forget/evaluation fixtures as memory.
- Idempotency is atomic under concurrent writes.
- Event ordering is stable and queryable.
- Migrations follow ADR 0005.
