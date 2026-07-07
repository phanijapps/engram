# ADR 0004: Contract Generation Strategy

## Status

Accepted

## Context

Engram is contract-first and will later expose Rust and TypeScript surfaces.
The project needs a stable source of truth while the public contract is still
being hardened and before implementation behavior exists.

## Decision

Use a manually maintained v1 contract package with conformance tests for the
initial open-source phase.

The source of truth is:

- `docs/domain-data-model.md` for semantics.
- `contracts/v1/schemas/engram-v1.schema.json` for wire shape.
- `contracts/v1/examples/` for valid and invalid examples.
- `docs/specs/` for acceptance behavior.

Rust and TypeScript projections must conform to these artifacts. Rust types may
be promoted to the generation source later only after the v1 contract has proven
stable through spec-driven implementation.

## Consequences

- Contract review stays explicit and accessible to contributors who do not know
  Rust.
- CI can validate examples before any implementation crate changes.
- Generated TypeScript should come from accepted schemas, not hand-written
  duplicate models.
- A future ADR is required before switching to Rust-generated schemas.
