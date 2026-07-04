# Plan: knowledge-graph-retraction

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn.

## Approach

Add three fine-grained delete ports to the knowledge traits (default impl +
`SqlKnowledgeStore` override), make `delete_graph` cascade to its entities and
relationships in one transaction, then build a small reconcile step in
`adapters/ingest` that composes those ports — no coarse method on the knowledge
trait. Reconciliation keys on the `(stable_source_key, path)` Phase 1 stamps on
each graph: before writing a re-ingested file's new graph, delete the prior
graph(s) for that pair; after a scan, delete graphs for manifest paths the scan
no longer contains; and drop the per-source `Repository` node once its last
document graph is gone. The riskiest parts are the cascade transaction (must be
atomic and scope-scoped) and the Repository-node ref-count (must not delete it
while sibling graphs remain). Tests: TDD at the adapter for the delete/cascade
invariants, integration over scan→store for the reconcile behaviors.

## Constraints

- **RFC-0009 / ADR-0018** — retraction model, SHA-free `(stable_source_key, path)`
  diff basis, hard delete, ref-count shared nodes.
- **ADR-0017 / structured-repo-identity** — supplies the stable key + graph
  `path` column + `Repository` node this reconciler operates on.
- **RFC-0009 god-trait concern** — the coarse reconcile logic lives in
  `adapters/ingest`, not as a new `KnowledgeRepository` method.

## Construction tests

**Integration tests:** (T3) re-ingest a changed `(key, path)` → one graph, no
orphans; (T4) removed path → graph deleted; (T5) Repository node survives then
dies with its last graph — all over the scan→store path.
**Cross-cutting checks:** (T6) `pnpm run contracts:generate` no diff.
**Manual verification:** none.

## Design (LLD)

Shape: `service`. Stack: `core/knowledge` (trait method signatures + defaults),
`adapters/knowledge/sqlite` (delete impls + cascade transaction), `adapters/ingest`
(reconcile step in the scan flow). No new domain type, no schema column.

### Interfaces & contracts
- `KnowledgeRepository::delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool>`
  and `delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool>`;
  `KnowledgeGraphRepository::delete_graph(&self, id: &KnowledgeGraphId, scope: &Scope) -> CoreResult<bool>`.
  Default impls return a `CoreError::Adapter` not-supported error (mirroring `put_entity`, lib.rs:37); `SqlKnowledgeStore` overrides.
  Traces to: AC-1 · none.

### Data & schema
- No new columns. Deletes are `DELETE FROM … WHERE id = ?1` guarded by
  `scope_allows` (read-modify in a transaction for the cascade). Traces to: AC-2, AC-3 · none.

### Failure, edge cases & resilience
- `delete_graph` runs `DELETE FROM knowledge_entities WHERE graph_id=?`,
  `DELETE FROM knowledge_relationships WHERE graph_id=?`, `DELETE FROM knowledge_graphs WHERE id=?`
  inside one `rusqlite` transaction; scope-checked before delete. A delete under a
  non-matching scope is a no-op returning `false`. Traces to: AC-2, AC-3 · none.
- Reconcile is idempotent: deleting an already-absent graph returns `false`, no error. Traces to: AC-4, AC-5 · none.

### Quality attributes (NFRs)
- Hard delete (AC-7); no tombstone column. No generated-contract change (AC-8).

## Tasks

### T1: Fine-grained entity/relationship delete ports

**Depends on:** none · **Verifies:** AC-1, AC-3, AC-7

**Tests:**
- TDD (adapter): `delete_entity`/`delete_relationship` remove the row (row count
  goes to zero — hard delete, AC-7) and return `true`; a second delete returns
  `false`; a delete under a mismatched scope returns `false` and leaves the row
  (AC-1, AC-3).

**Approach:**
- Add the two methods to `KnowledgeRepository` (core/knowledge/src/lib.rs) with a
  default not-supported `CoreError::Adapter` impl (mirroring `put_entity`,
  lib.rs:37-42); implement in `adapters/knowledge/sqlite/src/service.rs` with a
  `scope_allows` guard and `DELETE FROM …` (hard delete, no tombstone column).

**Done when:** the delete/scope-check unit tests are green.

### T2: `delete_graph` with cascade

**Depends on:** none · **Verifies:** AC-1, AC-2, AC-3, AC-7

