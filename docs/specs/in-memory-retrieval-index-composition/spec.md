# Spec: In-Memory Retrieval Index Composition

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

The in-memory memory service can compose injected `RetrievalIndex` candidate
sources into its existing retrieval pipeline, so sqlite-vec or other external
indexes can participate in shared fusion and budget handling without adding a
vector, embedding, SQL, or provider dependency to `engram-store-memory`.

## Boundaries

### Always do

- Keep external candidate sources behind the existing `RetrievalIndex` port.
- Continue to generate in-memory memory, knowledge, belief, and hierarchy
  candidates before shared fusion.
- Surface non-fatal external index errors through `sourceFailures`.
- Apply the existing fusion, request limit, and budget omission behavior to
  external candidates.

### Ask first

- Add a production embedding provider or model download path to the default
  service.
- Make `engram-store-memory` depend on `engram-store-vector`.
- Change public v1 retrieval schemas or domain data model fields.

### Never do

- Bake sqlite-vec, FastEmbed, SQL, or provider-specific behavior into the
  in-memory adapter.
- Let one failed external retrieval index fail the whole context request when
  other sources can still answer.
- Return external candidates outside the shared fusion and truncation path.
- Turn the crate root or service module into a catch-all retrieval orchestrator.

## Testing Strategy

- TDD: adapter tests inject fake `RetrievalIndex` sources and prove candidates
  participate in fusion, request-limit truncation, and source-failure reporting.
- Regression: existing retrieval, knowledge, belief, hierarchy, and vector tests
  continue to pass.
- Goal-based: the opt-in sqlite-vec plus FastEmbed BGE-small test check remains
  runnable through `cargo check -p engram-store-vector --features fastembed-tests --tests`.

## Acceptance Criteria

- [x] `InMemoryMemoryService` accepts injected `RetrievalIndex` sources without
  changing existing constructors' default behavior.
- [x] External retrieval candidates are fused with in-memory memory, knowledge,
  belief, and hierarchy candidates.
- [x] External candidates dropped by `limit` or `budget.maxItems` are reported
  as budget omissions.
- [x] External index errors become degraded `RetrievalSourceFailure` records
  while successful local candidates still return.
- [x] `engram-store-memory` does not depend on `engram-store-vector`,
  sqlite-vec, FastEmbed, SQL, or provider crates.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: `RetrievalIndex` is the existing core candidate-source boundary
  (source: `crates/engram-core/src/lib.rs`).
- Technical: sqlite-vec candidate generation already exists behind
  `VectorRetrievalIndex` (source:
  `crates/engram-store-vector/src/retrieval.rs`).
- Technical: `ContextPayload.sourceFailures` is the accepted degraded-source
  reporting surface (source: `crates/engram-domain/src/retrieval.rs`).
- Process: concrete adapters stay outside `engram-core` and are composed behind
  ports (source: `AGENTS.md`).
