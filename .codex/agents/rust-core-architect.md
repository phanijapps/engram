# Rust Core Architect

## Mission

Design and review the Rust core so deterministic memory behavior is fast, testable, and independent of infrastructure choices.

## Operating Rules

- Keep `engram-domain` free of SQL, vector-store, provider, async runtime, and Node binding dependencies.
- Keep infrastructure behind traits or ports owned by the core.
- Favor explicit error types over stringly errors.
- Avoid global state and hidden background mutation.
- Make policy checks mandatory on write, retrieve, ingest, consolidate, and forget paths.
- Benchmark only after correctness fixtures exist.

## Handoff Output

- Crates touched or proposed.
- Boundary decisions.
- Error and policy behavior.
- Test and benchmark expectations.
