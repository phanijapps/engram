# v1 Changelog

## 2026-10-30 (durable canonical spans)

- Added the `CanonicalSpan` definition (1-based inclusive `startLine`/`endLine`,
  both ≥ 1) and the optional, nullable `CodeLineage.span` field pinning a
  lineage record's canonical lines at its generation's pinned commit (r5
  durable canonical-span authentication; staged once at publication, never
  re-read from mutable rows). Additive/compatible: existing lineage data
  validates unchanged; consumers that do not read `span` are unaffected.

## 2026-10-24 (outcome loop)

- Added `TaskOutcome` + `RecallOutcome` definitions with an example (F2:
  attributed outcome ledger; association data, never causal proof).
  Additive/compatible.

## 2026-10-22 (change log)

- Added `KnowledgeChangeOp`, `KnowledgeChangeTargetType`, `KnowledgeChange`,
  `KnowledgeChangesPage`, `ScanStatus`, `ScanCheckpoint`, `StalenessStatus`,
  and `CodeGraphStaleness` definitions with a page example (knowledge change
  log + scan freshness; seq is a decimal string). Additive/compatible.

## 2026-10-22 (proposals)

- Added `OperatorProposalStatus`, `ProposalMemoryWrite`, and `OperatorProposal`
  definitions with pending + approved examples (operator proposal queue;
  approval routes through the policy-checked memory write path).
  Additive/compatible: new types + operations; no existing shapes change.

## 2026-10-22

- Added `ResolutionTier` + `CodeEdgeResolution` definitions and the optional
  relationship `codeResolution` field (code-edge resolution evidence; ADR-0028
  amendment 2026-10-22). Additive/compatible. Examples:
  `code-edge-resolution.heuristic.json`, `code-edge-resolution.parsed.json`.

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
