# Phase 06 Plan: SQL Persistence Adapter

SQLite is accepted as the first SQL engine in ADR 0006. The current slice
implements schema initialization, memory/event repository ports, SQL-backed
`MemoryService` orchestration, idempotent writes, retrieval, forget behavior,
and evaluation fixture conformance.
