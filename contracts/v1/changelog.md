# v1 Changelog

## 2026-06-30

- Added accepted retrieval evaluation fixtures for positive recall, forbidden
  recall, budget-constrained retrieval, and no-result behavior.
- Added accepted forget request/result examples for delete, redact, and archive
  alongside the existing tombstone examples.

## 2026-06-29

- Accepted the v1 core memory contract.
- Versioned schemas under `contracts/v1/schemas/`.
- Added operation examples under `contracts/v1/examples/`.
- Deferred belief, contradiction, hierarchy, taxonomy evolution, consolidation,
  and ingestion execution behavior to extension contracts.
- Excluded `training_export` from v1 `AllowedUse`.
- Required `MemoryRecord.status`.
- Kept `Scope.subject` optional.
- Kept embedded `Actor` in `Provenance`.
