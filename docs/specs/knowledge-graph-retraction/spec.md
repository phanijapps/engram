# Spec: knowledge-graph-retraction

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0009, ADR-0018, ADR-0017, docs/specs/structured-repo-identity
- **Brief:** none
- **Contract:** none (adds Rust port methods + adapter/ingest behavior; no engram interface surface, no generated-contract change)
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Continuous re-ingestion keeps the knowledge graph **current** instead of only
accreting. The knowledge layer gains deletion: `delete_entity` and
`delete_relationship` on `KnowledgeRepository`, and `delete_graph` on
`KnowledgeGraphRepository` — `delete_graph` cascades to every entity and
relationship carrying that `graph_id`, in one transaction. On re-ingest, the
scan **reconciles each source's declared set**: a file re-ingested at a given
`(stable_source_key, path)` first has its **prior** graph(s) for that
`(stable_source_key, path)` deleted (with their entities/relationships) before
the new graph is written, so a changed file replaces rather than duplicates; and
a path present in the previous scan's manifest but **absent** from the current
scan has its graph deleted, so a removed file leaves no stale nodes. The
per-source `EntityKind::Repository` node is reference-counted by its document
graphs: it survives while any remain and is deleted once the last document graph
for its `stable_source_key` is gone. Deletion is **hard** (rows removed, not
tombstoned). The result: after any re-scan, the knowledge **graph** — entities,
relationships, graphs, and the Repository node — reflects the source's current
state.

Scope boundary: this covers the knowledge *graph* records. Retraction of the
underlying `SourceDocument`s, `KnowledgeChunk`s, and their vector embeddings
(keyed by `document_id`/`source_id`, not `graph_id`) is a **separate cascade
deferred** to a follow-up (`knowledge-source-retraction`) — see Assumptions and
`docs/backlog.md`. General reference-counting of shared *contract* nodes via
entity `source_refs` / relationship `evidence` (RFC-0009 D4) is exercised by
`contract-first-ingestion`; in the code-symbol graph the only node shared across
document graphs is the per-source Repository node, ref-counted here by its graph
count. The compound reconcile (delete_graph → recount → delete Repository node)
is safe under `SqlKnowledgeStore`'s single-writer connection mutex; concurrent
multi-writer re-ingest is out of scope at demo scale.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Implement `delete_entity(&self, id, scope)` / `delete_relationship(&self, id, scope)`
  on `KnowledgeRepository` and `delete_graph(&self, id, scope)` on
  `KnowledgeGraphRepository`, each with a default impl (returns a not-supported
  error) overridden by `SqlKnowledgeStore`; each returns whether a row was deleted.
- Make `delete_graph` cascade to all entities and relationships with that
  `graph_id`, in a single transaction, scope-checked.
- On re-ingest of a `(stable_source_key, path)`, delete the prior graph(s) for
  that exact pair (and their entities/relationships) before writing the new graph.
- After a scan, delete the graph for every path in the previous manifest that is
  absent from the current scan (removed files converge away).
- Delete the per-source `Repository` node once its last document graph for the
  `stable_source_key` is removed.

### Ask first

- Changing the fine-grained delete port signatures (e.g. to a `ForgetRequest`
  shape) — the simple `(id, scope) -> bool` form is the default.
- Introducing soft-delete/tombstones instead of hard delete.
- Deleting across tenants, or any delete that is not scope-checked.

### Never do

- No new coarse "delete-by-source" or "reconcile" **method on the knowledge
  trait** — the reconciliation lives in `adapters/ingest` composing the
  fine-grained ports, so the already-broad `KnowledgeRepository` does not grow a
  god-method (RFC-0009).
- No new top-level crate or module boundary; changes live in `core/knowledge`
  (trait signatures), `adapters/knowledge/sqlite`, and `adapters/ingest`.
- No domain type change; no generated-contract change.
- No deletion that ignores `scope` (a delete must not cross the tenant/scope it
  is issued for).

## Testing Strategy

