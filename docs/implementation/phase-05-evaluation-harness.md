# Phase 05 Plan: Evaluation Harness

`engram-eval` provides a deterministic Rust runner that works against any
`MemoryService`. It seeds setup memories through normal writes, runs retrieval
cases, and reports missing targets, forbidden leaks, score failures, result
count failures, and missing explanations. TypeScript fixture authoring remains
future work.
