# engram-store-memory

`engram-store-memory` provides process-local implementations of Engram core
ports for specs, examples, and deterministic vertical slices.

This crate may own concrete in-memory state, local clocks, local ID generators,
and adapter-specific test helpers. It must not become the production persistence
contract. Durable storage belongs in future adapter crates such as
`engram-store-sql`, and those adapters should pass the same behavior fixtures as
this crate.

Current scope:

- v1 write-memory slice
- memory record lookup
- memory lifecycle event lookup
- permissive policy authorizer for tests
- injectable clock and ID generator dependencies

Out of scope:

- SQL persistence
- vector indexes
- embedding providers
- TypeScript bindings
- background consolidation workers
