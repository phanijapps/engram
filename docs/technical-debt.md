# Technical Debt

This log records known gaps that should not block the current implementation
loop but must stay visible.

## SQL Adapter File-Backed Construction

Phase: `PHASE06`

`engram-store-sql` currently supports in-memory SQLite for CI conformance. That
is enough to prove SQL semantics without provisioning infrastructure.

Remaining work:

- add file-backed SQLite construction after in-memory conformance is stable
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
