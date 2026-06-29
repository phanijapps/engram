# Roadmap

The detailed implementation sequence lives in
`docs/implementation-roadmap.md`. Keep this file as the short status view and
use the implementation roadmap as the spec-driven execution loop.

## Current: Spec-Driven Implementation

- Keep v1 schemas, examples, and specs coherent.
- Validate Rust projections against accepted wire contracts.
- Maintain generated-contract drift checks in CI.
- Keep write-memory behavior covered by executable fixtures.
- Extend retrieve-context fixtures from the exact/keyword baseline.
- Keep concrete adapters outside `engram-core`.

## Next: Retrieval And Lifecycle Slices

- Forget memory slice.
- Evaluation fixture runner.
- Reusable adapter conformance fixture utilities.

## Later

- SQL and vector adapters.
- Code and document ingestion.
- Belief network.
- Hierarchy navigation.
- Consolidation and sleep-cycle behavior.
