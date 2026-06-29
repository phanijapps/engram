# RFC 0002: Knowledge Source Extension

## Status

Draft

## Context

The initial memory layer focuses on agent memory: observations, events, facts,
preferences, provenance, policy, retrieval, and evaluation.

The system should later support code repositories and unstructured documents as
knowledge sources. These should extend the knowledge layer without turning the
core memory model into a document parser, code indexer, or vector database.

## Decision Direction

Treat code and unstructured documents as external knowledge sources that are
ingested, normalized, indexed, and retrieved through composable adapters.

Keep the distinction clear:

- `MemoryRecord`: what the agent experienced, inferred, or was explicitly told.
- `KnowledgeChunk`: source-grounded content extracted from files, documents,
  repositories, URLs, or other corpora.

These models connect through provenance:

```text
MemoryRecord
  derived_from -> KnowledgeChunk

KnowledgeChunk
  sourced_from -> SourceDocument

SourceDocument
  sourced_from -> KnowledgeSource
```

## Proposed Future Crates

```text
crates/
  mem-source/
  mem-source-fs/
  mem-source-git/
  mem-ingest/
  mem-docs/
  mem-code/
  mem-store-vector/
  mem-store-graph/
```

### `mem-source`

Defines source connector traits and shared source metadata.

Responsibilities:

- Source identity.
- Source scanning.
- Source reading.
- Content versioning.
- Content hashing.
- Source-level provenance.

Sketch:

```rust
#[async_trait::async_trait]
pub trait KnowledgeSource {
    async fn scan(&self) -> Result<Vec<SourceItem>>;
    async fn read(&self, id: &SourceId) -> Result<SourceDocument>;
}
```

### `mem-source-fs`

Reads local filesystem content.

Initial targets:

- Markdown.
- Plain text.
- JSON/YAML/TOML.
- Source files as raw text before code-aware parsing exists.

Later targets:

- HTML.
- PDF text extraction.
- Office document extraction.

### `mem-source-git`

Reads repositories with version-aware provenance.

Responsibilities:

- Repository root, remote, branch, and commit metadata.
- File snapshots.
- Diffs.
- Optional blame data.
- Incremental reindexing by changed files.

### `mem-ingest`

Normalizes source documents into indexable knowledge artifacts.

Responsibilities:

- Chunking.
- Metadata extraction.
- Deduplication.
- Content hashing.
- Incremental reindex planning.
- Provenance attachment.
- Emitting `KnowledgeChunk` records.

### `mem-docs`

Document-aware parsing and structure extraction.

Responsibilities:

- Markdown section hierarchy.
- Heading paths.
- Tables and code block extraction.
- HTML text extraction.
- Citation/link extraction.
- Document hierarchy and anchors.

### `mem-code`

Code-aware parsing and indexing.

Responsibilities:

- Symbol extraction.
- Imports and exports.
- Function/class/module boundaries.
- AST-aware chunks.
- Dependency edges.
- Call/reference edges where feasible.
- Language-specific parsers behind a shared interface.

Use Tree-sitter when code-aware indexing becomes necessary.

### `mem-store-vector`

Stores embeddings and performs similarity search for knowledge chunks and memory
records.

Responsibilities:

- Embedding metadata.
- Vector insert/update/delete.
- Similarity search.
- Granularity-aware filters.
- Adapter-specific persistence.

### `mem-store-graph`

Stores relationships extracted from code, documents, taxonomy, and memory.

Example edges:

- File contains symbol.
- Function calls function.
- Document section references concept.
- Memory derived from knowledge chunk.
- Knowledge chunk belongs to source document.
- Entity maps to taxonomy concept.

## Knowledge Model

Add a knowledge artifact model when this extension begins:

```text
KnowledgeSource
  -> SourceDocument
  -> KnowledgeChunk
  -> Embedding
  -> Entity / Relationship
  -> RetrievedKnowledge
```

Suggested core entities:

- `KnowledgeSource`: repository, folder, file set, URL, uploaded corpus.
- `SourceDocument`: one versioned source unit, such as a file or extracted
  document.
- `KnowledgeChunk`: retrievable section, code symbol, text span, or structured
  unit.
- `KnowledgeEntity`: extracted concept, symbol, person, project, API, or object.
- `KnowledgeRelationship`: typed edge between entities, chunks, documents, or
  taxonomy concepts.

## Retrieval Integration

The retrieval layer should route across memory and knowledge without merging
their storage concerns.

```text
Incoming query
  -> memory retrieval
       episodic events, semantic facts, preferences, prior task traces
  -> knowledge retrieval
       code chunks, symbols, docs, document sections, graph relationships
  -> ranking
  -> provenance annotation
  -> context composition
```

Retrieval explanations should distinguish memory from knowledge:

- Included because a prior successful task trace matched this cue.
- Included because this code symbol matched the query.
- Included because this document section defines the relevant API.
- Included because this chunk is linked to the retrieved entity.

## Boundary Rules

- Do not put parsing, chunking, embeddings, source scanning, or graph extraction
  in `mem-core`.
- Do not make TypeScript own source indexing if Rust owns the engine.
- Do not make vector search the only knowledge retrieval path.
- Do not treat source documents as memories unless the agent created or learned
  from them as part of an event.
- Store source and version provenance on every knowledge artifact.

## Implementation Sequence

1. Finish the first memory vertical slice:
   write, persist, retrieve, policy filter, rank, explain, evaluate.
2. Add `KnowledgeChunk` and source provenance contracts.
3. Add `mem-source` and `mem-ingest`.
4. Add `mem-source-fs` for local files.
5. Add document chunking for plain text and Markdown.
6. Add vector indexing behind `mem-store-vector`.
7. Add `mem-source-git` for repository-aware ingestion.
8. Add `mem-code` with Tree-sitter-backed symbol extraction.
9. Add `mem-store-graph` for code/doc/taxonomy relationships.
10. Add graph-aware retrieval and context composition.

## Open Questions

- Which source types are required first: filesystem, Git repositories, URLs, or
  uploaded corpora?
- Should knowledge chunks live in the same SQLite database initially or in a
  separate adapter?
- Which embedding provider should be the first adapter?
- How should incremental indexing be scheduled?
- How much code intelligence is required before Tree-sitter is worth adding?
- Should graph storage begin as SQLite edge tables before introducing a graph
  database adapter?

