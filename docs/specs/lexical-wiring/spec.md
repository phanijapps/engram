# Spec: lexical-wiring

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, [`lexical-keyword-retrieval`](../lexical-keyword-retrieval/spec.md) (the B1 adapter), `docs/domain-data-model.md` (contract-freeze policy)
- **Brief:** none
- **Contract:** none — composes already-accepted retrieval contracts; no public contract change
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

**Scope.** Wire the shipped lexical `RetrievalIndex` adapter (`engram-store-lexical`,
delivered by B1) into the live retrieval pipeline so `RetrievalMode::Keyword`
returns BM25-ranked knowledge chunks end-to-end, composed with graph + vector
through the existing RRF fusion.

## Objective

A retrieval caller asking "where is `parseError` defined?" (or any lexical query)
gets BM25-ranked knowledge chunks through the live pipeline — composed with graph
and vector candidates via reciprocal-rank fusion — instead of substring dumps.
The lexical index resolves chunk content, provenance, and policy from the
canonical `SqlKnowledgeStore`, never storing policy in the index.

## Boundaries

### Always do
- Compose through the existing bindings-layer RRF fusion
  (`graph_candidates_json` + `fuse_rrf_json`); add a `lexical_candidates_json`
  binding mirroring the graph one.
- Resolve chunk content/provenance/policy from `SqlKnowledgeStore` via a
  `LexicalTargetResolver` — the index holds only target refs + normalized text.
- Let source failures surface as `RetrievalSourceFailure` (the router/fusion
  path already converts per-index errors).

### Ask first
- Index-population strategy: **populate-on-query** (transient index rebuilt from
  the store per call — mirrors the lazy-embeddings pattern; fast to ship) vs a
  **persistent index fed by ingest** (the production shape). Recommend
  populate-on-query for the first slice; persistent/ingest is task L6 (deferred).

### Never do
- Change the accepted v1 retrieval contract or the `RetrievalMode` /
  `RerankStrategy` enums.
- Put Tantivy or the lexical store in `engram-domain` or `engram-retrieval` core.
- Store policy or provenance in the lexical index — always resolve from the store.

## Testing Strategy

- **TDD** — `SqlLexicalResolver` (chunk id → resolved target, skip stale/hidden),
  and the `lexical_candidates_json` binding round-trip.
- **Goal-based check** — an accepted `EvaluationFixture` asserts must-include /
  must-exclude over the wired path; full gates green including the
  contract-conformance hooks.

## Acceptance Criteria

- [ ] A `RetrievalMode::Keyword` request through the wired pipeline returns
  BM25-ranked chunk candidates composed via RRF with graph and/or vector.
- [ ] A `SqlKnowledgeStore`-backed `LexicalTargetResolver` resolves chunk
  content/provenance/policy; stale or policy-hidden chunks are skipped.
- [ ] A lexical source failure is reported as `RetrievalSourceFailure`, not a
  silent empty result.
- [ ] An accepted `EvaluationFixture` passes: relevant chunks must-include in
  top-K; unrelated chunks must-exclude.
- [ ] Full gates green: `cargo fmt --all --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm typecheck`, `pnpm build`, `.codex/hooks/check-contracts.sh`,
  `.codex/hooks/check-docs.sh`.

## Assumptions

- Technical: the lexical adapter (`LexicalIndex`, `LexicalRetrievalIndex`,
  `LexicalTargetResolver`) ships in `engram-store-lexical` via B1.
  (source: `docs/specs/lexical-keyword-retrieval/spec.md`)
- Technical: the active composition is RRF fusion at the bindings layer
  (`graph_candidates_json` + `fuse_rrf_json`); `RetrievalRouter` is an unused
  primitive. (source: `bindings/node/src/knowledge_fusion.rs`; A1 audit)
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)
