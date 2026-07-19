# Spec: TypeScript Native Surface

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003
- **Brief:** none
- **Contract:** contracts/v1/memory.schema.json
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

TypeScript consumers can use Engram through generated contract types, a native
binding package, and an ergonomic client package without receiving a second
implementation of memory behavior. The Rust core remains the behavior owner;
TypeScript packages validate, wrap, package, and compose the Rust-backed API for
application and adapter integrations.

## Boundaries

### Always do

- Export public TypeScript payload shapes from generated contracts.
- Treat the native package as a transport over Rust behavior.
- Keep `packages/client` as a narrow facade over generated contracts and native
  binding calls.
- Put framework-specific integrations in `packages/adapters`.

### Ask first

- Add or replace the native binding technology.
- Change generated contract source, accepted JSON schemas, or Rust domain
  serialization behavior.
- Add a gateway runtime, daemon, HTTP server, or framework adapter in the same
  slice.

### Never do

- Redefine memory, policy, provenance, hierarchy, belief, or lifecycle domain
  truth in TypeScript.
- Hide native calls, validation, retries, serialization, policy decisions, and
  fixture execution inside one monolithic client class.
- Put Node, N-API, or TypeScript tooling dependencies into `engram-domain` or
  `engram-core`.
- Ship binding behavior without round-trip tests against Rust domain types or a
  local engine fixture.

## Testing Strategy

- Generated contracts: goal-based checks through contract generation and
  TypeScript typecheck.
- Native binding round trips: TDD/integration tests that pass JSON payloads
  through Rust domain/service surfaces and assert accepted contract shape.
- Client ergonomics: goal-based package tests using Vitest against a local
  engine or deterministic native test double where the native artifact is not
  yet buildable.
- Packaging: goal-based checks through `pnpm run build`, `pnpm run typecheck`,
  `pnpm run test`, and Rust workspace gates.

## Acceptance Criteria

- [x] `@engram/contracts` remains generated from accepted contract artifacts and
  has no manual domain redefinitions.
- [x] `@engram/node` exposes a narrow native binding surface for write,
  retrieve, and forget flows backed by Rust behavior.
- [x] `@engram/client` composes generated types and native calls without owning
  persistence, policy, or domain logic.
- [x] Native payload round-trip tests prove Rust accepts and returns the same v1
  contract shapes TypeScript exports.
- [x] Client tests exercise write, retrieve, and forget flows through the local
  binding surface or a deterministic equivalent declared in the plan.
- [x] Package entry points remain facades with behavior split into focused
  modules for validation, native transport, client operations, adapters, and
  fixtures.

## Assumptions

- Technical: ADR-0003 selects Rust 2024 core plus TypeScript bindings and SDK
  packages (source: `docs/adr/0003-implementation-stack.md`).
- Technical: the TypeScript workspace uses pnpm, Node >=22, TypeScript, tsup,
  and Vitest (source: `package.json`).
- Technical: package placeholders exist for `packages/contracts`,
  `packages/client`, `packages/node`, `packages/adapters`, and `packages/eval`
  (source: `packages/` tree).
- Process: TypeScript wraps and composes Rust-backed contracts rather than
  defining domain truth (source: `AGENTS.md`).
- Product: TypeScript bindings are required for the Rust core, while exact
  client ergonomics remain open for implementation review (source: user
  confirmation 2026-06-29).
