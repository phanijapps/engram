# v1 Compatibility Rules

After v1 acceptance, compatible changes are limited to:

- Add an optional field.
- Add a new schema file for a new operation or payload.
- Add a new enum value only when consumers are explicitly required to tolerate
  unknown values for that enum.
- Add examples or specs that do not change existing field meaning.
- Deprecate a field without removing it.
- Tighten explanatory text without changing behavior.

Breaking changes require v2:

- Rename a field.
- Remove a field.
- Change field meaning.
- Change enum value meaning.
- Make an optional field required.
- Make a nullable/omittable field non-nullable or non-omittable.
- Change identifier semantics.
- Change required policy or provenance behavior.
- Add storage-specific, provider-specific, language-specific, or gateway-specific
  concepts to the portable contract.

V1 consumers must treat identifiers as opaque strings. They must not parse
tenant, workspace, timestamps, or storage locations from IDs.

`metadata` may carry arbitrary JSON-compatible values, but no v1 behavior may
depend on metadata keys unless a later version promotes those keys to typed
fields.

## Enum extensions applied

- **RetrievalTargetType** (RFC-0013 Phase 1, 2026-07-13): added `rule`,
  `policy`, `axiom`, `decision_trace`. Consumers must tolerate unknown
  `RetrievalTargetType` values per the enum-add rule above; exhaustive `match`
  sites require a wildcard arm or updated arms.
