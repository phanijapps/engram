# Plan: knowledge-source-retraction

## Design (LLD)

Mirror the graph's source-keyed lookup onto documents, then cascade deletes
children-first. Ports are additive + default-no-op (existing adapters compile
unchanged); only the SQLite adapter gains impls this slice.

### Stack
Rust, rusqlite (existing), sqlite-vec (existing). No new dependency. Ports in
`engram-knowledge`; impls in `engram_store_sqlite`; cascade wiring in
`engram-ingest/src/reconcile.rs`.

## Tempted to add, declining
- A generic dependency-propagation engine (manual edit → cascade) — out of scope;
  this spec is source-reingest retraction only.
- Retracting episode/provenance-audit links — episodes are audit history; keep.
- A pgvector impl of the new ports — additive later (the whole point of doing
  this before pgvector).

## Tasks

### T1 — documents get a source key + path index
Depends on: none
Spec mapping: AC#2 (find prior doc), enables AC#4
Verification: TDD (schema migration is goal-based; the lift + lookup is testable)
Approach:
1. `documents` schema: `ALTER TABLE documents ADD COLUMN stable_source_key TEXT` + `path` + `CREATE INDEX idx_documents_source_path ON documents(stable_source_key, path)` (mirror `knowledge_graphs`). Migration in `adapters/sqlite/src/knowledge/schema.rs`.
2. `put_document` lifts `stable_source_key` + `path` into the columns (the scanner's `SourceDocument` carries path; stable key comes from the ingest source).
3. `list_documents_by_source(scope, stable_source_key) -> Vec<SourceDocument>` port (default no-op) + SqlKnowledgeStore impl.

### T2 — document/chunk retraction ports
Depends on: T1
Spec mapping: AC#1, AC#4
Verification: TDD
Approach:
1. Additive `KnowledgeRepository`: `list_chunks_by_document(scope, doc_id)`, `delete_document(id, scope) -> bool`, `delete_chunk(id, scope) -> bool` (default no-ops).
2. SqlKnowledgeStore impls: chunk delete by id; document delete by id (chunks are separate rows — delete them via list_chunks_by_document first in the cascade, not via SQL cascade, to keep it explicit + scoped).

### T3 — vector delete-by-target-id
Depends on: none
Spec mapping: AC#3
Verification: TDD (insert → delete → search returns none)
Approach: add `delete_by_target_id(target_id) -> CoreResult<()>` to the vector index port + sqlite-vec impl (`DELETE FROM vec_items WHERE target_id = ?`).

### T4 — reconcile cascade (children-first)
Depends on: T1, T2, T3
Spec mapping: AC#4
Verification: integration test (the real proof)
Approach: extend the retraction path in `engram-ingest/src/reconcile.rs`. For each prior graph found by `(stable_source_key, path)`: find prior documents for that `(key, path)` → for each, list its chunks → delete each chunk's embedding by `target_id` → delete the chunks → delete the document. Children before parents. Best-effort per item; failures count into the scan summary (mirror graph-retraction soft-fail). A pure `retraction_plan(doc, chunks) -> Vec<DeletionStep>` helper is the TDD-able unit.

### T5 — gates + PR
Depends on: T4
Verification: the gate suite + the new integration test.
Approach: fmt, check (0 warnings), test --workspace, neutrality, parity, docs. Branch `feat/knowledge-source-retraction`, open PR (no merge).

## Rollout

Single PR, branch `feat/knowledge-source-retraction`, off current main. Commits
aligned to T1–T4 + the integration test. Left open for review — not auto-merged.
After merge, pgvector implements the same additive ports (no rework).
