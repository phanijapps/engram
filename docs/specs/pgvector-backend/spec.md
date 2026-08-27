# Spec: pgvector-backend (RFC-0017 Phase A)

Status: Draft
Mode: full (new engine — structural, multi-crate, conformance-gated)
Shape: data
Constrained by: ADR-0022 (engine neutrality, recipe = feature-gated submodule, one crate per backend), RFC-0017 (the 3-module + pgvector target), RFC-0005 (backend-agnostic retrieval composition)
- **Contract:** none — reuses every existing port (`MemoryService`, `KnowledgeRepository`, `BeliefRepository`, `VectorIndex`, etc.); the domain types are unchanged. pgvector is an additive engine.

## Objective

Add **Postgres + pgvector** as engram's second storage backend so the 3-module
target architecture (RFC-0017: concurrent ingestion + retrieval + maintenance
writers) has a concurrent-writer-capable store. SQLite stays the local/default;
backend is chosen by config (`engine: pgvector`). One Postgres holds the graph
(entities/relationships/graphs), chunks/documents/sources, memory records, AND
embeddings (pgvector type) + keyword (tsvector/pg_trgm) — the
`pgvector(graph+vector)` shape from the backlog.

This is **Phase A** of RFC-0017. It unblocks Phases B–D (the 3-module
separation needs concurrent writers, which SQLite's single-writer model
serializes).

## ADR-0022 implementation shape (verified from the accepted ADR)

Per the 2026-07-16 amendments:
- **Recipe = feature-gated engine submodule** of `core/integration` — NOT a
  separate crate (Cargo cycle: bootstrap returns `EngramProvider`, owned by the
  facade). SQLite = `src/sqlite/`; SurrealDB = `src/surreal/`. pgvector =
  `src/pgvector/` (new). Each is an ADR-0022-exempt engine zone.
- **One crate per backend**: `adapters/pgvector/` consolidates every capability
  cell behind a shared Postgres connection pool (like `adapters/sqlite/` does for
  SQLite).
- **Only the thin `bootstrap_pgvector`** (returns the facade-owned
  `EngramProvider`) lives in `core/integration/src/pgvector/`; the cells live in
  `adapters/pgvector/`.
- **Config**: `EngramConfig.engine: EngineKind` gains a `Pgvector` variant with
  a connection-string field. `EngramProvider::open` dispatches to
  `bootstrap_pgvector` when the engine is selected (feature-gated).

## Phase A scope (the read/write hot path)

| Capability | Postgres shape | Priority |
|---|---|---|
| **memory** | `memories` table (JSON `record_json` + scope columns + `created_at` index) | P0 |
| **knowledge** (sources/documents/chunks/entities/relationships/graphs) | Relational tables mirroring the SQLite schema, using Postgres JSONB for record_json | P0 |
| **graph** (traversal/neighbors) | SQL recursive CTEs over the relationships table (or pg_trgm for name resolution) | P0 |
| **vector** | `pgvector` type (`embedding vector(dimensions)`) + `ivfflat` or `hnsw` index; `VectorIndex` port impl | P0 |
| **lexical** (keyword) | `tsvector` column on chunk text + GIN index; `RetrievalIndex` port impl (replaces Tantivy for this engine) | P1 |
| **unified_recall** | Composes the above lanes via the existing `SqlUnifiedRecall`-equivalent (Postgres connection shared) | P0 |
| belief / hierarchy / procedures / identity | Same table-per-capability pattern; lower priority (P2 — ship after the hot path passes conformance) | P2 |

## Boundaries

### Always do
- Reuse every port trait; no new domain types. pgvector is purely an adapter.
- Pass the same conformance suite as SQLite (`engram-conformance::Sql*` probes) — a capability is `Supported` only when its conformance check passes.
- Connection lifecycle: a shared `tokio-postgres` (or `sqlx`) connection pool, owned by the store crate. No per-operation connections.
- Feature-gate: `--features pgvector` in `core/integration`; off by default (SQLite is the zero-dep default).

### Ask first
- `sqlx` vs `tokio-postgres` + `deadpool-postgres` for the pool. (Leaning: `sqlx` — compile-time SQL check + pool + migration support.)
- HNSW vs IVFFlat index for pgvector. (Leaning: HNSW — better recall, no training step.)
- Whether to reuse the existing `engram-store-sqlite` SQL or write Postgres-native SQL. (Leaning: Postgres-native — JSONB, array types, recursive CTEs differ from SQLite's dialect.)

### Never do
- Name `Pg*` / `sqlx` / `tokio_postgres` types in `core/` port crates or the facade (engine neutrality — ADR-0022 lint).
- Change the domain model or the port traits.
- Make pgvector the default backend (SQLite stays zero-dep default).
- Couple pgvector cells to SQLite internals (cross-adapter SQL / shared connections across crate boundaries — AGENTS.md boundary rule).

## Testing Strategy

- **Conformance**: the existing `engram-conformance` probes (`Sql*` fixtures) run against the pgvector store. Each capability must pass the same assertions as SQLite. This is the primary gate — no capability ships without conformance green.
- **Integration**: a `PgUnifiedRecall` integration test (mirror of `SqlUnifiedRecall` tests) over a test Postgres instance (docker-compose or CI service).
- **Migration**: `export_import` capability moves data SQLite → pgvector (a dry-run + apply round-trip test).

## Revision (2026-08-27): production-switch gap assessment

Reality check against the codebase: the P0 hot path largely **shipped** —
`adapters/pgvector/` (memory, knowledge+graph, vectors, schema) and the
`backends/pgvector` recipe (connection lifecycle, idempotent schema, cell
composition incl. belief/hierarchy/procedures cells, `PgUnifiedRecall`) exist,
and `pgvector-recipe` is Shipped. Two of the original ACs are obsolete by the
recipe's design (pgvector no longer routes through `EngramProvider::open` —
hosts call `backends::pgvector::open` directly; that supersedes the
"`EngramConfig` dispatch" AC and is *better* for neutrality).

