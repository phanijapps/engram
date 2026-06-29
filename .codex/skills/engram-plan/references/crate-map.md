# Planned Crate And Package Map

This map is a planning baseline, not an implementation lock. Change it through an ADR when the stack is accepted.

## Rust Workspace

- `engram-domain`: accepted domain types, invariants, serialization, version markers.
- `engram-core`: engine orchestration, policy gates, ports, retrieval pipeline, write path, forget path.
- `engram-eval`: fixtures, deterministic harness, recall/leakage/ranking assertions.
- `engram-ingest`: source parsing interfaces, chunking contracts, document/code ingestion pipeline.
- `engram-hierarchy`: hierarchy build/maintenance algorithms, paths, expansion strategies.
- `engram-belief`: belief derivation, contradiction detection, consolidation tasks.
- `engram-store-memory`: in-memory adapter for tests and first vertical slices.
- `engram-store-sql`: SQL persistence adapter after the domain and ports are stable.
- `engram-store-vector`: vector index adapter after retrieval interfaces stabilize.
- `engram-provider-embed`: embedding provider adapter traits and selected provider implementations.
- `engram-node`: N-API bridge exposing stable Rust APIs to TypeScript.

## TypeScript Workspace

- `@engram/client`: ergonomic TypeScript SDK for application callers.
- `@engram/contracts`: generated types and JSON schemas from Rust/domain contracts.
- `@engram/node`: native binding package wrapping `engram-node`.
- `@engram/adapters`: optional JS-side integrations for frameworks, tools, and gateway code.
- `@engram/eval`: fixture authoring helpers and CLI wrappers around the Rust eval harness.

## Boundary Rules

- The Rust domain crate must not depend on storage adapters, model providers, or TypeScript bindings.
- The Rust core crate depends on ports, not concrete SQL/vector/provider implementations.
- TypeScript may compose application workflows, but deterministic domain behavior should live in Rust.
- Adapters may translate infrastructure-specific errors into stable domain errors.
- Generated contracts should be reproducible from source, not edited manually.
