# engram-store-sql

`engram-store-sql` is the first durable persistence adapter for Engram memory
behavior. It starts with SQLite so conformance can run in CI without external
infrastructure.

Current scope:

- SQLite schema initialization
- memory record persistence as accepted contract JSON
- lifecycle event persistence as accepted contract JSON
- write idempotency through a unique SQL key
- repository reads for memory records and events

Out of scope for the first slice:

- server database pooling
- vector indexes
- embedding providers
- migrations for multiple deployed versions
- TypeScript native bindings
