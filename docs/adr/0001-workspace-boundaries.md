# ADR 0001: Workspace Boundaries

## Status

Accepted

## Context

The implementation language and framework are not yet selected. The project
needs a structure that supports research, architectural decisions, and interface
design before runtime-specific code appears.

## Decision

Use a language-neutral workspace organized around architecture documents,
contracts, and future package boundaries:

- `docs/` for research, RFCs, and ADRs.
- `contracts/` for portable interface and schema definitions.
- `packages/` for future implementation modules.
- `examples/` for scenarios and fixtures.

## Consequences

- Early work can proceed without committing to a stack.
- Architecture decisions remain explicit and reviewable.
- Runtime scaffolding can be added later without moving the design record.
- Some directories are placeholders until a language is selected.
