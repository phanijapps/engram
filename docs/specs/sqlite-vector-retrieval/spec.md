# Spec: SQLite Vector Retrieval

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0005, ADR-0006
- **Brief:** none
- **Contract:** contracts/v1/memory.schema.json
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can store and query vector candidates through SQLite `sqlite-vec` without
making embeddings canonical storage. Deterministic sqlite-vec tests prove vector
nearest-neighbor behavior in CI, while an explicit FastEmbed BGE-small test path
proves the intended local embedding provider integration for developers who opt
into model downloads.

## Boundaries

### Always do

- Keep vector bytes and model-provider behavior outside `engram-domain` and
  `engram-core`.
- Treat vector indexes as secondary indexes over memory, chunk, entity, or
  concept targets.
- Report vector adapter failures through typed adapter errors.
- Preserve a deterministic sqlite-vec test that does not download a model.

### Ask first

- Make semantic retrieval mandatory for `MemoryService::retrieve`.
- Add a hosted embedding provider or network service.
- Change vector dimensions or model choice for the FastEmbed smoke path.

### Never do

- Store embeddings as canonical domain records.
- Make `fastembed` a dependency of core, domain, SQL memory storage, or ingest.
- Hide vector source failures as empty successful retrievals.
- Replace exact/keyword retrieval with vector-only behavior.

## Testing Strategy

- SQLite vector behavior: TDD through an in-memory sqlite-vec adapter test using
  fixed vectors and nearest-neighbor assertions.
- FastEmbed integration: opt-in ignored integration test using
  `EmbeddingModel::BGESmallENV15` to generate BGE-small embeddings and query
  sqlite-vec.
- Workspace hygiene: goal-based Rust, contract, documentation, and TypeScript
  gates.

## Acceptance Criteria

- [x] `engram-store-vector` registers sqlite-vec and creates an in-memory vector
  index.
- [x] Fixed-vector tests insert candidates and return nearest targets in score
  order.
- [x] Vector rows include target type, target ID, model, dimensions, and content
  hash metadata.
- [x] FastEmbed BGE-small test code exists and uses `BGESmallENV15`, but is
  ignored by default to avoid implicit model downloads in normal gates.
- [x] No core/domain crate depends on sqlite-vec or fastembed.

## Assumptions

- Technical: `sqlite-vec` exposes `sqlite3_vec_init` and registers with rusqlite
  through `sqlite3_auto_extension` (source: `sqlite-vec` crate docs/source).
- Technical: sqlite-vec `vec0` tables support float vector columns and KNN
  queries with `embedding match ?` and `k = N` (source:
  https://alexgarcia.xyz/sqlite-vec/features/vec0.html).
- Technical: FastEmbed supports `TextEmbedding::try_new`, `embed`, and
  `EmbeddingModel::BGESmallENV15` (source: Context7 `/anush008/fastembed-rs`).
- Process: vector stores and embedding providers stay out of core/domain
  contracts (source: `AGENTS.md`).
- Product: SQLite vector testing uses `sqlite-vec` plus FastEmbed BGE-small
  (source: user confirmation 2026-06-29).
- Product: hybrid fusion policy beyond vector nearest-neighbor order is deferred
  (source: user confirmation 2026-06-29).
