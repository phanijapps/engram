# Spec: ast-symbol-extraction (tree-sitter symbols + chunk entity refs)

- **Status:** Draft
- **Shape:** data
- **Constrained by:** AGENTS.md (Rust owns deterministic behavior; `engram-ingest` is the module); the existing `CodeSymbolChunker` + `GraphExtractor` contracts; no domain-type change (KnowledgeChunk already has `entities: Vec<EntityRef>`)
- **Contract:** none (internal pipeline change)

## Objective

Two improvements to the indexing pipeline, shipped in sequence:

**Part A — Chunk entity-ref population.** After extraction, every `KnowledgeChunk` carries the entity refs of the symbols extracted from it. The extractor already knows which chunk each symbol came from (it iterates chunks sequentially) but discards the mapping during dedup. Stamping the entity IDs back onto chunks means the Q&A finds the exact code that defines an entity, not just text that mentions the name.

**Part B — Tree-sitter AST symbol extraction.** The `CodeSymbolChunker` is replaced by a tree-sitter-backed chunker for 10 languages (Java, Kotlin, Salesforce Apex, Perl, Bash, PHP, COBOL, Rust, TypeScript, Python). Tree-sitter gives AST-level declaration detection: scoped names, accurate spans, correct kind classification (method vs. function vs. class), and symbol bodies that don't bleed into the next declaration. The line-based `CodeSymbolChunker` stays as a fallback for languages without a grammar. Relationship formation stays co-occurrence-based (tree-sitter improves symbols, not call edges — yet).

## Decision

**Coexist + fallback.** Tree-sitter for the 10 supported languages; the existing line-based `CodeSymbolChunker` as fallback for any other extension. No regression on unsupported languages.

**Symbols only.** Tree-sitter drives symbol extraction (declarations → chunks with accurate anchors + spans). Relationship edges stay co-occurrence-based (`mentions()` heuristic). AST-level call-edge formation is a follow-up.

**Chunk refs first.** Part A ships standalone (small, immediate Q&A quality win). Part B (tree-sitter) is the bigger follow-up.

Languages: Java (`tree-sitter-java = "0.23.5"`), Kotlin (`tree-sitter-kotlin = "0.3.8"`), Apex (`tree-sitter-apex = "1.0.0"`), Perl (`tree-sitter-perl = "1.1.2"`), Bash (`tree-sitter-bash = "0.25.1"`), PHP (`tree-sitter-php = "0.24.2"`), COBOL (`tree-sitter-cobol = "0.1.0"`), Rust (`tree-sitter-rust = "0.24.2"`), TypeScript (`tree-sitter-typescript = "0.23.2"`), Python (`tree-sitter-python = "0.25.0"`).

## Assumptions

- Technical: `KnowledgeChunk.entities: Vec<EntityRef>` is currently always empty — the extractor never stamps entity IDs back onto chunks. (`extractor.rs`)
- Technical: The extractor iterates chunks sequentially and builds `symbols: Vec<(name, kind, chunk_text)>` — it knows which chunk each symbol came from but discards the mapping during name dedup. (`extractor.rs:65-89`)
- Technical: Tree-sitter grammar crates exist for all 10 target languages on crates.io. (`cargo search` — versions cited above)
- Technical: `CodeSymbolChunker` is a dependency-free line scanner recognizing Rust/TS/Python declaration patterns. (`code_symbol.rs`)
- Technical: The `Chunker` trait is `fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>>` — a tree-sitter chunker implements the same trait. (`chunker.rs:24`)
- Product: Coexist + fallback; symbols only (no AST call edges); chunk refs first, then tree-sitter. (user confirmation 2026-06-30)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- Keep the existing `CodeSymbolChunker` as a fallback for unsupported extensions.
- Implement the tree-sitter chunker behind the same `Chunker` trait so the ingestor is unaware of the swap.
- Part A (chunk entity-refs) must not change the `KnowledgeChunk` domain type — the `entities` field already exists.
- Keep the `mentions()` co-occurrence logic for relationship formation (no AST call-edge change in this spec).

**Ask first**
- AST-level call-edge formation (tree-sitter queries for `call_expression` / `method_invocation` nodes).
- Supporting additional languages beyond the 10 listed.
- Changing the `Chunker` trait signature.

**Never do**
- Change Rust domain types or v1 contracts.
- Break the existing line-based chunker for unsupported languages.
- Make tree-sitter a hard dependency (the demo must build without it for unsupported extensions).

## Testing Strategy

- **TDD (unit, Rust):** Part A — `extractor.rs`: after `extract_into`, the returned chunks have non-empty `entities` for code documents with recognized symbols (and empty for files with no symbols). Part B — `tree_sitter_chunker.rs`: each language fixture (a minimal `.java`, `.kt`, `.cls`, `.pl`, `.sh`, `.php`, `.cbl`, `.rs`, `.ts`, `.py` file with known declarations) produces chunks with correct anchors + spans. Fallback: an unknown extension (`.vim`) falls through to the line-based chunker.
- **Goal-based (build):** `cargo fmt --all && cargo check --workspace && cargo test -p engram-ingest`; rebuild native binding; backend typecheck/build/test.
- **Goal-based (plumbing):** after re-indexing with force, a chunk's `entities` field is non-empty → Q&A chunk-entity-ref matching finds the actual code (not docs that mention the name).

## Acceptance Criteria

**Part A:**
- [ ] After `extract_into`, chunks returned (or updated in the store) carry the entity refs of the symbols extracted from them.
- [ ] Unit test: a code fixture with 2 symbols → the corresponding chunks have entity refs; a chunk with no symbols has empty entities.
- [ ] Q&A chunk-entity-ref matching finds the actual code (entity-ref path fires, not just text-term fallback).

**Part B:**
- [ ] A `TreeSitterChunker` implements `Chunker` for Java, Kotlin, Apex, Perl, Bash, PHP, COBOL, Rust, TypeScript, Python — each with a grammar fixture test asserting correct anchors + spans.
- [ ] The scanner + ingestor use `TreeSitterChunker` for supported extensions and fall back to `CodeSymbolChunker` for others.
- [ ] No regression: files in unsupported languages still chunk via the line-based scanner.
- [ ] `cargo fmt/check/test --workspace` green; rebuild native binding; demo re-index produces entities with accurate symbol boundaries.