- **`delete_graph` cascade** (deleting a graph removes its entities +
  relationships by `graph_id`, and only those): **TDD** at the SQLite adapter —
  a compressible invariant with clear before/after row counts.
- **Fine-grained deletes are scope-checked** (a delete issued under a different
  tenant/scope does not remove the row; returns `false`): **TDD** at the adapter.
- **Re-ingest replaces a changed file** (re-ingesting the same `(key, path)` with
  changed content yields the new graph and leaves no orphaned prior entities):
  assertion-based **integration** test over the scan→store path.
- **Removed file converges away** (a file in the prior manifest, absent from the
  new scan, has its graph + entities deleted): assertion-based **integration** test.
- **Repository-node lifecycle** (survives while ≥1 document graph remains for the
  key; deleted when the last is removed): assertion-based **integration** test.
- **No generated-contract change** (`pnpm run contracts:generate` no diff):
  **goal-based check**.

## Acceptance Criteria

- [x] `KnowledgeRepository` exposes `delete_entity` and `delete_relationship`, and `KnowledgeGraphRepository` exposes `delete_graph`, each scope-checked and returning whether a row was deleted; `SqlKnowledgeStore` implements all three.
- [x] `delete_graph(id, scope)` removes the graph row and every entity and relationship with that `graph_id` (and no records of other graphs) in a single transaction.
- [x] A fine-grained delete issued under a scope that does not match the record's scope does not delete it and returns `false`.
- [x] Re-ingesting the same `(stable_source_key, path)` with changed content deletes the prior graph and its entities/relationships before writing the new ones — no orphaned prior **graph records** (entities/relationships/graph) remain, and the graph count for that `(key, path)` is 1. (Prior `SourceDocument`/`KnowledgeChunk`/embedding retraction is deferred — see Assumptions.)
- [x] A path present in the previous scan's manifest but **not observed** during the current scan's walk (genuinely absent — not merely skipped, denylisted, or oversize) has its graph (and entities/relationships) deleted after the scan.
- [x] The per-source `EntityKind::Repository` node remains while at least one document graph for its `stable_source_key` exists, and is deleted once the last one is removed.
- [x] Deletion is hard (rows are removed; no tombstone column is added).
- [x] No domain or generated-contract change: `pnpm run contracts:generate` yields no diff.

## Assumptions

- Technical: `KnowledgeRepository` owns entity/relationship (`put_entity` has a default impl, lib.rs:37); `KnowledgeGraphRepository` owns graphs (lib.rs:84-96) — deletes mirror that placement with default impls + SQLite override (source: repo read).
- Technical: `SqlKnowledgeStore` is the only implementor of both traits (service.rs:278,494; in-memory retired) — minimal blast radius (source: grep).
- Technical: Phase-1 added `list_graphs_by_source`/`list_entities_by_source`/`list_relationships_by_source` and graphs carry a `path` column (service.rs:159+) — the reconciler can find prior graph(s) for `(key, path)` (source: repo read).
- Technical: the scanner tracks a prior→new manifest (rel path → hash, scanner.rs:148,502) — removed paths are computable for scan-level graph deletion (source: repo read).
- Technical: adding trait methods does not touch generated contracts (confirmed by Phase-1 T7) (source: prior gate run).
- Process: destructive + public-port change → heavy; reconciliation placed in `adapters/ingest` to avoid a god-method on the knowledge trait (source: RFC-0009 god-trait concern; user confirmation 2026-07-04).
- Technical: `delete_graph` cascades over `knowledge_entities`/`knowledge_relationships` (by `graph_id`) only; `knowledge_chunks`/`knowledge_documents` (keyed by `document_id`/`source_id`) and their sqlite-vec embeddings are NOT cascaded here — document/chunk/embedding retraction is **deferred** to `knowledge-source-retraction` (source: schema.rs:34-46, index.rs:193-203; deferred: knowledge-source-retraction).
- Technical: the compound Repository-node reconcile is atomic only under `SqlKnowledgeStore`'s single-writer connection mutex; concurrent multi-writer re-ingest safety is out of scope at demo scale (source: repo read; user confirmation 2026-07-04).
