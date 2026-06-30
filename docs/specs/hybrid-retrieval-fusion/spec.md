# Spec: Hybrid Retrieval Fusion

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can merge candidate retrieval results from keyword, vector, metadata, and
other sources with deterministic scoring, duplicate collapse, and `FusionTrace`
explanations without baking any index or embedding provider into core crates.

## Boundaries

### Always do

- Implement the existing `RetrievalFusion` port in a focused retrieval crate.
- Preserve candidate policy and provenance.
- Collapse duplicate target records by `target_type` and `target_id`.
- Populate `FusionTrace` with fusion strategy, score, source rank, and
  deduplicated candidate IDs.

### Ask first

- Add learned rerankers, LLM judges, cross-encoders, or MMR.
- Call vector indexes or embedding providers from the fusion layer.
- Change public retrieval schemas.

### Never do

- Bypass policy checks or synthesize policy decisions.
- Treat missing vectors as retrieval failure.
- Hide duplicate collapse from the trace.
- Depend on a concrete store or runtime adapter.

## Testing Strategy

- TDD: fusion tests cover deterministic ordering, duplicate collapse, source
  weighting, and request limit behavior.
- Goal-based: full repository gates prove no public contract drift.

## Acceptance Criteria

- [x] A fusion implementation ranks candidates by deterministic weighted score.
- [x] Duplicate target records collapse into one result with trace evidence.
- [x] Results preserve policy, provenance, content, and explanation from the
  winning candidate.
- [x] Request limits are applied after fusion ranking.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: core already defines `RetrievalFusion` as the fusion boundary
  (source: `crates/engram-core/src/lib.rs`).
- Technical: `FusionTrace` already carries strategy, scores, source rank, and
  duplicate IDs (source: `crates/engram-domain/src/retrieval.rs`).
- Process: public contracts do not change for this fusion slice (source:
  ADR-0004).
