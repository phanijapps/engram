# Spec: Policy Enforcement

## Contract Types

- `Scope`
- `Requester`
- `Policy`
- `ContextPayload`
- `OmittedResult`
- `ForgetResult`

## Required Behavior

- `tenant` is the hard isolation boundary in v1.
- `subject`, `workspace`, `session`, and `environment` are optional policy
  inputs. Their absence must not be treated as wildcard permission by default.
- `visibility` controls who may retrieve a record.
- `retention` controls whether a record is eligible for expiration or legal
  hold behavior.
- `sensitivity` may further restrict retrieval and debugging access.
- `allowedUses` limits use categories when present.
- When `allowedUses` is absent, v1 allows safe internal use except
  `training_export`.

## Forbidden Behavior

- Do not allow `training_export` as a v1 `allowedUses` value.
- Do not rely on metadata keys for core policy decisions.
- Do not compose context before policy filtering.
- Do not expose denied content in failure messages.

## Acceptance Checks

- A requester from another tenant retrieves no items.
- A record without `retrieval` in `allowedUses` is omitted from retrieval.
- A redacted record is not returned as normal content.
- A legal-hold record is not physically deleted by a forget request unless a
  future governance contract permits it.
