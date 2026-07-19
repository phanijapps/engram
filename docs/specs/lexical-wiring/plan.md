# Plan: lexical-wiring

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy; may change as we learn.

**Light-mode lean fill.** Approach + short Tasks list.

## Approach

Wire the shipped lexical adapter (`engram-store-lexical`) into the live retrieval
pipeline at the bindings-layer RRF composition seam, mirroring the graph path:

1. A `SqlKnowledgeStore`-backed `LexicalTargetResolver` resolves a chunk id (from
   a BM25 hit) into content/provenance/policy via the existing `KnowledgeRepository`
   chunk lookup, skipping stale or policy-hidden chunks.
2. A `lexical_candidates_json` N-API binding (mirror `graph_candidates_json`)
   builds a `LexicalIndex`, **populates it from the store's chunks for the
   request scope** (populate-on-query first slice), runs the query, and returns
   `LexicalRetrievalIndex` candidates.
3. The TS demo adds the lexical candidate list to the existing `fuse_rrf_json`
   call (graph + vector + lexical).
4. An accepted `EvaluationFixture` exercises the wired path; full gates run.

The persistent-index + ingest-feed production shape is task L6 (deferred past
this slice — see Open question).

## Constraints

- RFC-0012 + B1 adapter; contract-freeze policy (`docs/domain-data-model.md`).
- `AGENTS.md`: Tantivy/lexical store stay in the adapter crate; policy resolved
  from the store, never stored in the index.

## Construction tests (cross-cutting)

- **Integration:** ingest a small corpus → a `Keyword` request returns
  BM25-ranked chunks composed via RRF.
- **Regression:** existing retrieval fixtures stay green.

## Tasks

### L1: `SqlLexicalResolver` (store-backed `LexicalTargetResolver`)
**Depends on:** none (B1 adapter shipped)
**Tests:**
- Resolves a known chunk id → content/provenance/policy; returns `None` for an
  unknown or policy-hidden id.
**Approach:**
- Implement `LexicalTargetResolver` over `Arc<SqlKnowledgeStore>` using the
  `KnowledgeRepository::get_chunk(id, scope)` lookup; lives in the knowledge
  SQLite adapter or a thin wiring module (not in core).
**Done when:** resolver unit tests green.

### L2: `lexical_candidates_json` binding (populate-on-query)
**Depends on:** L1
**Tests:**
- Binding round-trip: given a request, returns BM25-ranked `RetrievalResult`
  JSON for chunks in scope.
**Approach:**
- In `bindings/node`, add `lexical_candidates_json` mirroring
  `graph_candidates_json`: build a `LexicalIndex`, iterate in-scope store
  chunks calling `upsert`, construct `LexicalRetrievalIndex` with the L1
  resolver, return `retrieve_candidates`.
**Done when:** binding test green.

### L3: TS-side RRF composition adds lexical
**Depends on:** L2
**Tests:**
- The demo's retrieval path passes graph + vector + lexical candidate lists to
  `fuseRrfJson`; a keyword query surfaces lexical-ranked chunks.
**Approach:**
- Update the demo/TS retrieval wiring to call `lexicalCandidatesJson` and feed
  its list into the existing RRF fusion.
**Done when:** demo keyword query returns fused results including lexical hits.

### L4: Accepted `EvaluationFixture` (must-include / must-exclude)
**Depends on:** L3
**Tests:**
- Fixture passes in the runner: relevant chunks must-include in top-K;
  unrelated must-exclude.
**Approach:**
- Author the fixture under the accepted eval set; run the runner.
**Done when:** fixture green.

### L5: Full gates
**Depends on:** L4
**Tests:**
- `cargo fmt --all --check`, `cargo check --workspace`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm typecheck`,
  `pnpm build`, `.codex/hooks/check-contracts.sh`, `.codex/hooks/check-docs.sh`.
**Done when:** all gates green; B1 + lexical-wiring marked done in the roadmap.

## Open question

- **L6 (deferred): persistent index + ingest feed.** Populate-on-query (L2)
  rebuilds the index per request — fine for small/demo corpora, not for
  repo-scale. The production shape is a file-backed `LexicalIndex` fed by the
  ingest chunk-write path (the T4b originally in B1). Decide after L4 measures
  latency on a realistic corpus. Recommend: ship L1–L5, then a separate
  `lexical-persistent-index` spec for L6.

## Risks

- Populate-on-query cost on large corpora — bounded by measuring at L4;
  persistent index (L6) is the mitigation.
- Scope/policy correctness in the resolver — TDD'd in L1 (skip stale/hidden).

## Changelog

- 2026-07-08: initial plan, split out of B1 (T4a/T4b/T5) after the composition
  layer turned out to be bindings-layer RRF fusion, not the unused
  `RetrievalRouter`.
