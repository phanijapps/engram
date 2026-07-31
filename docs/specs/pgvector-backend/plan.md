# Plan: pgvector-backend (RFC-0017 Phase A)

## Design (LLD) — grounded in the reindexed codebase

### What exists (the SQLite pattern to mirror)

From the reindexed graph + this session's work, the current SQLite backend:

- **`core/integration/src/sqlite/bootstrap.rs`** — `bootstrap_sqlite(config) -> EngramProvider`. Constructs `SqlMemoryService`, `SqlKnowledgeStore`, `SqlBeliefStore`, `SqlHierarchyStore`, `SqlProcedureStore`, `SqlIdentityStore`, each gated by an inlined conformance check. Builds `retrieval_lanes` (graph + associative + community-summary + lexical + temporal). Composes `SqlUnifiedRecall`. Returns the facade-owned `EngramProvider`. Feature-gated `sqlite`.
- **`adapters/sqlite/`** — ONE crate consolidating all SQLite capability cells behind a shared `rusqlite::Connection` (Arc<Mutex>). Memory/knowledge/graph/belief/hierarchy/procedures/identity + vector (sqlite-vec) + schema + scope helpers.
- **`EngramConfig`** — carries `storage_path`, `sqlite_storage_layout`, `embedding_provider`. `EngramProvider::open` dispatches to `bootstrap_sqlite` (feature-gated). No explicit engine-kind enum today (SQLite is the only engine, so dispatch is implicit).
- **Conformance** — `core/integration/src/sqlite/conformance.rs` — inline checks (`memory_ok()`, `knowledge_ok()`, `recall_ok()`, etc.) that probe an in-memory store and return bool. Each capability attaches only when its check passes.

### What pgvector adds (additive, per ADR-0022)

```
adapters/pgvector/                 NEW crate (engram-store-pgvector)
  src/
    lib.rs                         facade: mod declarations + re-exports
    pool.rs                        Postgres connection pool (sqlx::PgPool)
    schema.rs                      CREATE TABLE + pgvector extension + HNSW index
    memory.rs                      PgMemoryService: MemoryService impl
    knowledge.rs                   PgKnowledgeStore: KnowledgeRepository + KnowledgeGraphRepository impl
    vector.rs                      PgVectorIndex: VectorIndex impl (pgvector type)
    recall.rs                      PgUnifiedRecall: UnifiedRecall impl (mirrors SqlUnifiedRecall)
    conformance.rs                 inline probes (mirror sqlite/conformance.rs)

core/integration/src/pgvector/     NEW feature-gated submodule
    mod.rs                         pub(crate) mod bootstrap;
    bootstrap.rs                   bootstrap_pgvector(config) -> EngramProvider
```

`EngramConfig` gains:
```rust
pub enum Engine {
    Sqlite,                              // current default (zero-dep)
    Pgvector { connection_string: String }, // new
}
```
`EngramProvider::open` dispatches: `Engine::Pgvector` → `bootstrap_pgvector` (feature `pgvector`).

## Stack

Rust + `sqlx` (compile-time SQL check + pool + migration) + `pgvector` crate (sqlx integration for the `vector` type). New workspace dependencies — this slice ADDS them (unlike prior slices that avoided new deps; pgvector genuinely needs Postgres driver + vector type support).

## Tasks (P0 hot path)

### T1 — crate scaffold + connection pool + schema
Depends on: none
Verification: goal-based (crate compiles; pool connects to a test Postgres; schema creates tables + pgvector extension).
Approach: `adapters/pgvector/Cargo.toml` (deps: sqlx + pgvector + engram-domain + engram-knowledge + engram-memory + engram-retrieval + engram-runtime). `pool.rs` — `PgPool::connect(connection_string)`. `schema.rs` — DDL: `CREATE EXTENSION IF NOT EXISTS vector`, tables mirroring the SQLite schema (memories, knowledge_sources, knowledge_documents, knowledge_chunks, knowledge_entities, knowledge_relationships, knowledge_graphs) with JSONB for record_json + scope columns + indexes. Vector table: `embedding vector(dimensions)` + `CREATE INDEX ... USING hnsw (embedding vector_cosine_ops)`.

