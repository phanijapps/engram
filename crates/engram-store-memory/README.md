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
- exact and keyword retrieve-context baseline
- memory record lookup
- memory lifecycle event lookup
- permissive policy authorizer for tests
- injectable clock and ID generator dependencies

Module boundaries:

- `lib.rs`: crate facade and public re-exports only.
- `dependencies.rs`: default local clock, ID generator, and permissive policy.
- `service.rs`: service construction plus repository and event trait impls.
- `write.rs`: write-memory transaction behavior.
- `retrieval.rs`: exact/keyword retrieval, scoring, omissions, and composition.
- `state.rs`: private process-local storage shape.
- `scope.rs`: shared scope visibility checks.
- `validation.rs`: behavior-level request validation.

Out of scope:

- SQL persistence
- vector indexes
- embedding providers
- TypeScript bindings
- background consolidation workers
