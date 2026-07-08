# Plan: lexical-keyword-retrieval

- **Spec:** [`spec.md`](spec.md)
- **Status:** T1–T3 shipped; T4a/T4b/T5 split to `lexical-wiring`

> **Plan contract:** implementation strategy; may change as we learn. Substantial
> approach changes are recorded in the Changelog below.

**Light-mode lean fill.** Approach + short Tasks list; Design (LLD) kept thin.

## Approach

Add a focused adapter crate `adapters/retrieval/tantivy-lexical`
(`engram-store-lexical`) that implements the existing `RetrievalIndex` port. It
owns a Tantivy full-text index over `KnowledgeChunk.text` (plus a stored target
reference for rehydration), exposes upsert/delete for the ingest path, and
returns BM25-ranked `RetrievalResult` candidates for `RetrievalMode::keyword`.
Register it through the existing provider/router exactly like the sqlite-vec
vector adapter, so its candidates flow through `RetrievalFusion` (RRF/weighted)
alongside vector and keyword-memory candidates.

Order: port impl skeleton → index + upsert/delete → candidate + failure shaping
→ provider registration (seeded) → ingest-sync + regression → eval fixture +
gates. The riskiest part — keeping the index in sync with ingest (upsert/delete
on re-index) — is isolated behind its own task (T4b) so it has its own
verification artifact, not bundled into registration.

## Constraints

- RFC-0012 + `docs/codegraph-parity-roadmap.md` (B1); gated by A1 audit.
- Contract-freeze policy (`docs/domain-data-model.md`): no enum or contract change.
- `AGENTS.md`: Tantivy stays in a focused adapter crate — never in
  `engram-domain` or `engram-retrieval` core.

## Construction tests (cross-cutting)

- **Integration:** ingest a small corpus → a service-level `keyword` query
  returns BM25-ranked candidates composed through fusion.
- **Regression:** existing accepted retrieval fixtures (positive/forbidden
  recall, budget, no-result) stay green.

## Design (LLD)

- **Data & schema** — Tantivy schema: a tokenized `text` field (BM25) over
  `KnowledgeChunk.text` using an identifier-aware tokenizer (split camelCase /
  snake_case / non-alphanumeric, then lowercase), plus stored fields carrying the
  target type/id and provenance for `RetrievalResult` rehydration. Index is
  adapter-local (not portable contract).
- **Interfaces & contracts** — implements `engram_retrieval::RetrievalIndex`;
  consumes chunk upsert/delete from the ingest path. No new public contract.

## Tasks

### T1: Adapter crate skeleton + `RetrievalIndex` impl stub
**Depends on:** none
**Tests:**
- New crate compiles; the adapter type-checks against `RetrievalIndex`
  (`core/retrieval/src/ports.rs:22`).
**Approach:**
- Create `adapters/retrieval/tantivy-lexical/` with `Cargo.toml` (workspace
  member) + a `lib.rs` facade; add a stub `LexicalRetrievalIndex` implementing
  `RetrievalIndex` with empty/err returns for now.
**Done when:** `cargo check -p <new-crate>` is green and the impl is wired to the
port.

### T2: Tantivy index over `KnowledgeChunk.text` + identifier tokenizer + upsert/delete
**Depends on:** T1
**Tests:**
- Index N chunks; a query returns BM25-ranked hits in the expected order on a
  fixture corpus; `parseError` / `parse_error` / `parse` query match via the
  identifier-aware tokenizer; delete removes a chunk from results.
**Approach:**
- Add `tantivy` dep; define the schema with an identifier-aware tokenizer (split
  camelCase/PascalCase boundaries, underscores, and non-alphanumerics, then
  lowercase) for the `text` field; implement open/upsert/delete/search returning
  ranked `(target_ref, score)` pairs.
**Done when:** unit test asserts deterministic BM25 rank order, identifier
splitting, and deletion.

### T3: Shape `RetrievalResult` candidates + `RetrievalSourceFailure`
**Depends on:** T2
**Tests:**
- Candidates carry policy/provenance/`FusionTrace`; a forced index error yields
  a `RetrievalSourceFailure` (degraded), never a silent empty success.
**Approach:**
- Map ranked hits to `RetrievalResult` (`targetType=chunk`); populate
  `FusionTrace`; translate Tantivy errors to `RetrievalSourceFailure`.
**Done when:** unit tests for candidate shape and failure reporting are green.

### T4a: Register the lexical index via the provider/router (seeded)
**Depends on:** T3
**Tests:**
- Integration: with a directly-seeded lexical index (no ingest wiring), a
  service-level `keyword` query returns BM25-ranked candidates composed through
  fusion.
**Approach:**
- Inject the lexical index through the integration provider builder the same way
  sqlite-vec is registered; add the integration test on a directly-seeded index.
**Done when:** the seeded-index integration test is green.

### T4b: Feed the lexical index from ingest (upsert/delete) + regression
**Depends on:** T4a
**Tests:**
- Ingest writes upsert chunk text into the lexical index; re-ingest/deletion
  converges the index (updated/removed chunks reflect in results); the full
  accepted retrieval regression suite stays green.
**Approach:**
- Wire the ingest chunk write/delete path to the lexical index upsert/delete
  (reuse the existing content-hash reconcile path); run the regression suite.
**Done when:** ingest-sync test + full retrieval regression suite green. (This
isolates the plan's highest risk — index/ingest sync — behind its own gate.)

### T5: Accepted `EvaluationFixture` + gates + docs
**Depends on:** T4b
**Tests:**
- `EvaluationFixture` (must-include / must-exclude) passes in the runner; gates
  green; roadmap/README updated.
**Approach:**
- Author the fixture under the accepted eval set; run the full validation sweep.
**Done when:** `cargo fmt --all --check`, `cargo check --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`pnpm typecheck`, `pnpm build`, `.codex/hooks/check-contracts.sh`,
`.codex/hooks/check-docs.sh` all green, and B1 is marked done in
`docs/codegraph-parity-roadmap.md`.

## Risks

- Index/ingest sync drift on re-index or deletion — isolated in T4b; reuses the
  existing chunk content-hash reconcile path; T2 tests deletion explicitly.
- Tokenizer choice is pinned in T2 (identifier-aware: camelCase/snake_case split
  + lowercase) so identifier recall holds; the regression suite guards against
  recall collapse vs the substring path.

## Changelog

- 2026-07-08: initial plan (light mode), gated by A1 audit.
- 2026-07-08: adversarial-review fixes — added contract-conformance hooks to AC5
  and T5 gates; split T4 into T4a (register, seeded) + T4b (ingest-sync +
  regression) to isolate the highest risk; pinned the identifier-aware tokenizer
  in T2; dropped the stale `provider.rs:294` line citation; added the
  contract-freeze policy to `Constrained by:`.
- 2026-07-08: **T1–T3 shipped** — `engram-store-lexical` (LexicalIndex +
  LexicalRetrievalIndex + LexicalTargetResolver + normalize_identifier_text),
  10 tests green, fmt + clippy clean. T4a/T4b/T5 moved to a new
  `lexical-wiring` spec after the composition layer was found to be
  bindings-layer RRF fusion (`graph_candidates_json` + `fuse_rrf_json`), not the
  unused `RetrievalRouter`; the wiring touches 4 crates and warrants its own spec.
