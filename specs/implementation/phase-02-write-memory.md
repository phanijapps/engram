# Phase 02 Spec: Write Memory Completion

## Status

Done for the in-memory baseline.

## Scope

Implement contract-backed write behavior with validation, policy, idempotency,
record persistence, event persistence, and executable fixtures.

## Acceptance

- Valid writes create one active memory and one written event.
- Invalid and denied writes create no memory or event.
- Idempotent retries return the original response without appending an event.
- Accepted and invalid v1 examples execute as fixtures.
