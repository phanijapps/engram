# Technical Debt

This log records known gaps that should not block the current implementation
loop but must stay visible.

## SQL Adapter Service Orchestration

Phase: `PHASE06`

`engram-store-sql` currently implements repository and event ports over SQLite.
It initializes schema, preserves memory/event payloads as contract JSON, applies
scope checks on reads, and tests event ordering.

Remaining work:

- implement SQL-backed `MemoryService` write orchestration
- enforce write idempotency through the `write_idempotency` table
- run write/retrieve/forget/evaluation fixtures against SQL service behavior
- add file-backed SQLite construction after in-memory conformance is stable
