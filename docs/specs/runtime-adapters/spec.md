# Spec: Runtime Adapters

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** TypeScript package

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram has a framework-neutral TypeScript adapter package that can wrap an
existing client transport and emit structured observability events without
changing domain contracts or reimplementing memory behavior.

## Boundaries

### Always do

- Depend on stable `@engram/client` transport interfaces and generated
  `@engram/contracts` types.
- Keep framework-specific code out of the first slice.
- Report operation starts, operation successes, operation failures, retrieval
  traces, source failures, omissions, and policy-denial-shaped failures.

### Ask first

- Add agent-framework-specific adapters.
- Add HTTP servers, CLIs, queues, background workers, or telemetry exporters.
- Add new domain payloads or contract fields.

### Never do

- Redefine Engram domain objects in TypeScript.
- Hide transport failures as successful operations.
- Make adapters depend on Rust internals or native binding implementation
  details.

## Testing Strategy

- Unit-test observer event ordering around write, retrieve, and failure paths.
- Typecheck the package against `@engram/client` and `@engram/contracts`.
- Build the package with the same TypeScript workspace gates as other packages.

## Acceptance Criteria

- [x] `@engram/adapters` exposes a narrow public facade.
- [x] A transport wrapper emits structured observability events without
  changing operation results.
- [x] Retrieval successes include item, omission, and source-failure counts.
- [x] Policy-denial-shaped errors are classified distinctly.
- [x] The package has typecheck, test, and build scripts.

## Assumptions

- Technical: runtime integrations should consume client transports, not core
  crates directly (source: `packages/README.md`).
- Process: framework adapters can be added later without changing domain
  contracts (source: `docs/implementation-roadmap.md`).
