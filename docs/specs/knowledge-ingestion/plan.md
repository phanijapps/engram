# Plan: Knowledge Ingestion

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a small `engram-ingest` crate that converts caller-provided text into
portable knowledge records and writes them through `KnowledgeRepository`.
Extend the in-memory adapter with knowledge maps so ingestion can be tested
without SQL migrations, vector indexes, or external readers. Keep PHASE08
source-grounded and deterministic; PHASE09 owns embeddings and vector retrieval.

Tempted to add file walkers and Git readers; declining because the first slice
should prove the knowledge contract before I/O adapters. Tempted to embed chunks
now; declining because the requested SQLite `sqlite-vec` plus FastEmbed BGE-small
work belongs to PHASE09. Tempted to add a generic ingestion manager; declining
because a focused text ingestor and chunker are enough.

## Constraints

- ADR-0003 keeps Rust behavior in crates and TypeScript as wrappers.
- ADR-0004 keeps accepted contract artifacts as the source of truth.
- Knowledge domain types are already portable and storage-neutral.
- PHASE09 must use SQLite `sqlite-vec` and FastEmbed BGE-small for vector tests,
  so PHASE08 must not pick a competing embedding path.

## Construction tests

**Integration tests:** ingest a text document into `InMemoryMemoryService` and
read a chunk back through `KnowledgeRepository`.

**Manual verification:** none.

## Design (LLD)

### Data & schema

`engram-ingest` builds existing domain records only:
`KnowledgeSource`, `SourceDocument`, and `KnowledgeChunk`. Content hashes are
SHA-256 strings. Chunk embedding refs remain empty.

### Interfaces & contracts

The ingestion crate exposes a `KnowledgeIngestor` over `KnowledgeRepository`,
`Clock`, `IdGenerator`, and a `Chunker`. The repository contract remains the
core trait; no new public JSON schema is introduced in this slice.

### Component / module decomposition

- `request.rs` owns caller input and options.
- `hash.rs` owns stable content hash formatting.
- `chunker.rs` owns deterministic plain-text chunking.
- `ingestor.rs` owns record assembly and repository writes.
- `engram-store-memory::knowledge` owns in-memory repository persistence.

### Failure, edge cases & resilience

Empty source names, missing tenant scope, empty document text, and zero-sized
chunk options fail before repository writes. Re-ingestion stability comes from
content-derived identifiers rather than process-local counters.

### Dependencies & integration

`sha2` is the only new PHASE08 dependency. It is local hashing infrastructure,
not an embedding or model provider.

## Tasks

### T1: In-memory knowledge repository persists source, document, and chunks

**Depends on:** none

**Tests:**
- Repository integration test stores a source, document, and chunk.
- `get_chunk` applies scope boundaries.

**Approach:**
- Add knowledge maps to `InMemoryState`.
- Implement `KnowledgeRepository` for `InMemoryMemoryService` in a focused
  module.

**Done when:** in-memory knowledge repository tests pass.

### T2: Deterministic text ingestion creates portable knowledge records

**Depends on:** T1

**Tests:**
- Ingesting text creates one source, one document, and one or more chunks.
- Re-ingesting unchanged text yields stable IDs and hashes.

**Approach:**
- Add `engram-ingest` with request, hash, chunker, and ingestor modules.
- Use content-derived IDs for source, document, and chunks.

**Done when:** ingestion crate tests pass.

### T3: Roadmap state and package docs reflect PHASE08

**Depends on:** T2

**Tests:**
- Code documentation hook passes.
- Full Rust and TypeScript gates pass.

**Approach:**
- Mark PHASE08 complete once tests pass.
- Move PHASE09 into progress with the SQLite vector testing requirement.

**Done when:** roadmap JSON, specs, changelog, and validation gates are clean.

## Rollout

This ships as library code and tests only. No external readers, migrations,
model downloads, or service deployments are included.

## Risks

- Plain-text chunking is intentionally simple and not code-symbol aware yet.
- SQL knowledge persistence is deferred, so PHASE08 conformance starts with the
  in-memory adapter only.
- Retrieval composition across memory and knowledge remains a later slice.

## Changelog

- 2026-06-29: initial plan for deterministic text knowledge ingestion.
- 2026-06-29: implemented `engram-ingest` plus in-memory knowledge repository
  support.
