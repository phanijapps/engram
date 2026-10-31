# Accepted behavior — memory contract fixtures

> Behavior authority for the memory write/retrieve contract, migrated from the
> `memory-contract-fixture-runners` feature spec. The enforcing code + JSON
> examples below are normative; this file documents the contract they enforce.

`engram-eval` ships **adapter-neutral** fixture runners so in-memory, SQL, native,
and future adapters exercise the same portable write/retrieve behavior without
copying one-off test logic.

- **`MemoryContractRunner`** (`core/eval/src/contract_runner.rs`) — constructed
  over `Arc<dyn MemoryService>`; methods `write_accepted_example()`,
  `write_request(request)`, and `retrieve_accepted_example()` (write + retrieve
  as one contract flow, returning `RetrievalContractOutcome { write, context }`).
  It intentionally knows nothing about storage, events, repositories, or adapter
  construction.
- **Accepted-example loaders** (`core/eval/src/accepted_examples.rs`) `include_str!`
  the checked-in `contracts/v1/examples/` files: `write-memory-request.json`,
  `retrieval-request.json`, and the invalid examples
  `write-memory-request.missing-scope-tenant.json`,
  `write-memory-request.training-export.json` (structurally valid but rejected by
  service validation), and `retrieval-request.missing-requester.json`.

**Invariants.** Fixture execution is expressed exclusively through `MemoryService`;
invalid deserialization is visible to tests without invoking a store (schema-level
required fields enforced at the boundary); policy, provenance, and lifecycle
assertions are not hidden behind generic success; `engram-eval` does not depend on
concrete store crates outside dev-tests.

**Proof.** `cargo test -p engram-eval`; `cargo test -p engram-store-sqlite --test service`
(`adapters/sqlite/tests/service.rs` consumes the runner via
`SqlMemoryService::open_in_memory()`).
