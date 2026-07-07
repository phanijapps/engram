# Plan: ast-symbol-extraction (tree-sitter symbols + chunk entity refs)

Two improvements shipped in sequence: **Part A** (chunk entity-refs — quick win),
then **Part B** (tree-sitter for 10 languages + fallback). Per user confirmation:
coexist + fallback, symbols only (no AST call edges), chunk refs first.

## Tasks

### T1 — Chunk entity-ref population [Part A]
- **Tests (TDD):** `extractor.rs` — after `extract_into` on a code fixture with 2 symbols, the corresponding chunks have non-empty `entities`; a chunk with no symbols has empty entities.
- **Depends on:** none
- **Approach:** In `GraphExtractor::extract`, track which chunk each symbol came from. Currently `symbols: Vec<(String, EntityKind, String)>` loses the chunk index. Add a `chunk_index` field → `symbols: Vec<(String, EntityKind, String, usize)>`. After dedup + entity creation, for each entity, find its source chunk + stamp the entity ref onto the chunk's `entities` field. Return the updated chunks in the `ExtractedGraph` (or update them in the store via `repository.put_chunk`). The `KnowledgeChunk.entities` field already exists — this is population, not a type change.

### T2 — Tree-sitter chunker scaffolding [Part B]
- **Tests:** goal-based — cargo check; the crate compiles with tree-sitter deps.
- **Depends on:** T1 (so tree-sitter chunks also carry entity refs)
- **Approach:** Add `tree-sitter = "0.26"` + the 10 grammar crates to `engram-ingest` Cargo.toml as optional features (behind a `tree-sitter-langs` feature flag so the crate builds without them if needed). New module `adapters/ingest/src/tree_sitter_chunker.rs`: a `TreeSitterChunker` that selects the grammar by extension, parses the text into an AST, queries for declaration nodes (function/method/class/interface/struct/enum), and produces `ChunkCandidate`s with accurate anchors + line spans. The chunker implements the existing `Chunker` trait.

### T3 — Language grammars + fixture tests [Part B]
- **Tests (TDD):** per-language fixture — a minimal `.java` / `.kt` / `.cls` (Apex) / `.pl` / `.sh` / `.php` / `.cbl` / `.rs` / `.ts` / `.py` file with known declarations → assert the chunker produces chunks with the correct symbol names + kinds + line spans. One test per language.
- **Depends on:** T2
- **Approach:** For each language, write a tree-sitter query that matches declaration nodes:
  - Java/Kotlin/Apex: `method_declaration`, `class_declaration`, `interface_declaration`, `function_declaration`.
  - Perl: `sub_declaration`.
  - Bash: `function_definition`.
  - PHP: `function_definition`, `class_declaration`.
  - COBOL: `program`, `procedure`.
  - Rust: `function_item`, `struct_item`, `enum_item`, `trait_item`.
  - TypeScript: `function_declaration`, `class_declaration`, `interface_declaration`, `type_alias_declaration`.
  - Python: `function_definition`, `class_definition`.
  Each produces a `ChunkCandidate` with `anchor = "fn <name>"` / `"class <name>"` etc. + the node's line span.

### T4 — Scanner + ingestor wiring (fallback) [Part B]
- **Tests:** goal-based — cargo check/test; an unsupported extension falls through to CodeSymbolChunker.
- **Depends on:** T3
- **Approach:** The scanner's `scan_repository` currently constructs `KnowledgeIngestor::new(CodeSymbolChunker)` for code. Replace with a `CompositeChunker` that dispatches by extension: tree-sitter for supported, `CodeSymbolChunker` for others. The `CompositeChunker` implements `Chunker` + holds a `HashMap<&str, Box<dyn Chunker>>` (extension → grammar chunker) + a fallback. This keeps the ingestor unaware of the swap.

### T5 — Validate + lighter adversarial pass
- **Tests:** `cargo fmt --all && cargo check --workspace && cargo test -p engram-ingest`; rebuild native; backend typecheck/build/test; force re-index → chunks have entity refs + tree-sitter symbols.
- **Depends on:** T4

## Out of scope (logged)
- AST-level call-edge formation (tree-sitter queries for `call_expression` nodes — follow-up).
- Additional languages beyond the 10 listed.
- Changing the `Chunker` trait signature.
- LLM-enhanced extraction during the scan (stays per-doc on /ingest).
