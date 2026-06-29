# Spec: Write Memory

## Contract Types

- `WriteMemoryRequest`
- `WriteMemoryResponse`
- `MemoryRecord`
- `MemoryEvent`
- `Policy`
- `Provenance`

## Preconditions

- Request body validates against `contracts/v1/schemas/write-memory-request.schema.json`.
- `scope.tenant` is present and non-empty.
- `requester.actor` is present.
- `provenance.actor` is present.
- `policy.visibility` and `policy.retention` are present.
- `content.text` is present.

## Required Behavior

- A successful write produces a `MemoryRecord` with `status: active` unless a
  future version explicitly accepts another initial state.
- The written record preserves `scope`, `policy`, `provenance`, `kind`,
  `content`, and `links` from the request.
- The write produces a `MemoryEvent` with `kind: written`.
- The written event is queryable by memory ID and by visible scope through the
  memory event repository contract.
- If `idempotencyKey` matches an existing write in the same scope, the response
  may return the existing record with `deduplicated: true`.
- IDs returned by the implementation are opaque strings.

## Forbidden Behavior

- Do not infer tenant, workspace, subject, timestamps, or authorization from an
  identifier.
- Do not store policy or provenance only in `metadata`.
- Do not accept `training_export` in `policy.allowedUses` for v1.
- Do not create beliefs, hierarchy nodes, or consolidation runs as required v1
  side effects.

## Acceptance Checks

- Valid example: `contracts/v1/examples/write-memory-request.json`.
- Valid response example: `contracts/v1/examples/write-memory-response.json`.
- Missing `content.text` is rejected.
- Missing `scope.tenant` is rejected.
- Missing `provenance.actor` is rejected.
- A write outside requester policy returns a denied result or error without
  creating a retrievable memory or lifecycle event.
- An idempotent retry returns the original record and does not append a second
  written event.