What actually blocks a production switch (verified 2026-08-27):

1. **No agent surface.** `engram-mcp` is SQLite-only — no pgvector wiring, no
   launch flag. The switch does not exist for the MCP/agent path.
2. **Recall is degraded on Postgres.** `PgUnifiedRecall` composes facts +
   beliefs lanes only — the vector lane (the `PgVectorIndex` cell exists but is
   not fused into recall) and lexical lane (no tsvector implementation) are
   missing. SQLite recall fuses 6 lanes; Postgres would fuse 2.
3. **No CI safety net.** CI runs no Postgres service; conformance/
   integration tests against Postgres are not in the gate.
4. **No migration runbook.** SQLite → Postgres export/import round-trip is
   specced but not demonstrated end-to-end.
5. **Embed/reindex operations.** `scan_repo` embeds 256 chunks/call scoped to
   the scan's source; there is no reindex op with progress for initial
   backfill (needed on ANY backend switch — vectors start empty).

These become the working ACs for promoting this spec; the original Phase-A
ACs below stay as the (mostly met) historical record.

## Acceptance Criteria (production switch — working set)

- [x] PS1 — `engram-mcp --storage <dir> --backend pgvector` (or the
  equivalent config env/flag) opens through `backends::pgvector::open` and
  serves the full tool surface on Postgres; `capability_report` is honest
  about lanes that are missing. **Shipped 2026-08-27** (feature `pgvector`):
  `--backend sqlite|pgvector` + `--pg-connection-string` (or
  `ENGRAM_PG_CONNECTION_STRING`), dispatch in `open_provider`; the recipe
  gained a `PgKnowledgeQuery` (the read surface behind search /
  symbol_context / architecture / the scan's lexical delta + embed listing —
  previously `knowledge_query not wired` on Postgres). Verified live against
  Docker Postgres: scan lands 6 entities + 5 relationships (3 resolved `calls`
  edges confirmed via SQL), `symbol_context("alpha")` → `callers:
  ["beta","gamma"]`, memory write→recall round-trips, capability report shows
  `knowledge_query: supported` and honestly `unsupported` for lexical /
  consolidation / ontology / taxonomy / identity / vector-lanes.
- [x] PS2 — recall on Postgres fuses the vector lane (and documents the
  lexical gap or ships a tsvector lane). **Shipped 2026-08-27:**
  `PgUnifiedRecall` composes facts + beliefs + the vector lane (embed →
  pgvector search → chunk rehydration) with per-lane `source_failures`
  degradation; the recipe wires a FastEmbed `EmbeddingProvider` under its
  `fastembed` feature (model-load failure degrades, never fails boot), also
  enabling scan-side chunk embedding. Two latent bugs found + fixed: the
  pgvector `$n::vector` param binding always failed client-side in
  tokio-postgres (cast via text now — the lane had never actually run
  against real Postgres), and `PgKnowledgeQuery::list_chunks` queried scope
  columns that don't exist on `knowledge_chunks` (now joined through
  `knowledge_sources`). Verified live: scan embeds chunks into pgvector;
  recall returns the semantically-matching chunk first (no lexical lane
  exists on pg — the hit could only arrive via the vector lane). Lexical
  (tsvector) remains a documented gap.
- [ ] PS3 — CI runs a Postgres service (docs/how-to-pg compose shape) and the
  pgvector conformance + integration tests are part of the gate.
- [ ] PS4 — migration runbook demonstrated: SQLite export → Postgres import →
  recall parity spot-check; documented under docs/guides/how-to/.
- [ ] PS5 — a reindex op (MCP tool or CLI) drains the embed backlog with
  progress, keyed on the vector index's embedded-set, not per-scan scope.
- [ ] PS6 — ops hardening: schema versioning/migration strategy beyond
  idempotent DDL, connection-string/TLS/pool validation errors surfaced at
  `open`, and a backup/restore runbook page.

## Acceptance Criteria (Phase A — P0 hot path, historical)

- [ ] `adapters/pgvector/` crate exists with `PgMemoryService`, `PgKnowledgeStore`, `PgVectorIndex` implementing the P0 ports.
- [ ] `core/integration/src/pgvector/bootstrap.rs` constructs an `EngramProvider` from a Postgres config (feature-gated `pgvector`).
- [ ] `EngramConfig` carries a `Pgvector` engine variant + connection string; `EngramProvider::open` dispatches to it.
- [ ] Memory + knowledge/graph + vector pass the `engram-conformance` probes (same suite as SQLite).
- [ ] `cargo check --workspace --features pgvector` compiles (0 warnings).
- [ ] Engine-neutrality lint passes (no `Pg*` in neutral layers).
- [ ] A migration round-trip (SQLite export → pgvector import → recall returns the same results) is demonstrated.

## Explicit deferrals

- **P2 cells** (belief, hierarchy, procedures, identity) — land after P0 conformance green.
- **Phases B–D of RFC-0017** (ingestion module, maintenance service, events/streaming) — these CONSUME the pgvector backend but are separate specs.
- **HNSW vs IVFFlat tuning** — ship with a sensible default; tune after benchmarks.
