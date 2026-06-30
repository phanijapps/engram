# engram-store-sql

`engram-store-sql` is the first durable persistence adapter for Engram memory
behavior. It starts with SQLite so conformance can run in CI without external
infrastructure.

Current scope:

- SQLite schema initialization
- in-memory SQLite construction for conformance tests and examples
- file-backed SQLite construction for local durable smoke tests
- memory record persistence as accepted contract JSON
- lifecycle event persistence as accepted contract JSON
- write idempotency through a unique SQL key
- repository reads for memory records and events
- SQL-backed `MemoryService` write, retrieve, forget, and evaluation behavior

Out of scope for the first slice:

- server database pooling
- vector indexes
- embedding providers
- migrations for multiple deployed versions

See `docs/sql-adapter-design.md` for the design boundary and deferred server
database work.
