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
  │    ├─ Chunk             → CodeSymbolChunker or PlainTextChunker
  │    ├─ Persist           → KnowledgeSource, SourceDocument, KnowledgeChunk
  │    ├─ Extract entities  → GraphExtractor (symbols + concepts)
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
| **KnowledgeChunk** | A slice of the file's text (for retrieval) | `knowledge_chunks` table |
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

### CodeSymbolChunker (code files)

The code chunker is a **dependency-free line scanner**, not a tree-sitter parser.
It recognizes declaration starts in Rust, TypeScript/JavaScript, and Python by
matching line prefixes:

| Language | Patterns recognized |
|---|---|
| Rust | `fn`, `struct`, `enum`, `trait`, `impl` (strips `pub`, `async`, `unsafe`) |
| TypeScript/JS | `function`, `class`, `interface`, `enum`, `type` (strips `export`, `async`) |
| Python | `def`, `class` |

Each declaration becomes a chunk spanning from its line to the next declaration
(or end of file). The chunk carries the symbol text + line range + an anchor
string like `"fn remember"` or `"class MemoryRecord"`.

When no declaration is recognized, the entire file becomes one chunk so text is
never dropped silently.

### PlainTextChunker (text/markdown files)

A line-aware chunker that accumulates lines until the chunk reaches
`max_chars_per_chunk` (default 1,200), then starts a new chunk. Each chunk
carries its text + line span.

### What tree-sitter would add

The current chunker is **not** tree-sitter-based. It doesn't build an AST. It
recognizes declarations by line-prefix matching — fast and dependency-free, but
limited:

- **No import resolution** — it can't tell that `use crate::memory` in Rust
  connects to the `memory` module.
- **No scope awareness** — a local variable named `parse` and a function named
  `parse` are indistinguishable.
- **No type inference** — parameter types, return types, and generics are not
  parsed.
- **Language-agnostic extraction** — the same declaration patterns work across
  languages, but language-specific constructs (Go interfaces, Elixir macros, Lua
  metatables) are missed.

Tree-sitter would give real AST-level extraction: scoped symbol tables, import
graph edges, accurate call targets, type information. It's the biggest
improvement opportunity in the pipeline.

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

1. **Entity matching** — entities are ranked by how well their names match the
   question's terms (exact > prefix > substring), capped at 20.
2. **Call-graph expansion** — relationships whose endpoints are matched entities
   are included (capped at 30). This gives the LLM the local call graph.
3. **Chunk text** — chunks that reference matched entities (via entity refs) or
   whose text contains query terms are included (capped at 8, 1,200 chars each).
   This gives the LLM the actual code to explain.
4. **Memory + beliefs** — keyword retrieval over written memories + query-term
   matching over beliefs.
5. **LLM synthesis** — the pi SDK drives ollama cloud (`gemma4:31b-cloud`) with
   the assembled context. The system prompt instructs the model to trace call
   graphs, read code, and describe data flow — no LaTeX, plain arrows.

### Known limitation: chunk entity refs

The deterministic extractor stamps `source_refs` on entities but does **not**
populate `entities` on chunks — the reverse link is missing. So the Q&A's
chunk-entity-ref matching (`chunk.entities.includes(matchedEntityId)`) falls
back to text-term matching, which can surface documentation that mentions the
entity name rather than the code that implements it. Fixing this requires the
extractor to stamp entity IDs back onto chunks during extraction.

## Design choices and tradeoffs

### Why line-based chunking instead of tree-sitter?

The line-based scanner is zero-dependency, works across 30+ languages with the
same patterns, and runs fast on large repos. It's deliberately simple: get
symbols + their bodies into the graph, let the LLM do the deep semantic work.
Tree-sitter would add accuracy but also add per-language grammar dependencies,
slower scanning, and more maintenance surface. The current design prioritizes
breadth (any language, any repo) over depth (precise call analysis).

### Why co-occurrence for relationships?

True call-graph analysis requires AST-level parsing + cross-file resolution — a
much heavier pipeline. Co-occurrence is a pragmatic heuristic that catches the
common case (direct function calls) with zero language-specific logic. The
false-positive rate is acceptable for a Q&A-assisted demo where the LLM can
read the actual code and correct the graph's claims.

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

1. **Tree-sitter integration** — the single biggest win. Real AST-level symbol
   extraction with scoped names, import resolution, and accurate call targets.
   Would eliminate false positives in the call graph and enable cross-file
   edges.

2. **Chunk entity-ref population** — stamp entity IDs back onto chunks during
   extraction so Q&A can find the exact code that defines an entity, not just
   text that mentions it.

3. **Embeddings during indexing** — index chunk embeddings at ingest time so
   Q&A can do semantic chunk retrieval automatically (not just keyword +
   entity-name matching).

4. **Cross-file + cross-repo relationship edges** — the extractor works within
   one document. A post-extraction pass that resolves entity-name matches across
   documents (within a scan) would create cross-file call-graph edges.

5. **Value-stream / requirement / API-endpoint extraction** — the LLM extractor
   (pi SDK) currently uses a generic entity-kind set. Enriching it with
   domain-specific kinds (value stream, requirement, API endpoint) would let the
   graph capture `valuestream → requirement → code` chains.

6. **Agentic Q&A tool-use** — instead of pre-assembling a filtered context,
   give the LLM tools (`search_entities`, `get_neighbors`, `traverse`) to explore
   the graph step-by-step. The pi SDK supports custom tools via `ToolDefinition`
   + TypeBox schemas.

## See also

- [Scanner source](../../adapters/ingest/src/scanner.rs) — the parallel walk +
  filter + ingest implementation.
- [Extractor source](../../adapters/ingest/src/extractor.rs) — entity extraction
  + co-occurrence relationship formation.
- [Code symbol chunker](../../adapters/ingest/src/code_symbol.rs) — the
  declaration-pattern scanner.
- [Q&A logic](../../demo/backend/src/qa.ts) — entity ranking, chunk grounding,
  context assembly.
- [Background repo indexer spec](../../docs/specs/background-repo-indexer/spec.md)
  — the spec that introduced the Rust parallel scanner.
- [ADR-0007](../../docs/adr/0007-napi-binding-surface-extension.md) — the N-API
  binding surface decision.
