# Spec: Vector Retrieval Candidates

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

The vector store can expose sqlite-vec nearest-neighbor rows as
`RetrievalResult` candidates through the existing `RetrievalIndex` port, using
injected query-vector and target-resolver collaborators so embeddings, policy
rehydration, and canonical records remain outside the vector table.

## Boundaries

### Always do

- Keep sqlite-vec rows as secondary adapter state, not domain truth.
- Require a query-vector provider to translate retrieval requests into vectors.
- Require a target resolver to rehydrate policy, provenance, content, and target
  type before returning candidates.
- Preserve vector distance and similarity evidence in `FusionTrace` and scores.

### Ask first

- Add a production embedding provider, model download, or FastEmbed runtime path
  outside opt-in tests.
- Add SQL/vector fields to public v1 schemas.
- Make `engram-core` depend on sqlite-vec or a concrete vector adapter.

### Never do

- Return vector hits without rehydrated policy and provenance.
- Treat missing vector targets as policy-approved empty content.
- Bypass downstream fusion or context composition.
- Couple query embedding generation to canonical retrieval behavior.

## Testing Strategy

- TDD: vector retrieval tests cover nearest-neighbor order, missing-target skips,
  trace/source score population, and query-vector dimension errors.
- Regression: existing sqlite-vec tests keep raw vector storage behavior covered.
- Goal-based: full repository gates and opt-in FastEmbed feature check continue
  to pass.

## Acceptance Criteria

- [x] `engram-store-vector` provides a `RetrievalIndex` implementation over
  `SqliteVectorIndex`.
- [x] Query vector generation is injected behind a trait.
- [x] Target rehydration is injected behind a trait and required for returned
  candidates.
- [x] Vector distance is converted into deterministic retrieval scores and
  trace evidence.
- [x] Missing target rows are skipped without failing the whole request.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: sqlite-vec search already returns adapter-local rows ordered by
  distance (source: `crates/engram-store-vector/src/index.rs`).
- Technical: `RetrievalIndex` is the core candidate-source boundary (source:
  `crates/engram-core/src/lib.rs`).
- Process: vector storage is secondary adapter state and must not become domain
  truth (source: `docs/implementation-roadmap.md`).
