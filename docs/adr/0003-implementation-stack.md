# ADR 0003: Implementation Stack

## Status

Accepted

## Context

engram needs deterministic core behavior, portable contracts, and ergonomic
integration with TypeScript applications and agent gateways. The repository has
already selected a contract-first architecture with adapters for storage,
retrieval, ingestion, and model providers.

## Decision

Use Rust 2024 for the core implementation and TypeScript for bindings, SDKs,
and application-facing integrations.

Initial Rust crates:

- `engram-domain`: portable domain data models, enums, value objects,
  serialization rules, and lightweight invariants.
- `engram-core`: behavior traits and ports for memory writes, retrieval,
  forgetting, ingestion, consolidation, policy, and evaluation.

Initial TypeScript packages will be added after Rust contracts stabilize:

- `@engram/contracts`: generated TypeScript types and schemas.
- `@engram/node`: native binding package.
- `@engram/client`: ergonomic SDK.
- `@engram/adapters`: framework and gateway integrations.
- `@engram/eval`: evaluation helpers and CLI wrappers.

## Consequences

- Rust domain types become the implementation source for generated contracts
  once the draft domain model is accepted.
- SQL, vector stores, model providers, and Node runtime details remain outside
  the domain crate.
- TypeScript must wrap and compose the Rust-backed contract rather than define a
  second domain model.
- Adapter crates can evolve independently behind stable core traits.
