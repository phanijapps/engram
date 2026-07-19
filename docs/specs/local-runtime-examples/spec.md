# Spec: Local Runtime Examples

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0006
- **Brief:** none
- **Contract:** none
- **Shape:** examples

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram provides checked local examples for the first usable runtime surfaces:
the process-local memory adapter, the SQLite-backed adapter, and the
TypeScript client facade over an injected transport. These examples must stay
thin, compile under normal validation, and point users to exact commands.

## Boundaries

### Always do

- Keep examples close to the package or crate that owns the API.
- Reuse accepted v1 contract fixtures instead of inventing parallel payloads.
- Keep Rust examples runnable with Cargo without requiring external services.
- Include TypeScript examples in package typechecking or tests.
- Keep examples as usage sketches, not second implementations of runtime
  behavior.

### Ask first

- Add new public API only to make examples cleaner.
- Add a runtime example that depends on a built native addon artifact.
- Add new package managers, task runners, or example-only frameworks.
- Change accepted v1 schemas or generated contract types.

### Never do

- Put adapter internals in top-level example code.
- Bypass policy or scope fields in example requests.
- Make FastEmbed/model downloads part of default example validation.
- Add a monolithic sample application that owns storage, transport, validation,
  and behavior in one module.

## Testing Strategy

- TDD: examples are added before status is marked shipped and must compile.
- Regression: existing workspace checks continue to pass.
- Goal-based: Rust adapter examples compile with `cargo check --examples`; the
  TypeScript client example is covered by package typecheck and a small test.

## Acceptance Criteria

- [x] The in-memory adapter has a runnable local write/retrieve example.
- [x] The SQLite adapter has a runnable local write/retrieve example.
- [x] The TypeScript client package has a checked injected-transport example.
- [x] `examples/README.md` lists exact commands for the runnable examples.
- [x] No new public domain contract or schema is introduced.
- [x] Default validation does not require FastEmbed downloads or a native addon.

## Assumptions

- Technical: `InMemoryMemoryService::new` is the local process adapter entry
  point.
- Technical: `SqlMemoryService::open_in_memory` is the SQLite local adapter
  entry point.
- Technical: `EngramClient` accepts an injected `EngramTransport`, allowing
  TypeScript examples to compile without a native addon artifact.
