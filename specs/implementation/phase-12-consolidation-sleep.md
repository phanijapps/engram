# Phase 12 Spec: Consolidation And Sleep Cycle

## Status

Draft.

## Scope

Run auditable consolidation tasks over bounded scopes.

## Acceptance

- Every durable mutation appears in a `ConsolidationRun`.
- Failed tasks are recoverable and inspectable.
- Protected evaluation fixtures do not regress after consolidation.
