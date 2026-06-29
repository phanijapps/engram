# Roadmap

The detailed implementation sequence lives in
`docs/implementation-roadmap.md`. Keep this file as the short status view and
use the implementation roadmap as the spec-driven execution loop.

## Current: Spec-Driven Implementation

- Keep v1 schemas, examples, and specs coherent.
- Validate Rust projections against accepted wire contracts.
- Maintain generated-contract drift checks in CI.
- Complete write-memory behavior through executable fixtures.
- Keep concrete adapters outside `engram-core`.

## Next: Retrieval And Lifecycle Slices

- Retrieve context slice.
- Forget memory slice.
- Evaluation fixture runner.

## Later

- SQL and vector adapters.
- Code and document ingestion.
- Belief network.
- Hierarchy navigation.
- Consolidation and sleep-cycle behavior.
