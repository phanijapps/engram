# Spec: Retire Memory In-Memory Adapter

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0005, ADR-0006, ADR-0010, RFC-0001, RFC-0003, RFC-0006, `docs/specs/retire-knowledge-inmem`
- **Brief:** user request, "go for kill"
- **Contract:** none (workspace/test-harness cleanup; no v1 wire-contract change)
- **Shape:** mixed (adapter removal + test migration + documentation)

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram retires the broad process-local `engram-store-memory` crate and makes
SQLite-backed adapters the executable local conformance surface for memory,
knowledge ingestion, belief, and hierarchy persistence. `engram-store-sql`
becomes the memory fixture runner's concrete store through
`SqlMemoryService::open_in_memory()`, while ingestion tests use
`SqlKnowledgeStore::open_in_memory()`. The retired crate's in-memory-only
consolidation executor is removed as an adapter-local prototype rather than
kept as canonical behavior.

## Boundaries

### Always do

- Migrate every active outside dependency on `engram-store-memory` before
  deleting `adapters/memory/inmem`.
- Keep `engram-eval` adapter-neutral and prove accepted evaluation fixtures
  against `SqlMemoryService::open_in_memory()`.
- Keep `engram-ingest` knowledge tests on `SqlKnowledgeStore::open_in_memory()`
  instead of pulling memory storage into knowledge ingestion.
- Preserve memory, knowledge, belief, hierarchy, retrieval, and consolidation
  port boundaries; do not replace the removed crate with another catch-all
  adapter.
- Keep historical shipped specs and research readable as history, while active
  docs and local instructions stop presenting the in-memory memory crate as
  current architecture.

### Ask first

- Changing `engram-memory`, `engram-domain`, `engram-eval`, `engram-belief`,
  `engram-hierarchy`, or `engram-consolidation` public trait signatures.
- Recreating a process-local all-in-one adapter under a different name.
- Reintroducing mutating consolidation algorithms as durable behavior without a
  dedicated replacement spec and adapter boundary.
- Changing accepted v1 JSON schemas or generated TypeScript contracts.

### Never do

- Leave `adapters/memory/inmem` in the workspace after active references are
  migrated.
- Make `engram-store-sql` own knowledge graph, ontology, belief, hierarchy, or
  vector responsibilities.
- Make `engram-store-knowledge-sqlite`, `engram-store-belief-sqlite`, or
  `engram-store-hierarchy-sqlite` depend on the memory SQL adapter.
- Weaken evaluation fixtures, policy checks, scope filtering, idempotency, or
  forget behavior to make the deletion pass.
- Rewrite historical docs to hide that the retired crate existed.

## Testing Strategy

- **TDD / integration:** `engram-eval` fixture-runner tests execute accepted
  retrieval fixtures against `SqlMemoryService::open_in_memory()` so the Rust
  evaluation harness no longer depends on a fake memory store.
- **TDD / integration:** ingest document/code-symbol tests execute against
  `SqlKnowledgeStore::open_in_memory()` so knowledge ingestion remains separate
  from memory persistence.
- **Regression:** SQL memory service tests cover write idempotency, retrieval,
  forget, file-backed persistence, and accepted fixture execution.
- **Goal-based check:** a retirement hook proves active manifests, code, and
  current docs do not reintroduce `engram-store-memory`,
  `InMemoryMemoryService`, `InMemoryConsolidationExecutor`, or
  `adapters/memory/inmem`.
- **Goal-based repository gates:** Rust format/check/tests plus docs and
  contract hooks prove workspace coherence and contract stability.

## Acceptance Criteria

- [x] No active Cargo workspace member, test, example, or package manifest
  depends on `engram-store-memory`.
- [x] `adapters/memory/inmem` is removed from the workspace after its active
  outside usages are replaced by SQLite-backed tests.
- [x] `engram-eval` fixture-runner tests pass while using
  `SqlMemoryService::open_in_memory()` instead of `InMemoryMemoryService`.
- [x] `engram-ingest` document and code-symbol ingestion tests pass while using
  `SqlKnowledgeStore::open_in_memory()` instead of the memory in-memory store.
- [x] SQL memory service tests remain green for accepted evaluation fixture,
  write, retrieve, forget, idempotency, and local file-backed persistence.
- [x] Active architecture docs and local instructions describe SQLite-backed
  local conformance and do not instruct contributors to use the retired memory
  in-memory crate.
- [x] A retirement hook proves `engram-store-memory`,
  `InMemoryMemoryService`, `InMemoryConsolidationExecutor`, and
  `adapters/memory/inmem` have not re-entered active dependencies.
- [x] `cargo fmt --all --check`, `cargo check --workspace`, relevant Rust
  tests, `.codex/hooks/check-contracts.sh`, and
  `.codex/hooks/check-docs.sh` pass.
- [x] No v1 contract schemas, generated TypeScript contracts, or public Rust
  port signatures change.

## Assumptions

- Technical: `SqlMemoryService` implements `MemoryService`,
  `MemoryRepository`, and `MemoryEventRepository`, and has an
  `open_in_memory()` constructor for fast conformance tests (source:
  `adapters/memory/sqlite/src/engine.rs`).
- Technical: `SqlKnowledgeStore` implements the knowledge repository surface
  needed by ingestion tests (source:
  `adapters/knowledge/sqlite/src/service.rs`).
- Technical: active direct outside imports of `InMemoryMemoryService` are
  limited to `core/eval` and `adapters/ingest` tests (source: `rg
  "InMemoryMemoryService|engram-store-memory" --glob '!target/**'`).
- Technical: no active outside code imports `InMemoryConsolidationExecutor`;
  its behavior is tested only by the retiring crate's own test suite (source:
  `rg "InMemoryConsolidationExecutor" --glob '!target/**'`).
- Process: prior research warned that the in-memory memory crate was
  load-bearing until eval and ingest moved to SQLite. This slice performs that
  migration before deletion (source:
  `docs/research/graphmind-prior-art-survey.md`).
