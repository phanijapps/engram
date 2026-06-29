# Changelog

This project follows a contract-first changelog while pre-1.0.

## Unreleased

- Accepted the v1 core memory contract.
- Added Rust 2024 workspace scaffolding for domain and core ports.
- Added contract validation, examples, invalid examples, and spec templates.
- Added generated TypeScript contract drift checks.
- Added Rust schema conformance tests for accepted v1 payloads.
- Added the first spec-driven write-memory slice with an in-memory service.
- Added the `engram-store-memory` crate for process-local adapter behavior.
- Moved concrete in-memory write state out of `engram-core`.
- Added memory event query contracts and deterministic clock/ID injection for
  the in-memory write path.
- Added executable write-memory fixture tests against accepted v1 examples.
- Added exact and keyword retrieval behavior to the in-memory adapter.
- Added retrieval tests for scope isolation, policy omission, explanations, and
  budget omissions.
- Documented storage adapter transaction, idempotency, event, and scope
  semantics before SQL implementation.
