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
