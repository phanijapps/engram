# v1 Changelog

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
