# ADR-0018: Knowledge-graph retraction and convergence on re-ingest

- **Status:** Accepted
- **Date:** 2026-07-04
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** RFC-0009 (knowledge-graph retraction), ADR-0017 (repository model — supplies the SHA-free stable-source-key), RFC-0008 / `docs/specs/contract-first-ingestion/` (unblocked by this), RFC-0001 (memory `forget` precedent)

## Decision summary

- **Decision:** The knowledge layer gains **retraction** — `delete_*` ports on `KnowledgeRepository` plus per-source **declared-set reconciliation** on re-ingest (diff current vs prior → add/update/delete), ref-counting shared nodes so a node dies only when its last source retracts.
- **Because:** continuous re-ingestion must converge the graph to each source's current state; today it only accretes, so removed/renamed records linger forever.
- **Applies to:** the knowledge graph (code-symbol and contract nodes). Not the memory layer (which already has `forget`) or the belief layer.
- **Tradeoff accepted:** adds methods to a public port and uses hard delete (no built-in history); the diff requires a new SHA-free attribution key.
- **Revisit if:** the SHA-free `(stable-source-key, path)` diff basis proves insufficient, or a graph history/audit requirement makes soft-delete necessary.

## Context

The knowledge layer has no deletion operation or port for entities/relationships/graphs (`core/knowledge/src`, `adapters/knowledge/sqlite/src`), and re-ingest is add-only: the manifest skips unchanged files but a changed file upserts and nothing removes what it superseded (`adapters/ingest/src/scanner.rs`, `ingestor.rs`). Memory already supports retraction (`forget`, `core/memory/src/lib.rs:79`), so knowledge's lack is an asymmetry. The existing `source_id` embeds the commit SHA (via `source_name`), so it is not a stable diff key. RFC-0009 carries the full analysis, options, and prior art.

## Decision

**We will add retraction and per-source declared-set reconciliation to the knowledge layer**, as specified in RFC-0009:

- `KnowledgeRepository` gains `delete_entity`, `delete_relationship`, `delete_graph`, and a coarse `delete_by_source` (mirroring memory's `forget`).
- Re-ingest of a source reconciles that source's current declared set against its prior set — add new, update changed, retract removed — keyed by a **SHA-free `(stable-source-key, path)`** attribution (the stable-source-key is ADR-0017's normalized repo identity), recorded via an additive column on entities/relationships.
- Records are reference-counted by their source-attribution refs (entity `source_refs` / relationship `evidence`); a shared node is deleted only when its last contributing source retracts.
- Applies graph-wide (code-symbol and contract).
- **Boundary:** the exact attribution mechanism, hard-delete-vs-tombstone storage, and whether the coarse reconcile surface lives on a narrower port are spec-level decisions (RFC-0009 open questions).

## Decision drivers

- **Convergence** — the graph must reflect current source state, not accrete forever.
- **Stable diff basis** — the reconciliation key must survive commits (SHA-free), unlike `source_id`/`document_id`.
- **Layer symmetry** — retraction belongs on the knowledge port, as `forget` does on memory.

## Consequences

**Positive:**
- The graph converges on re-ingest; stale records no longer accumulate.
- Unblocks continuous-update for `contract-first-ingestion` (RFC-0008) and fixes drift in the shipped code-symbol graph.
- Reuses an established pattern (memory `forget`; desired-state reconciliation with prune).

**Negative:**
- Adds methods to an already-broad `KnowledgeRepository` (the coarse surface may warrant a separate reconciler port — spec decision).
- Hard delete forgoes built-in history; retraction is convergence, not an audit log.
- Requires a new SHA-free attribution key that is not backfillable (recomputed on re-scan).

**Revisit if:** the `(stable-source-key, path)` diff basis proves insufficient, or a graph-history/audit requirement makes soft-delete necessary.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** re-ingesting a source after removing a record deletes that record's contribution and converges the graph; a shared node survives until its last source retracts.
- **Owner:** maintainer (phanijapps).

## Alternatives considered

- **Delete-all-then-reinsert per source.** Rejected against *stable diff basis*: churns every id each scan, breaking stable identity and embeddings.
- **Tombstone / soft-delete as the convergence strategy.** Rejected: read-time filtering cost and unbounded growth; retraction is convergence, not history.
- **Do-nothing (add-only).** Rejected: the graph drifts permanently stale — the status quo this decision exists to fix.

## References

- RFC-0009 (full analysis, options, external prior art: kubectl prune, K8s controllers, ES delete_by_query, Sourcegraph uploads, Terraform destroy).
- ADR-0017 (SHA-free stable-source-key); `core/memory/src/lib.rs:79` (`forget`).
