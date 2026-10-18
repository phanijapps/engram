# Performance

Recorded performance work: methodology, local smoke benchmarks, and dated
comparison runs. Raw artifacts stay in this directory (`.json` data +
`.md` write-ups).

## Local SQLite Smoke

Run:

```bash
cargo run -p engram-store-sql --example benchmark_sql
```

The example writes a fixed number of synthetic memories through
`MemoryService::write_memory`, retrieves a keyword query through
`MemoryService::retrieve`, and prints wall-clock elapsed milliseconds.

Example output shape:

```text
engram local benchmark smoke
adapter=sqlite-in-memory
memories_written=250
write_elapsed_ms=...
retrieved_items=...
retrieve_elapsed_ms=...
note=local timing only; not a performance claim
```

## Claim Boundaries

- Do not compare Engram to other systems from this smoke output.
- Do not use one workstation run as production capacity evidence.
- Do not add latency thresholds until benchmark fixtures, datasets, and target
  environments are specified.
- Do not run model-download or hosted-provider benchmarks in default validation.

## Recorded runs in this directory

- `PERFORMANCE.md` — lazy embeddings vs knowledge-graph structural retrieval study
- `embedding-*-2026-10-16.*` — configurable embedding-model comparison matrix (PR #125 era)
