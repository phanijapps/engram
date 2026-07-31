# Spec: knowledge-source-retraction

Status: Draft
Mode: light (medium slice; contract-additive — new retraction ports; phased)
Shape: data
Constrained by: RFC-0009 (knowledge-graph retraction and convergence — this completes the cascade the RFC flagged as OQ2), ADR-0022 (engine neutrality — ports are neutral; the impl is one SQLite adapter cell now, pgvector later)
- **Contract:** additive — new optional `KnowledgeRepository` methods (`delete_document`, `delete_chunk`, `list_documents_by_source`, `list_chunks_by_document`) + a vector-index `delete_by_target_id`. No existing field changes.

## Objective

When a source file is re-ingested (changed) or removed, engram already
**converges the knowledge graph** (RFC-0009: entities/relationships/graphs +
Repository node are deleted + rebuilt). It does **not** retract the underlying
`SourceDocument`s, `KnowledgeChunk`s, or their sqlite-vec embeddings — those are
keyed by content-derived `document_id`/`chunk_id` (and embeddings by `target_id`),
not by `graph_id`, so a changed file's prior docs/chunks/vectors **linger**
forever. This spec closes that gap: retraction cascades from the prior graph to
its document → chunks → embeddings, so a re-ingest fully repairs the source's
stored state.

This is the single highest-leverage debt item relative to the RFC-0017 future
state: it is what makes "repair on change" real and is a prerequisite for the
maintenance module (decay/GC). It is intentionally done **before pgvector** so
the retraction ports are defined once on SQLite; pgvector then implements the
same ports additively (ADR-0022), instead of retraction being built twice.

## Assumptions (verified)

- `KnowledgeRepository` has `put_document`/`put_chunk`/`get_chunk` and `delete_graph`/`delete_entity`/`delete_relationship`, but **no** `delete_document`/`delete_chunk`/`list_documents_by_source` (`core/knowledge/src/repository.rs`). ← the port gap.
- Document/chunk ids are content-derived (`document_id(source, metadata, content_hash)`, `chunk_id(document_id, index, chunk_hash)` in `adapters/ingest/src/ingestor.rs`) → a changed file's prior doc/chunks have **different ids** and can't be found by id.
- `knowledge_graphs` already has `stable_source_key` + `path` columns + `list_graphs_by_source` (used by the graph retraction). **`documents` does not** — it has no source-key/path lookup. ← the find-prior-doc gap.
- The sqlite-vec index keys embeddings by `target_id` (`adapters/sqlite/src/vector/`) and has `insert`/`search`/get-by-id but **no `delete`**. ← the embedding-retraction gap.
- `reconcile::delete_prior_graphs_for_path` already finds prior graphs by `(stable_source_key, path)` and deletes them; this is the seam to extend.

## Proposal (design)

Mirror the graph's source-keyed lookup onto documents, then cascade.

1. **Documents get a source key + path index** (schema, mirroring `knowledge_graphs`): add `stable_source_key` + `path` columns + an index to the documents table; lift them on `put_document` (the scanner already has both in the `SourceDocument`/ingest request). This lets reconcile find the prior document for a `(stable_source_key, path)` pair without knowing its content-derived id.
2. **New retraction ports** (additive `KnowledgeRepository` methods, default-implemented as no-ops so existing adapters keep compiling):
   - `list_documents_by_source(scope, stable_source_key) -> Vec<SourceDocument>`
   - `list_chunks_by_document(scope, document_id) -> Vec<KnowledgeChunk>`
   - `delete_document(id, scope) -> bool`
   - `delete_chunk(id, scope) -> bool`
3. **Vector delete-by-target-id**: add `delete_by_target_id(target_id)` to the vector index port + the sqlite-vec impl (chunk/document embeddings carry their id as `target_id`).
4. **Cascade in reconcile**: extend the retraction path so that for each prior graph found by `(stable_source_key, path)`, before/after deleting the graph, it also (a) finds the prior document(s) for that `(key, path)`, (b) lists their chunks, (c) deletes each chunk's embedding by `target_id`, (d) deletes the chunks + the document. Order: delete embeddings → chunks → document → graph (children before parents).

## Boundaries

### Always do
- New ports are **additive + default no-op** (existing adapters + the in-memory fixture compile unchanged).
- Engine-neutral ports; SQLite impl only this slice. pgvector implements the same ports later.
- Retraction is **scoped** (never crosses scope) and **best-effort per item** (a failed embedding delete doesn't abort the doc/chunk delete — degrade + count into the scan summary, mirroring the graph-retraction soft-fail).
- Document → chunk → embedding cascade deletes children before parents.

### Ask first
- Whether to also retract `SourceDocument` provenance/episode links (episodes_evidence) — likely out of scope v1 (episodes are audit history; keep them).

### Never do
- Delete across scopes.
- Auto-cascade on arbitrary manual `put_relationship` edits (that's a separate dependency-propagation capability, explicitly out of scope — this spec is source-reingest retraction only).
- Reintroduce a process-local fixture.
- Put SQL in `engram-knowledge` core (ports only; SQL stays in `engram_store_sqlite`).

## Testing Strategy

- **TDD** the pure bits where possible: the cascade ordering helper (given a prior doc + its chunks, produce the delete plan in children-first order).
- **Integration test** (the real proof): ingest a file → assert doc + chunks + embeddings exist → change the file content → re-ingest → assert the OLD document, OLD chunks, and OLD embeddings are gone (and the new ones present); graph already converges (existing RFC-0009 tests). Use the in-memory/SQLite store + the scanner.
- Existing `knowledge-graph-retraction` tests stay green (graph behavior unchanged).

## Acceptance Criteria

- [ ] `KnowledgeRepository` has additive `list_documents_by_source`, `list_chunks_by_document`, `delete_document`, `delete_chunk` (default no-ops); existing adapters compile unchanged.
- [ ] Documents carry `stable_source_key` + `path` (schema migration + lift on `put_document`); reconcile can find a prior document by `(stable_source_key, path)`.
- [ ] Vector index has `delete_by_target_id`; sqlite-vec impl deletes the row.
- [ ] Re-ingesting a changed file retracts its prior document + chunks + embeddings (integration test proves it); a removed file's do as well.
- [ ] `cargo fmt --all`, `cargo check --workspace` (0 warnings), `cargo test --workspace`, engine-neutrality, surface-parity, docs all green.
