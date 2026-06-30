# Technical Debt

This log records known gaps that should not block the current implementation
loop but must stay visible.

## SQL Server Database Adapter

Phase: `PHASE42`

`engram-store-sql` supports in-memory and file-backed SQLite for local durable
conformance. Server database adapters remain a separate decision.

Remaining work:

- decide whether server database adapters belong in this crate or sibling
  adapter crates

## Vector Hybrid Fusion

Phase: `PHASE09`

`engram-store-vector` currently proves sqlite-vec storage and nearest-neighbor
queries with fixed vectors. FastEmbed BGE-small coverage exists as an opt-in
smoke test because model downloads should not happen in the default gate.

Remaining work:

- define hybrid fusion policy across keyword, vector, recency, provenance, and
  hierarchy signals
- decide how vector source failures appear in `ContextPayload`
- revisit the `sqlite-vec` dependency pin after crates.io publishes a non-broken
  release newer than `0.1.9`
