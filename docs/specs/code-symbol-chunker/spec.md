# Spec: Code Symbol Chunker

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can split source-code documents into deterministic symbol-oriented
knowledge chunks that preserve line spans and symbol anchors without adding AST
parser dependencies or changing public domain contracts.

## Boundaries

### Always do

- Emit `KnowledgeChunkKind::CodeSymbol` for recognizable declarations.
- Preserve start line, end line, and anchor on each symbol candidate.
- Return a deterministic file-level chunk when no declarations are recognized.
- Compose with `KnowledgeIngestor` without changing repository behavior.

### Ask first

- Add tree-sitter, gitoxide, language servers, or other parser dependencies.
- Extract call graphs, imports, references, or type relationships.
- Add new public domain fields for symbols.

### Never do

- Claim full language parsing.
- Drop source text when no symbol is recognized.
- Treat code chunks as memory records.
- Hide parser uncertainty inside metadata.

## Testing Strategy

- TDD: chunker tests assert recognized Rust, TypeScript, and Python declaration
  anchors and line spans.
- TDD: integration with `KnowledgeIngestor` asserts code-symbol chunks keep
  source path, content hashes, and no embeddings.
- Goal-based: full repository gates prove no v1 contract drift.

## Acceptance Criteria

- [x] Recognized declarations produce `CodeSymbol` chunks with stable anchors.
- [x] Symbol chunks preserve source line ranges.
- [x] No-symbol code produces one `File` chunk instead of empty output.
- [x] `KnowledgeIngestor` persists code-symbol chunks with source path and
  content hashes intact.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: code-aware chunks are represented by existing
  `KnowledgeChunkKind::CodeSymbol` and `SourceLocation.anchor` fields (source:
  `crates/engram-domain/src/knowledge.rs`).
- Technical: ingestion owns chunk candidates while IDs/provenance are attached
  by `KnowledgeIngestor` (source: `crates/engram-ingest/src/ingestor.rs`).
- Process: public contracts do not change for this chunker slice (source:
  ADR-0004).
