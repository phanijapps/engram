# Contracts

This folder contains portable contracts for engram. Contracts are
implementation-neutral and must be usable from Rust, TypeScript, storage
adapters, HTTP/gateway layers, and evaluation tooling.

## Current Source Of Truth

- `v1/README.md`: accepted v1 scope.
- `v1/schemas/engram-v1.schema.json`: accepted v1 JSON Schema definitions.
- `v1/examples/`: valid operation and payload examples.
- `v1/compatibility.md`: compatibility rules.
- `v1/changelog.md`: contract history.

The files under `contracts/schemas/` are legacy pointers kept for early tooling.
Do not add new schemas there. Add versioned contracts under `contracts/v1/` or a
future `contracts/vN/` directory.
