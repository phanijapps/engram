# ADR 0009: Retrieval-composition seam

## Status

Accepted (records the decision adopted by [RFC-0005](../rfcs/0005-backend-agnostic-retrieval-composition.md), 2026-07-01).

## Context

Engram's demo proves two retrieval paths — the KG-only path (87.5% on the
code-intelligence eval) and lazy query-time embeddings (warm-up confirmed). Both
bypass the storage-neutral retrieval ports: Q&A grounding lives in a bespoke
TypeScript `buildEvidence` that is not a `RetrievalIndex`, does not fuse with
scores, and is wired to one SQLite graph store. The scale target is two specific
deployments — **pgvector(graph+vector)** (all-in-Postgres) and
**pgvector(vector)+neo4j(graph)** (split) — and reaching either by reworking the
bespoke path per backend is the cost this decision exists to avoid.

## Decision

Adopt the **retrieval-composition seam** as the integration contract every store
plugs into:

```
RetrievalIndex (per source) → RetrievalFusion (RRF) → ContextComposer → ContextPayload
```

1. Each store plane — graph, chunks, vectors — is an independently-swappable
   adapter behind `RetrievalIndex`, returning ranked, policy-checked
   `RetrievalResult`s of some `RetrievalTargetType`. **Implementation is
   SQLite-only for now** (knowledge/sqlite graph + sqlite-vec vectors); the two
   Postgres/Neo4j deployments are documented targets realized by follow-on
   adapter ADRs.
2. **Reciprocal Rank Fusion** (RRF, k=60) is the default cross-source fusion in
   `core/retrieval`. Reranking strength is configurable (RRF `k` + per-source
   weights) with defaults when config is absent.
3. **The target-type/mechanism-agnostic principle:** a `RetrievalIndex` is
   defined by *what it returns* (`RetrievalTargetType`), never by *how* it
   retrieves (traversal vs vector vs lexical). This guarantees "pgvector for
   graph" is an adapter choice, not an architecture change.
4. The composition orchestrator + backend-selection config live in
   `core/orchestration`; `core/retrieval` keeps the ports + fusion algorithms,
   store-free.
5. Durable sqlite-vec (`SqliteVectorIndex::open(path)`) is the first persistent
   vector backend; `content_hash`-keyed upsert + dead-vector GC are follow-on.

This is the **read path only**. Distributed cross-store write consistency is out
of scope.

## Consequences

- **Positive:** Reaching pgvector(graph+vector) or pgvector(vector)+neo4j(graph)
  is a config + adapter change, not an architecture change. Fusion is reusable,
  unit-tested, and backend-agnostic. Embeddings persist; re-index reuses them.
- **Negative:** A bounded refactor of the demo's Q&A to route through the seam,
  plus a new graph `RetrievalIndex` and orchestrator. Short-term regression risk
  on Q&A quality, mitigated by the benchmark re-run.
- **Follow-on:** this ADR + the `backend-agnostic-retrieval` spec drive the
  implementation; deployment-specific ADRs (pgvector(graph+vector);
  pgvector(vector)+neo4j(graph)) are non-blocking follow-ons.

## References

- [RFC-0005](../rfcs/0005-backend-agnostic-retrieval-composition.md) — full proposal, decisions D1–D7, de-risk spikes.
- ADR-0005 (storage-adapter-semantics), ADR-0006 (first-sql-adapter-sqlite), ADR-0007 (napi-binding-surface-extension).
- Spec [`backend-agnostic-retrieval`](../specs/backend-agnostic-retrieval/spec.md).
