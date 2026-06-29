# Spec: Forget Memory

## Contract Types

- `ForgetRequest`
- `ForgetResult`
- `MemoryEvent`
- `DeleteMode`

## Preconditions

- Request body validates against `contracts/v1/schemas/forget-request.schema.json`.
- `targetType`, `targetId`, `scope`, `requester`, and `mode` are present.
- `scope.tenant` is present and non-empty.

## Required Behavior

- `mode: delete` removes the target when policy allows physical deletion.
- `mode: redact` removes or replaces sensitive content while preserving allowed
  audit metadata.
- `mode: tombstone` prevents normal retrieval while retaining an audit marker.
- `mode: archive` retains the record but makes it ineligible for normal
  retrieval unless `includeArchived` is explicitly supported.
- A successful forget operation may produce a `MemoryEvent` with
  `kind: forgotten`, `redacted`, or another accepted lifecycle event.

## Forbidden Behavior

- Do not return forgotten records through normal retrieval.
- Do not leak redacted content through links, explanations, metadata, or
  evaluation output.
- Do not apply forget behavior across tenants.
- Do not silently downgrade a requested `delete` to `archive` without returning
  a status that makes the outcome visible.

## Acceptance Checks

- Valid request example: `contracts/v1/examples/forget-request.json`.
- Valid result example: `contracts/v1/examples/forget-result.json`.
- Missing requester is rejected.
- Cross-tenant forget is denied or not found.
- Forgotten memory is absent from subsequent normal retrieval.
