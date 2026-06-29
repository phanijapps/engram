# Roadmap

The detailed implementation sequence lives in
`docs/implementation-roadmap.md`. Keep this file as the short status view and
use the implementation roadmap as the spec-driven execution loop.

## Current: Spec-Driven Implementation

- Keep v1 schemas, examples, and specs coherent.
- Validate Rust projections against accepted wire contracts.
- Maintain generated-contract drift checks in CI.
- Keep write-memory behavior covered by executable fixtures.
- Keep retrieve-context behavior covered by exact/keyword fixtures.
- Keep forget lifecycle behavior covered by delete/redact/tombstone/archive
  tests.
- Keep evaluation fixtures executable through the Rust fixture runner.
- Keep concrete adapters outside `engram-core`.

## Next: SQL Adapter Conformance

- Reusable adapter conformance fixture utilities.
- SQLite-backed SQL repository adapter.

## Later

- SQL and vector adapters.
- Code and document ingestion.
- Belief network.
- Hierarchy navigation.
- Consolidation and sleep-cycle behavior.