### T2 — PgMemoryService
Depends on: T1
Spec mapping: AC#1 (P0 memory)
Verification: TDD (mirror `SqlMemoryService` tests: write → retrieve → forget round-trip + scope isolation).
Approach: `memory.rs` — `MemoryService` impl over the PgPool. write_memory (INSERT ... ON CONFLICT), retrieve (SELECT + rank), forget (DELETE/tombstone). Scope-filtered queries (tenant/subject/workspace columns). Mirror SqlMemoryService's behavior.

### T3 — PgKnowledgeStore
Depends on: T1
Spec mapping: AC#1 (P0 knowledge/graph)
Verification: TDD (mirror SqlKnowledgeStore: put/get entities/relationships/graphs + neighbors traversal + scope-filtered list).
Approach: `knowledge.rs` — `KnowledgeRepository + KnowledgeGraphRepository` impls. put_document/put_chunk/put_entity/put_relationship/put_graph + list/get/delete + list_graphs_by_source + neighbors (recursive CTE for multi-hop). Includes the new retraction ports (list_chunks_by_document, delete_document, delete_chunk) from KSR (#71).

### T4 — PgVectorIndex
Depends on: T1
Spec mapping: AC#1 (P0 vector)
Verification: TDD (insert → search → nearest-first; delete_target; gc_orphan_targets; upsert idempotency).
Approach: `vector.rs` — `VectorIndex` impl. insert (pgvector type + upsert via ON CONFLICT — Postgres supports it natively, unlike vec0). search (`SELECT target_id, embedding <=> $query FROM vectors ORDER BY embedding <=> $query LIMIT n`). delete_target + gc_orphan_targets (Postgres DELETE WHERE target_id NOT IN (...)). embedding_space validation.

### T5 — bootstrap_pgvector + EngramConfig dispatch
Depends on: T2, T3, T4
Spec mapping: AC#2, AC#3
Verification: goal-based (provider opens with a Postgres config; capability_report shows Supported for wired cells; conformance checks pass).
Approach: `core/integration/src/pgvector/bootstrap.rs` — mirror `bootstrap_sqlite`: construct PgMemoryService + PgKnowledgeStore + PgVectorIndex, run conformance checks, build retrieval_lanes (graph + temporal + vector), compose PgUnifiedRecall, return EngramProvider. `EngramConfig` gains the `Engine` enum + `EngramProvider::open` dispatches. Feature-gated `pgvector` in core/integration.

### T6 — conformance probes pass
Depends on: T5
Spec mapping: AC#4
Verification: the conformance suite (mirror the SQLite probes against the pgvector store).
Approach: `adapters/pgvector/conformance.rs` — `memory_ok()`, `knowledge_ok()`, `graph_ok()`, `vector_ok()`, `recall_ok()`. Each opens a test pool, exercises the cell, returns bool. bootstrap gates capabilities on these.

### T7 — integration test (migration round-trip) + gates
Depends on: T6
Spec mapping: AC#5, AC#6, AC#7, AC#8
Verification: integration test + full gate suite.
Approach: integration test: seed a SQLite store → export → import into a pgvector store → recall returns the same results. Requires a test Postgres (docker-compose or CI service). Gates: fmt, check --workspace --features pgvector (0 warnings), engine-neutrality (no Pg* in neutral layers), surface-parity, docs.

## Tempted to add, declining

- P2 cells (belief/hierarchy/procedures/identity) — land after P0 conformance green (explicit deferral in the spec).
- A `tsvector` lexical lane — reuse the existing Tantivy adapter for now (it's engine-independent); a Postgres-native `tsvector` lane is a P1 follow-up.
- Phases B–D (ingestion module, maintenance, events/streaming) — separate specs.
- HNSW tuning — ship with default parameters; tune after benchmarks.

## Rollout

Single PR (or two: crate + bootstrap), branch `feat/pgvector-backend`, off main.
Left open for review — not auto-merged.
