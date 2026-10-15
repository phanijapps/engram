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

## 2026-10-14 — Code source preservation

- Added optional code/documentKind extensions and typed file/byte-range,
  generation, contribution and source-read payloads under RFC-0021.
- Existing record meanings and chunk-kind enum values remain unchanged;
  source-reader/index activation is separately versioned and capability-gated.
- Added the SQLite knowledge source cell, fenced per-file generations,
  versioned scan sidecars, exact source reads, and typed `ImportData` source
  bundles. Reads are bound to trusted scope/requester context; source read,
  export, and import activation use separate authority checks. Snapshot chunks
  are excluded from normal knowledge candidate listing and model-document
  feeds.

## 2026-10-14 — Reliable code-intelligence shapes (compatible)

Add code evidence query/packet, strict partial/effective operator configuration,
source ranges, derived segment inventory, document heads and pinned model and
calibration descriptors. Canonical code role/inventory enums and existing v1
memory/recall payloads are unchanged. Generated TypeScript comes from the accepted
schema. The examples are synthetic contract fixtures, not measured model output.
Runtime wiring and quality gates are still in progress under the approved spec.

Adding optional Rust launch fields requires updates to explicit `EngramConfig`
struct literals; constructors preserve old memory/recall defaults. Custom
transports must add the code operation before advertising it as supported.
