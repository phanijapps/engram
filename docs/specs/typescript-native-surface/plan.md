# Plan: TypeScript Native Surface

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Start from generated contracts, then add the smallest native bridge that proves
payload round trips through Rust behavior. Keep `@engram/client` ergonomic but
thin: it owns TypeScript-facing composition and errors, not persistence,
ranking, policy, or domain lifecycle rules. Defer gateway adapters and broader
runtime integrations until the binding surface is mechanically proven.

Tempted to build a full gateway server; declining because this phase is a
library/binding surface. Tempted to make the TypeScript client a parallel
engine; declining because ADR-0003 makes Rust the behavior owner. Tempted to
bundle framework adapters immediately; declining because `packages/adapters`
belongs after the stable client surface.

## Constraints

- ADR-0003 requires Rust core behavior and TypeScript bindings/SDK packages.
- `AGENTS.md` requires TypeScript public APIs to be typed from generated
  contracts and prohibits domain truth redefinition.
- Existing package scripts use pnpm, TypeScript, tsup, and Vitest.
- Native binding implementation must not pull Node or TypeScript dependencies
  into `engram-domain` or `engram-core`.

## Construction tests

**Integration tests:** native round-trip tests cover write, retrieve, and forget
payloads through Rust-backed behavior once the binding crate/package exists.

**Manual verification:** package import smoke tests can be used for published
native artifacts. The current CI-friendly slice uses Rust unit tests for native
round trips and Vitest deterministic binding doubles for package behavior.

## Design (LLD)

### Design decisions

The binding surface starts narrow and mirrors existing memory service flows.
Advanced hierarchy, belief, sleep-cycle, vector, and gateway behaviors stay out
until their Rust contracts exist.

### Data & schema

`@engram/contracts` exports generated TypeScript types and schemas from
accepted contract artifacts. Client and native packages consume these types
instead of defining local domain models.

### Interfaces & contracts

`@engram/node` exposes native functions for memory operations backed by Rust
behavior. `@engram/client` wraps those calls in a stable ergonomic SDK that
returns generated payload types.

### Component / module decomposition

- `crates/engram-node` owns native bridge code and Rust serialization
  round-trips.
- `packages/node` owns native package metadata, exports, loading behavior, and
  JSON transport wrapping.
- `packages/client` owns public SDK methods and client construction over
  injected or native transports.
- `packages/contracts` owns generated types and schemas.
- `packages/eval` owns fixture authoring helpers after the native/client surface
  can run fixtures.

### State & control flow

TypeScript callers pass generated request payloads to the client, the client
delegates to the native package, the native package calls Rust behavior, and
responses return as generated contract payloads.

### Failure, edge cases & resilience

Native loading failures and Rust errors are translated into typed client errors
without hiding the underlying operation kind. Contract validation failures fail
before native execution when they can be detected at the TypeScript boundary.

### Dependencies & integration

The phase may add the native binding crate and package dependencies required by
the selected binding technology. Framework and gateway dependencies remain out
of scope.

## Tasks

### T1: Contracts package remains generated-only

**Depends on:** none

**Tests:**
- `pnpm run contracts:check-generated` passes.
- TypeScript package tests confirm exported schema and type entry points.

**Approach:**
- Keep generated outputs under `packages/contracts/src/generated`.
- Keep `packages/contracts/src/index.ts` as a facade.

**Done when:** generated contract checks and package tests pass.

### T2: Native bridge round trips memory payloads

**Depends on:** T1

**Tests:**
- Rust/native tests pass write, retrieve, and forget payloads through Rust domain
  or service behavior and assert accepted response shapes.

**Approach:**
- Add `crates/engram-node` with a narrow binding surface.
- Keep native bridge behavior focused on serialization and service invocation.
- Avoid adding native dependencies to domain or core crates.

**Done when:** native round-trip tests pass and Rust workspace gates remain
green.

### T3: Node package loads the native artifact through a facade

**Depends on:** T2

**Tests:**
- `packages/node` typecheck and tests import the package and call a smoke
  operation.

**Approach:**
- Add package exports in `packages/node`.
- Move loading, error normalization, and operation wrappers into focused
  modules behind a narrow `index.ts`.

**Done when:** node package build, typecheck, and tests pass.

### T4: Client package exposes ergonomic memory operations

**Depends on:** T3

**Tests:**
- `packages/client` tests exercise write, retrieve, and forget through the node
  package or a declared deterministic native equivalent.

**Approach:**
- Keep `packages/client/src/index.ts` as a facade.
- Split client operations, validation, native transport calls, and error
  translation into focused modules.

**Done when:** client package build, typecheck, tests, and full repository gates
pass.

## Rollout

This ships as library packages and a Rust native bridge on the implementation
branch. There is no service deployment or production data migration. Published
native artifact naming and CI matrix packaging remain future release work.

## Risks

- Native packaging can be platform-sensitive and may require CI matrix work.
- Premature client convenience helpers can accidentally duplicate Rust behavior.
- Binding technology choice may need an ADR if the implementation adds a major
  dependency or package-build convention.

## Changelog

- 2026-06-29: initial plan for the TypeScript/native binding phase.
- 2026-06-29: implemented NAPI JSON bridge, node transport package, and native
  client helper.