**Tests:**
- TDD (adapter): deleting a graph removes the graph row + all entities and
  relationships with that `graph_id` (row counts to zero — hard delete, AC-7),
  and leaves other graphs' records intact; scope-checked (a mismatched-scope
  delete is a no-op returning `false`, AC-3); returns whether the graph existed
  (AC-1, AC-2).

**Approach:**
- Add `delete_graph` to `KnowledgeGraphRepository` (default not-supported `CoreError::Adapter`);
  implement in `service.rs` as a single transaction deleting entities,
  relationships, then the graph, guarded by `scope_allows`.

**Done when:** the cascade + scope unit tests are green.

### T3: Per-(key,path) reconcile on re-ingest

**Depends on:** T2 · **Verifies:** AC-4

**Tests:**
- Integration: ingest a file, then re-ingest the same `(stable_source_key, path)`
  with changed content → exactly one graph for that pair, prior entities gone (AC-4).

**Approach:**
- In `adapters/ingest` (the ingest/scan flow), before writing a document's new
  graph, look up prior graph(s) for its `(stable_source_key, path)` via
  `list_graphs_by_source(scope, key)` filtered by the graph `path` metadata, and
  `delete_graph` each. Add a `reconcile` module owning this composition.

**Done when:** the replace-on-change integration test is green.

### T4: Scan-level removed-path convergence

**Depends on:** T2, T3 · **Verifies:** AC-5

**Tests:**
- Integration: scan a repo (paths A,B), then re-scan with A removed → A's graph +
  entities deleted; B untouched (AC-5).

**Approach:**
- The scanner currently builds `new_manifest = opts.manifest.clone()` and only
  *inserts* walked files (scanner.rs:502-513), so `prior − new` is always empty —
  removed files are undetectable. Fix: collect the set of paths actually **walked**
  this scan, compute `removed = prior_manifest.keys() − walked_paths`, `delete_graph`
  the graph for each removed `(key, path)` (via the T3 lookup), and rebuild the
  **emitted** manifest from walked paths only (so removed entries are pruned).

**Done when:** the removed-path integration test is green (a file dropped between
scans has its graph deleted and no longer appears in the emitted manifest).

### T5: Repository-node lifecycle

**Depends on:** T2, T4 · **Verifies:** AC-6

**Tests:**
- Integration: after all document graphs for a `stable_source_key` are removed,
  the `EntityKind::Repository` node is deleted; while ≥1 remains, it persists (AC-6).

**Approach:**
- After a graph deletion, if no document graphs remain for the `stable_source_key`
  (via `list_graphs_by_source`), `delete_entity` the Repository node (id from the
  full scope + key, as minted in Phase 1).

**Done when:** the Repository-node lifecycle integration test is green.

### T6: No-contract-change check

**Depends on:** T1, T2 · **Verifies:** AC-8

**Tests:**
- Goal-based: `pnpm run contracts:generate` yields no diff; `git status` clean
  under `contracts/`, `docs/domain-data-model.md`, `packages/contracts/`.

**Approach:**
- Confirm only trait method signatures + adapter/ingest changed; run the generator.

**Done when:** the generator produces no diff.

## Rollout

Additive to the knowledge port + adapter/ingest behavior. `delete_*` default
impls keep any future non-SQLite implementor compiling. Reversible: the reconcile
step can be disabled without schema change. No infra, no flag.

## Risks

- **Cascade atomicity** — entity/relationship/graph deletes must be one
  transaction, or a crash mid-delete orphans records; mitigate with an explicit
  `rusqlite` transaction.
- **Path lookup cost** — filtering `list_graphs_by_source` by path in Rust is
  O(graphs-per-repo) per file; acceptable at demo scale, revisit with a
  `(key, path)` index query if it bites.
- **Repository-node premature deletion** — the ref-count check must run after the
  graph delete and see the remaining graphs; mitigate by counting via
  `list_graphs_by_source` post-delete.
- **Compound reconcile not atomic across ports** — delete_graph → recount →
  delete Repository node spans multiple lock acquisitions; safe only because
  `SqlKnowledgeStore` serializes on a single connection mutex. Concurrent
  multi-writer re-ingest is out of scope (demo scale); documented in the spec.
- **Deferred cascade** — `SourceDocument`/`KnowledgeChunk`/embedding retraction is
  NOT in this spec (deferred: knowledge-source-retraction); a changed/removed
  file's prior document, chunks, and vectors linger until that follow-up. The
  graph converges; retrieval over stale chunks is the known gap.

## Changelog

- 2026-07-04: initial plan.
