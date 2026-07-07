# Plan: Runtime Adapters

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Start with `@engram/adapters` as a small observability wrapper around
`EngramTransport`. This makes runtime integration useful without choosing an
agent framework, server framework, queue, or telemetry backend too early.

Tempted to add framework adapters immediately; declining because the shared
transport boundary should stabilize first. Tempted to add OpenTelemetry
exporters; declining because exporter choice is application-specific and should
be composed outside the core adapter package.

## Constraints

- TypeScript adapters must not redefine domain truth.
- `packages/adapters/src/index.ts` remains a public facade.
- Observer failures must not alter the underlying Engram operation result.

## Construction tests

**Unit tests:** observed transport wrapper event ordering, retrieval traces, and
policy-denial classification.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Use `EngramTransport` from `@engram/client` as the wrapped interface. Event
payloads may include operation names, durations, counts, and sanitized error
classification, but not new domain records.

### Component / module decomposition

- `events.ts` owns adapter event types and error classification.
- `observer.ts` owns safe observer dispatch.
- `observed-transport.ts` owns transport wrapping.
- `index.ts` re-exports the public facade only.

### Failure, edge cases & resilience

Observer exceptions are isolated from transport behavior. Transport failures are
re-thrown after emitting an error event.

## Tasks

### T1: Observable transport adapter package

**Depends on:** PHASE07 TypeScript client.

**Tests:**
- Success path preserves write/retrieve/forget responses and emits ordered
  events.
- Retrieval trace includes counts for returned, omitted, and failed sources.
- Policy-denial-shaped failures emit `policy_denial`.

**Approach:**
- Add package metadata and TypeScript config.
- Add focused modules for event typing, observer dispatch, and wrapping.
- Add unit tests and package README.

**Done when:** package typecheck, tests, build, and repository gates pass.

## Rollout

Library code only. Framework packages can wrap this adapter later.

## Risks

- A generic event surface can become too abstract; keep fields tied to actual
  client operations and generated payload shapes.

## Changelog

- 2026-06-29: initial plan for framework-neutral runtime adapter events.
- 2026-06-29: shipped `@engram/adapters` observed transport utilities.
