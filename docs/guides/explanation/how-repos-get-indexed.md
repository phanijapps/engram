# About How Code Repositories Get Indexed

> Why the indexing pipeline works the way it does. This page is for readers who
> want a mental model of the full pipeline — not to run a scan (see
> [Index a folder or repo](../../demo/frontend/src/routes/repo-index.tsx)) or
> look up parameters. For the scanner's security controls, see
> [the scanner source](../../adapters/ingest/src/scanner.rs).

## The question this page answers

When you point engram at a code repository, what actually happens? How do 12,000
files become entities, relationships, and answerable questions? This page walks
through the pipeline end to end — the data model, the chunking strategy, how
relationships form, what embedding models are involved, and where the honest
limits are.

## The pipeline at a glance

```
repo root
  │
  ├─ 1. Git metadata        → remote URL, branch, SHA
  ├─ 2. Walk (.gitignore)   → ignore crate (ripgrep's walker)
  ├─ 3. Security filter     → path confinement, secret blocklist, size bound
  ├─ 4. Classification       → code vs. text (by extension)
  │
  ├─ 5. Parallel ingest      → rayon (one core per file)
  │    ├─ Chunk             → TreeSitterChunker (8 langs) or CodeSymbolChunker (fallback)
  │    ├─ Persist           → KnowledgeSource, SourceDocument, KnowledgeChunk
  │    ├─ Extract entities  → GraphExtractor (symbols + concepts)
  │    ├─ Stamp chunk refs  → entity IDs written back onto chunks
  │    ├─ Extract edges     → co-occurrence → "calls" / "mentions"
  │    └─ Persist graph     → KnowledgeEntity, KnowledgeRelationship
  │
  └─ 6. Summary             → counts + git metadata + manifest
```

Steps 1–4 are sequential (one walk). Step 5 fans out across CPU cores via
`rayon`. The entire pipeline runs on a background thread; the HTTP request
returns immediately with a job ID, and the UI polls for progress.

## The data model

Every file that survives the walk becomes a chain of records:

| Record | What it holds | Where it lives |
|---|---|---|
| **KnowledgeSource** | The repo itself (name, scope, git provenance) | `knowledge_sources` table |
| **SourceDocument** | One file (path, kind, content hash) | `knowledge_documents` table |
| **KnowledgeChunk** | A slice of the file's text (for retrieval + Q&A) | `knowledge_chunks` table |
| **KnowledgeGraph** | A named graph boundary for the document's entities | `knowledge_graphs` table |
| **KnowledgeEntity** | A function, class, concept, etc. (name, kind, source file path) | `knowledge_entities` table |
| **KnowledgeRelationship** | An edge: entity A `calls` entity B | `knowledge_relationships` table |

All records are stored as contract JSON in SQLite with scope columns for
tenant/workspace/environment filtering. WAL mode enables concurrent reads during
writes (so the UI can poll `/knowledge/overview` while the scan runs).

### Entity provenance

Every entity carries a `provenance` field whose `source` is enriched with git
metadata:

```
scan:agentzero [git@github.com:user/repo@main:abc123de]
```

This flows to Q&A citations so answers name the repo, branch, and commit.

### Source file paths

Each entity carries a `source_refs` array with an `EvidenceRef` pointing back to
the `SourceDocument` path (e.g., `src/intent.rs`). This is how the Q&A can cite
*which file* an entity came from, not just *which repo*.

### Chunk entity refs

After extraction, each chunk carries the entity refs of the symbols extracted
from it (`chunk.entities`). This reverse link lets the Q&A find the exact code
that defines an entity — not just text that mentions the entity's name.

## How files are walked

