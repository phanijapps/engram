# Spec: lexical-keyword-retrieval

- **Status:** Shipped (adapter unit)
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0012, `docs/codegraph-parity-roadmap.md` (item B1; gated by A1 audit), `docs/domain-data-model.md` (contract-freeze policy)
- **Brief:** none
- **Contract:** none — implements the already-accepted `RetrievalMode::keyword`; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

**Scope (shipped).** This spec covers the lexical `RetrievalIndex` **adapter
crate** only (`engram-store-lexical`: Tantivy BM25 store + identifier tokenizer +
resolver + `RetrievalResult` shaping). Wiring it into the live retrieval pipeline
(composition via RRF, store-backed resolver, end-to-end eval, full workspace
gates) is in [`lexical-wiring`](../lexical-wiring/spec.md); those ACs are
deferred there.

## Objective

The `keyword` retrieval mode ranks knowledge chunks by BM25 term relevance
through a Tantivy-backed `RetrievalIndex`, so lexical queries — identifiers,
error strings, prose — return the most relevant chunks first instead of every
substring match in insertion order. The index is general-purpose: any
`KnowledgeChunk` (memory, document, or code) benefits, not code alone. It
composes with the existing vector and fusion paths through the shared
`RetrievalFusion` pipeline without changing the public contract.

## Boundaries

### Always do
- Implement behind the existing `RetrievalIndex` port
  (`core/retrieval/src/ports.rs`); register through the router/provider the same
  way the sqlite-vec vector adapter does.
- Index `KnowledgeChunk.text` and return BM25-ranked `RetrievalResult`
  candidates carrying `FusionTrace` evidence.
- Surface index failure or degradation as `RetrievalSourceFailure`, not a silent
  empty result — matching the vector adapter's contract.

### Ask first
- Field boosts beyond plain text (name/anchor/path weighting) — confirm weights
  before adding.
- Whether to replace or augment the existing in-memory/SQLite substring keyword
  path.

### Never do
- Change the accepted v1 retrieval contract or the `RetrievalMode` /
  `RerankStrategy` enums.
- Add Tantivy to `engram-domain` or `engram-retrieval` core — keep it in a
  focused adapter crate.
- Make keyword the only retrieval path, or bypass policy/scope checks.

## Testing Strategy

- **TDD** — BM25 ranking order, identifier tokenization, missing-target skip,
  and source-failure reporting, as unit tests in the new adapter crate. These are
  logic with a compressible invariant (rank order is deterministic on a fixture).
- **Goal-based check** — fmt + clippy (`-D warnings`) + test on the crate.

## Acceptance Criteria

- [x] A `keyword` query over a seeded corpus returns candidates in BM25 rank
  order (not substring/insertion order), verified by a deterministic unit test.
- [x] An identifier-aware tokenizer splits camelCase / snake_case / non-alphanumeric
  boundaries and lowercases, so `parseError`, `parse_error`, and a `parse` query
  match — verified by a deterministic unit test.
- [ ] An accepted `EvaluationFixture` passes: relevant chunks are must-include in
  top-K; unrelated chunks are must-exclude. (deferred: `lexical-wiring`)
- [x] No public v1 contract or enum changes — the adapter adds a workspace crate
  + `tantivy` dep only; schema conformance unaffected.
- [ ] The lexical index is an injected `RetrievalIndex` composed through the
  existing router/fusion end-to-end. (deferred: `lexical-wiring`)
- [x] Index failure or degradation propagates (via `?`), never swallowed as a
  silent empty `Ok` — the `RetrievalRouter` precedent converts these to
  `RetrievalSourceFailure`.
- [x] Per-crate gates green: `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings`, `cargo test` on `engram-store-lexical`.
- [ ] Full workspace gates + contract hooks green. (deferred: `lexical-wiring`)

## Assumptions

- Technical: `RetrievalMode::keyword` is contracted but implemented only as
  substring match today; the `RetrievalIndex` port (`core/retrieval/src/ports.rs:22`)
  is the plug point; no Tantivy was present. (source: `docs/research/codegraph-parity-audit.md`)
- Technical: the sqlite-vec vector adapter (per the shipped
  `backend-agnostic-retrieval` spec) is the precedent for an injected
  `RetrievalIndex` + resolver + `RetrievalResult` shaping. (source: audit + repo)
- Process: light mode, single adversarial pass. (source: user confirmation 2026-07-08)

## Notes

- 2026-07-08: T1–T3 shipped (`engram-store-lexical`: `LexicalIndex`,
  `LexicalRetrievalIndex`, `LexicalTargetResolver`, `normalize_identifier_text`;
  10 tests green; fmt + clippy clean). T4a/T4b/T5 split to `lexical-wiring`
  after the composition layer was found to be bindings-layer RRF fusion, not the
  unused `RetrievalRouter`.
