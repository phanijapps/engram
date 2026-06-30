# Planned Crate And Package Map

This map is a planning baseline, not an implementation lock. Change it through an ADR when the stack is accepted.

## Rust Workspace

- `engram-domain`: accepted domain types, invariants, serialization, version markers.
- `engram-runtime`: shared portable errors, result type, clocks, id generation, scope matching, and policy gates.
- `engram-memory`: memory service and repository ports for write, retrieve, forget, and lifecycle events.
- `engram-knowledge`: source-grounded knowledge, knowledge graph, ontology, source reader, chunker, and ingestion ports.
- `engram-core`: orchestration facade, retrieval pipeline, consolidation, hierarchy, belief, and evaluation ports.
- `engram-eval`: fixtures, deterministic harness, recall/leakage/ranking assertions.
- `engram-ingest`: source parsing interfaces, chunking contracts, document/code ingestion pipeline.
- `engram-hierarchy`: hierarchy build/maintenance algorithms, paths, expansion strategies.
- `engram-belief`: belief derivation, contradiction detection, consolidation tasks.
- `engram-store-memory`: in-memory memory adapter for quick tests and first vertical slices only.
- `engram-store-knowledge-memory`: in-memory knowledge, graph, and ontology adapter for conformance tests and examples only.
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
- Memory and knowledge ports live in their own crates so memory storage and knowledge graph storage can use different backends.
- In-memory memory and in-memory knowledge fixtures stay in separate crates so quick tests do not normalize mixed production storage.
- The Rust core crate composes ports, not concrete SQL/vector/graph/provider implementations.
- TypeScript may compose application workflows, but deterministic domain behavior should live in Rust.
- Adapters may translate infrastructure-specific errors into stable domain errors.
- Generated contracts should be reproducible from source, not edited manually.