The scanner uses the [`ignore`](https://docs.rs/ignore) crate — the same walker
ripgrep uses. It respects:

- `.gitignore` files (project, parent, and global)
- `.git/` directories (never descended)
- Hidden files (dotfiles — skipped by default)
- Symlinks (`follow_links(false)` — never followed, preventing escape)

After the walk, each file passes through four filters before it's ingested:

1. **Path confinement** — `std::fs::canonicalize` + `starts_with(root)`. Rejects
   `..` traversal and symlink escapes.
2. **Secret blocklist** — `.env`, `*.key`, `*.pem`, `*.cert`, `id_rsa`, etc.
   Skipped by name; contents never read. `.env.example` / `.env.sample` are
   allowed (they document variables without secrets).
3. **Deny list** — `node_modules`, `target`, `dist`, `__pycache__`, `.venv`,
   etc. + file extensions `.db`, `.sqlite`, `.lock`, `.log`, `.pyc`.
4. **Size bound** — 1 MiB per file (configurable). Oversized files are skipped.

## How code is chunked

The scanner dispatches chunking by file extension:

- **TreeSitterChunker** for 8 supported languages (14 extensions).
- **CodeSymbolChunker** (line-based fallback) for everything else.
- **PlainTextChunker** for text/markdown files.

### TreeSitterChunker (8 languages, AST-level)

Uses tree-sitter to build an AST and walk for declaration nodes. This gives
accurate symbol spans (no bleeding into the next declaration), correct kind
classification, and scoped names.

| Language | Extensions | Declarations detected |
|---|---|---|
| Rust | `.rs` | `fn`, `struct`, `enum`, `trait` |
| TypeScript / JS | `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs` | `function`, `class`, `interface`, `type`, `method` |
| Python | `.py` | `def`, `class` |
| Java | `.java` | `method`, `class`, `interface`, `constructor` |
| Kotlin | `.kt`, `.kts` | `function`, `class`, `object` |
| Salesforce Apex | `.cls`, `.apex`, `.trigger` | `method`, `class`, `interface` |
| Perl | `.pl`, `.pm` | `sub` |
| Bash | `.sh`, `.bash` | `function` |
| PHP | `.php` | `function`, `class`, `method` |

Each declaration becomes a `ChunkCandidate` with the node's text, line span,
and an anchor like `"fn alpha"` or `"class Widget"`. When no declaration is
found, the entire file becomes one chunk so text is never dropped silently.

### CodeSymbolChunker (fallback)

A dependency-free line scanner that recognizes declaration starts by line-prefix
matching. Covers 30+ language extensions (Go, C/C++, Scala, Elixir, Lua, etc.)
with the same pattern set. Used for any code file the TreeSitterChunker doesn't
support.

### PlainTextChunker (text/markdown)

A line-aware chunker that accumulates lines until the chunk reaches
`max_chars_per_chunk` (default 1,200), then starts a new chunk. Each chunk
carries its text + line span.

## How relationships form

The `GraphExtractor` builds edges from **name co-occurrence** within symbol
bodies. After chunking, each symbol has a name + a body (the text between its
declaration and the next). The extractor:

1. Collects all unique symbol names in the document.
2. For each symbol, checks whether any other symbol's name appears as a
   word-boundary match in the body text.
3. If it does, creates a relationship with predicate `"calls"` (for code) or
   `"mentions"` (for prose).

The word-boundary check (`mentions`) ensures `File` doesn't match inside
`Filesystem` — it requires the name to be surrounded by non-identifier
characters.

### What this misses

Co-occurrence is a heuristic, not a call-graph analysis:

- **False positives** — a comment mentioning `tokenize` creates a `calls` edge
  even if the code doesn't actually call it.
- **False negatives** — indirect calls (through function pointers, trait
  objects, dynamic dispatch) are invisible.
- **No cross-file edges** — the extractor works within a single document. A
  function in `a.rs` that calls a function defined in `b.rs` only gets an edge
  if the callee's name appears in the caller's body (which it does for direct
  calls, but not for re-exports or aliased imports).

Cross-file and cross-repo linking (how different repos connect) is done at Q&A
time: entities with the same name across different graphs are linked by the
explorer + the Q&A grounding logic.

## Embedding models

Embeddings are **not** part of the indexing pipeline. The ingestor deliberately
leaves `embedding_refs` empty on every chunk — vectorization belongs to a later
adapter stage.

When you use the retrieval panel (`/chat` → Context composer tab), the
`NativeRetrievalEngine` indexes chunk text with **FastEmbed BGE-small** (a
passage embedding model) into an in-memory sqlite-vec index. This is separate
from the scan — it must be triggered manually by indexing a corpus in the
Context composer.

Bringing embeddings into the scan pipeline (so Q&A can do semantic chunk
retrieval automatically, not just keyword + entity-name matching) is a
significant improvement opportunity.

## How the Q&A uses the indexed data

When you ask a question in `/chat`:

1. **Pre-fetch** — entities, relationships, and chunks are loaded once from
   the knowledge store (scope-filtered).
2. **Agentic loop** (when LLM creds present) — the LLM gets 3 tools and
   explores the graph itself:
   - `search_entities(query)` — find entities by name keyword
   - `get_neighbors(entity)` — trace relationships (calls/mentions/defines)
   - `get_code(entity)` — read the source code text
   The LLM decides what to search, follows call chains across hops, reads
   code, and synthesizes a final answer. Max 9 turns. The pi SDK session
   persists across turns (multi-turn conversation).
3. **Evidence fallback** (no creds) — a pre-filtered evidence summary: entities
   ranked by name match quality (exact > prefix > substring, capped at 20) +
   their call-graph relationships (capped at 30) + chunk text (capped at 8).

## Design choices and tradeoffs

### Why tree-sitter + line-based fallback?

Tree-sitter gives AST-level accuracy for the 8 most common languages (correct
spans, no bleeding between declarations). The line-based `CodeSymbolChunker`
covers 30+ other languages where a tree-sitter grammar isn't available. The
scanner dispatches by extension at runtime — no configuration needed.

### Why co-occurrence for relationships?

True call-graph analysis requires AST-level call-expression queries + cross-file
resolution — a much heavier pipeline. Co-occurrence is a pragmatic heuristic
that catches the common case (direct function calls) with zero language-specific
logic. The false-positive rate is acceptable for a Q&A-assisted demo where the
LLM can read the actual code and correct the graph's claims.

### Why no embeddings during indexing?

Embedding every chunk at ingest time would add latency + a model dependency
(BGE-small) to the scan pipeline. The current design separates concerns:
indexing is deterministic + fast (symbols + text), retrieval is opt-in (the
Context composer). Bringing embeddings into the scan is a natural next step but
would change the scan's performance characteristics.

### Why WAL + busy_timeout?

The rayon scan writes through one SQLite connection while the UI polls reads
through a different connection to the same file. Without WAL, SQLite's rollback
journal blocks readers during writes. WAL lets readers + the writer coexist;
`busy_timeout` (5 s) makes a contended connection wait instead of failing.

## Areas for improvement

1. **AST-level call edges** — tree-sitter now drives symbol extraction, but
   relationship edges still use co-occurrence. Querying `call_expression` /
   `method_invocation` nodes would give accurate call targets + eliminate false
   positives.

2. **Embeddings during indexing** — index chunk embeddings at ingest time so
   Q&A can do semantic chunk retrieval automatically (not just keyword +
   entity-name matching).

3. **Cross-file + cross-repo relationship edges** — the extractor works within
   one document. A post-extraction pass that resolves entity-name matches across
   documents (within a scan) would create cross-file call-graph edges.

4. **COBOL grammar** — `tree-sitter-cobol` (0.1.0) has no Rust lib target.
   A compatible grammar crate needs to be found or built.

## See also

- [Scanner source](../../adapters/ingest/src/scanner.rs) — the parallel walk +
  filter + ingest implementation.
- [Tree-sitter chunker](../../adapters/ingest/src/tree_sitter_chunker.rs) — the
  AST-level symbol extraction for 8 languages.
- [Code symbol chunker](../../adapters/ingest/src/code_symbol.rs) — the
  line-based fallback for unsupported extensions.
- [Extractor source](../../adapters/ingest/src/extractor.rs) — entity extraction,
  co-occurrence relationship formation, + chunk entity-ref stamping.
- [Q&A logic](../../demo/backend/src/qa.ts) — entity ranking, chunk grounding,
  context assembly.
- [Background repo indexer spec](../../docs/specs/background-repo-indexer/spec.md)
  — the spec that introduced the Rust parallel scanner.
- [AST symbol extraction spec](../../docs/specs/ast-symbol-extraction/spec.md) —
  the spec that introduced tree-sitter + chunk entity-refs.
- [ADR-0007](../../docs/adr/0007-napi-binding-surface-extension.md) — the N-API
  binding surface decision.
