# Spec: Knowledge Ingestion

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** contracts/v1/memory.schema.json
- **Shape:** data

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can ingest source-grounded code and unstructured document text into
knowledge records without turning those records into memories. A caller can
register a source, create a versioned source document, chunk the document into
retrievable units, and persist the resulting `KnowledgeSource`,
`SourceDocument`, and `KnowledgeChunk` records through core repository ports.

## Boundaries

### Always do

- Preserve the distinction between memory records and source-grounded knowledge.
- Preserve source, document, chunk, provenance, policy, scope, and hash data.
- Keep ingestion behavior in `engram-ingest`, not in `engram-core` or domain
  types.
- Keep storage behind `KnowledgeRepository` implementations.

### Ask first

- Add filesystem crawling, Git traversal, URL fetching, PDF parsing, or code AST
  extraction.
- Change accepted domain or JSON schema contracts.
- Generate embeddings during PHASE08.

### Never do

- Store source documents as memories.
- Add vector providers, model providers, or embedding dependencies to
  `engram-domain`, `engram-core`, or `engram-ingest`.
- Hide source provenance inside untyped metadata when typed fields exist.
- Create a god ingestion service that owns reading, parsing, chunking, storage,
  embedding, retrieval, and policy.

## Testing Strategy

- Ingestion assembly: TDD through Rust tests that ingest a text document and
  assert source, document, chunk, provenance, policy, and hash fields.
- Re-ingestion behavior: TDD through deterministic duplicate input tests that
  prove stable IDs and hashes for unchanged content.
- Repository persistence: integration tests through `InMemoryMemoryService` as a
  `KnowledgeRepository`.
- Workspace hygiene: goal-based checks through Rust, contract, code-doc, and
  TypeScript gates.

## Acceptance Criteria

- [x] `engram-ingest` creates `KnowledgeSource`, `SourceDocument`, and
  `KnowledgeChunk` records from one source-grounded text input.
- [x] Plain-text chunking is deterministic and preserves source locations where
  line boundaries are known.
- [x] Re-ingesting unchanged input produces stable source, document, and chunk
  identifiers plus matching content hashes.
- [x] In-memory storage persists knowledge records behind `KnowledgeRepository`
  without mixing them into memory state.
- [x] Chunks carry provenance, policy, source ID, document ID, content hash, and
  no embedding refs in PHASE08.

## Assumptions

- Technical: domain already separates `KnowledgeSource`, `SourceDocument`, and
  `KnowledgeChunk` from memories (source: `crates/engram-domain/src/knowledge.rs`).
- Technical: `engram-core` exposes `KnowledgeRepository` for source, document,
  and chunk persistence (source: `crates/engram-core/src/lib.rs`).
- Technical: in-memory state needs a focused knowledge state path because it
  currently stores memories, events, and idempotency only (source:
  `crates/engram-store-memory/src/state.rs`).
- Process: crate roots remain facades and behavior lives in focused modules
  (source: `AGENTS.md`).
- Product: code and unstructured documents are stored as knowledge, not memory
  (source: user confirmation 2026-06-29).
- Product: filesystem, Git, URL, PDF, code AST, and embedding integrations stay
  outside this first PHASE08 slice (source: user confirmation 2026-06-29).
