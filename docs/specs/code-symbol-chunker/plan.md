# Plan: Code Symbol Chunker

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a deterministic line-oriented `CodeSymbolChunker` to `engram-ingest`. It
recognizes common declaration forms across Rust, TypeScript/JavaScript, Python,
Go, and JVM/C-like languages, emits symbol chunks with anchors and line spans,
and falls back to a file chunk when no declaration is found.

Tempted to add tree-sitter immediately; declining because AST parsing requires
language-specific dependency, grammar, and compatibility decisions. Tempted to
extract entities and relationships in the same slice; declining because symbol
relationship contracts are not specified.

## Constraints

- No public v1 schema changes.
- No parser dependencies in this slice.
- `lib.rs` remains a facade.
- Symbol detection stays deterministic and documented as declaration-oriented.

## Construction tests

**Unit tests:** code-symbol chunker tests for Rust, TypeScript, Python, and
fallback file chunks.

**Integration tests:** `KnowledgeIngestor` persists `CodeSymbol` chunks with
path and content hashes.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Implement the existing `Chunker` trait for `CodeSymbolChunker`. It emits
`ChunkCandidate` values only; stable IDs, source paths, policy, and provenance
are attached later by `KnowledgeIngestor`.

### Component / module decomposition

- `code_symbol.rs` owns declaration detection and symbol chunk boundaries.
- `lib.rs` re-exports public chunker types.
- Tests own language fixture snippets.

### Failure, edge cases & resilience

Empty input fails like other chunkers. No-symbol input returns one file chunk.
Nested declarations may become separate chunks if their declaration line matches
the detector; full AST nesting is future work.

## Tasks

### T1: Deterministic code-symbol chunker

**Depends on:** PHASE08 deterministic ingestion baseline.

**Tests:**
- Rust function/struct declarations emit `CodeSymbol` chunks with anchors.
- TypeScript and Python declarations emit stable anchors.
- No-symbol input returns one `File` chunk.
- `KnowledgeIngestor` persists code-symbol chunks with source paths.

**Approach:**
- Add `code_symbol.rs`.
- Keep declaration detection as small pure functions.
- Add unit and integration tests.

**Done when:** code-symbol tests and full repository gates pass.

## Rollout

Library code only. AST parsing, symbol graphs, and relationship extraction stay
future slices.

## Risks

- Line-oriented detection is intentionally incomplete; documentation must avoid
  promising full parser accuracy.

## Changelog

- 2026-06-30: initial plan for deterministic code-symbol chunking.
- 2026-06-30: shipped `CodeSymbolChunker` in `engram-ingest`.
