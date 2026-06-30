# Phase 04 Spec: Forget Lifecycle

## Status

Done for the in-memory baseline.

## Scope

Implement `ForgetRequest` for memory targets in the in-memory adapter with
delete, redact, tombstone, and archive behavior.

## Acceptance

- Forget validates scope and target fields.
- Forget checks policy before mutation.
- Delete removes the memory from normal lookup and retrieval.
- Redact removes content and prevents content leakage.
- Tombstone prevents normal retrieval and preserves an audit event.
- Archive hides the memory unless retrieval explicitly includes archived data.
- Cross-tenant forget does not mutate another tenant's memory.
