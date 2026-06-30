# ADR 0005: Storage Adapter Semantics

## Status

Accepted

## Context

Engram now has core ports and an in-memory adapter. Durable SQL and vector
adapters are intentionally deferred, but their required behavior must be clear
before implementation starts. Otherwise each adapter could make incompatible
choices about idempotency, lifecycle events, scope isolation, and partial
failure.

## Decision

All memory storage adapters must preserve the same behavior as the in-memory
adapter fixtures before they are accepted.

### Write Transactions

A successful memory write is one logical transaction:

- validate the request
- enforce write policy
- create the memory record
- append the written event
- record the idempotency key when supplied

If any step fails, the adapter must not leave behind a partial memory, event, or
idempotency record.

### Idempotency

Write idempotency is scoped by:

- tenant
- subject when supplied
- workspace when supplied
- idempotency key

An idempotent retry returns the original `WriteMemoryResponse` with
`deduplicated: true`. It must not append another written event.

Durable adapters must enforce this atomically under concurrent writes.

### Event Ordering

Adapters must preserve append order for lifecycle events as observed by that
adapter. Event IDs remain opaque; callers must not infer ordering from ID text.

Event reads must apply scope boundaries before returning data.

### Scope Isolation

Adapters must never return memories or events outside the requested tenant.
Optional scope fields narrow visibility when supplied by the request. A request
with `workspace: "a"` must not see records from `workspace: "b"`.

### Retrieval Baseline

Adapters that implement retrieval must first pass the exact and keyword fixture
set before adding semantic, vector, graph, or hierarchy retrieval. Unsupported
advanced retrieval modes must not become a hard dependency for v1 conformance.

### Contract Preservation

Adapters may choose their own tables, indexes, files, or internal records, but
they must preserve the accepted v1 contract payloads losslessly at the port
boundary. Storage-specific fields must not be added to portable domain types.

## Consequences

- SQL implementation starts with conformance fixtures, not schema design.
- Vector indexes are secondary indexes, not canonical storage.
- Event history is part of the adapter contract, not an optional debug feature.
- The in-memory adapter is the executable behavior baseline for later adapters.
