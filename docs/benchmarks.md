# Benchmarks

Engram does not publish performance claims yet. Benchmark commands in this file
are local smoke paths for collecting observations while correctness fixtures and
adapter contracts continue to stabilize.

## Local In-Memory Smoke

Run:

```bash
cargo run -p engram-store-memory --example benchmark_local
```

The example writes a fixed number of synthetic memories through
`MemoryService::write_memory`, retrieves a keyword query through
`MemoryService::retrieve`, and prints wall-clock elapsed milliseconds.

Example output shape:

```text
engram local benchmark smoke
adapter=in-memory
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
