# AST-aware code chunking audit

Date: 2026-10-14. Audited checkout: `9632c63` (`exp_develop`).
Status: verified findings and proposed changes; no runtime implementation change.

## Verdict

**Engram already uses Tree-sitter to produce AST-aware code chunks on the
repository scan path for supported extensions.** Leaf functions and methods
retain their source bodies. This is more than AST extraction for graph nodes:
the resulting candidates are persisted as `KnowledgeChunk` records, and the
MCP embedding path reads those records.

It does **not** yet provide the complete context-enriched, lossless chunking
and plain-English Silver pipeline described in the supplied examples. A live
fixture reproduced loss of outer-function statements when a nested function
exists. Class member declarations can also fall outside the emitted chunks.
Chunk context needs assembly from the graph, and automatic code summaries
are absent from the scan path.

## Reference synthesis

Three Luna agents separately synthesized the supplied material, traced the
chunk pipeline, and inspected summary/context capabilities. Inputs were the
user's TypeScript example, the clipboard AST image, and
`/home/videogamer/Downloads/interactive_visual_im_912c85340efdada7.html`.

The HTML illustrates source → parser → declaration traversal → method chunks
→ embedding-ready output. It calls for signatures, class scope and import
context, but its output JSON only demonstrates method text and parent class.
It discovers the constructor without showing it in the final output. The
clipboard image illustrates a control-flow tree, rather than method chunks.
Neither artifact demonstrates persistence, retrieval, embedding coverage or
an implemented Silver summary layer.

The useful requirement is: retrieve complete behavior with traceable source
and enough enclosing/dependency context to explain it. Import declarations
are context, not proof of which dependencies a method uses or how an external
database method behaves. Code establishes observed implementation behavior;
an inferred product requirement still needs to be identified as inference.

## What is implemented

| Concern | Evidence | Assessment |
| --- | --- | --- |
| Parser and declaration boundaries | [`tree_sitter_chunker.rs`](../../core/code/src/parser/tree_sitter_chunker.rs), `TreeSitterChunker::parse`, `walk_declarations_with_receivers`, `ts_kinds` | Supported repository code uses syntax nodes, not fixed character windows. |
| Method body and parent identity | Same module, leaf branch and qualified anchor construction | A leaf method keeps its complete body; `fn UserService::getUser` encodes its enclosing class. |
| Container chunks | Same module, `has_declaration_descendant` and `signature_line` | Any declaration with a declaration descendant gets a doc comment plus first source line. This rule also applies to functions, and the first line is not necessarily a complete multiline signature. |
| Real storage | [`scanner/mod.rs`](../../adapters/ingest/src/scanner/mod.rs), shared-tree ingestion; [`ingestor.rs`](../../adapters/ingest/src/ingestor.rs), chunk assembly | AST candidates become stored `KnowledgeChunk`s with location, hash, provenance and entity links. `summary` and `metadata` are unset. |
| Hierarchy and imports | [`edges.rs`](../../core/code/src/edges.rs); [`extractor.rs`](../../adapters/ingest/src/extractor.rs), structural extraction | `contains` and File→Module `imports` edges exist. Imports record module paths; this does not supply imported identifier/type bindings to each method. |
| Embedding | [`codegraph.rs`](../../mcp/engram-mcp/src/codegraph.rs), `embed_pending` | Nonempty stored chunks are eligible when embedding/vector capabilities are wired. Model input is capped to 8,192 **characters**, although stored source is not clipped. |
| Lexical search | Same module, `scan_repo` lexical delta feed | This feed indexes entity names and kinds, rather than method implementation text and comments. |
| Context query | Same module, `symbol_context` | Returns callers/callees/community. It does not hydrate parent fields, imports, constructor source or a semantic summary. |
| Summaries | [`knowledge.rs`](../../core/domain/src/knowledge.rs), `KnowledgeChunk.summary`; [`extract_knowledge.ts`](../../packages/runtime/src/maintenance/extract_knowledge.ts) | The summary field exists. TS maintenance has reusable LLM concept extraction, but scan does not generate method behavior summaries. |

The repository has no implemented Bronze/Silver/Gold contract. Those names
below are a proposed mapping, not current Engram capabilities. Existing SDLC
ontology and agent-side distillation are reusable foundations:
[`sdlc-ontology.json`](../../mcp/engram-mcp/examples/sdlc-ontology.json) and
[`engram-distill`](../../mcp/engram-mcp/extensions/engram-distill/SKILL.md).

## Live verification

Used the real stdio server through `scripts/dev/mcp_driver.py`, with vector
support disabled. The dedicated self store was retained, incrementally
rescanned, and queried with `code` architecture, health, changed, context,
search and impact operations. Self scan: 1,220 entries scanned, six ingested,
950 unchanged, zero errors. Architecture reported 3,162 nodes and 7,192 edges.
Engram search located `TreeSitterChunker` and its declaration walker; source
inspection verified the behavior rather than treating graph output as proof.

A separate temporary store indexed the exact supplied `UserService` source
plus a nested-function fixture and an import-only fixture. It ingested three
files with zero errors. Read-only inspection of persisted records showed:

| Chunk anchor | Source lines | Stored content |
| --- | --- | --- |
| `class UserService` | 3–19 | `export class UserService {` only |
| `fn UserService::constructor` | 6–8 | Complete constructor |
| `fn UserService::getUser` | 10–13 | Complete method, comment and SELECT statement |
| `fn UserService::deleteUser` | 15–18 | Complete method, comment and DELETE statement |

These chunks had entity links and source locations, but no populated summary
or metadata. The graph contained class→method `contains` edges and a
`user.service.ts`→`./db` import edge. Code search returned
`UserService::getUser`. Method context returned no callers or callees for the
external `db.query` call; this cannot establish the database implementation's
behavior. The class field `private db: Database;` was absent from chunk text.

The nested-function reproduction was:

```typescript
export function checkout(total: number) {
  const fee = 5;
  function validate(value: number) { return value > 0; }
  if (!validate(total)) throw new Error('invalid');
  return total + fee;
}
```

Its only stored texts were the outer signature and the complete inner
`validate` function. `const fee`, the error branch and `return total + fee`
were absent from **all** persisted chunks for this file. A location covering
lines 1–6 did not make the outer chunk complete. The import-only fixture was
stored as a whole-file `CodeBlock`, preserving both import and statement.

Existing adapter chunker tests passed: 11/11 via
`cargo test -p engram-ingest --test tree_sitter_chunker`. These tests do not
cover the demonstrated outer-function loss. No semantic vector recall or
LLM summary generation was exercised; embedding conclusions come from source
inspection. Temporary evidence lives under `/tmp/engram-ast-*`, not as durable
repository test fixtures.

## Changes to make, in order

1. **Preserve executable and contextual source before enrichment.** Restrict
   signature-only treatment to appropriate structural containers. Functions
   and methods containing nested declarations must keep the rest of their
   behavior. Preserve class fields, initialization blocks, decorators and
   relevant top-level statements through explicit context/residual chunks or
   a retrievable source snapshot. Keep AST ranges accurate and distinguish a
   container's full span from its abbreviated stored text. Acceptance:
   `checkout` retains its validation, error and fee calculation; `UserService`
   context exposes the field and constructor without losing either method.

2. **Define context hydration over existing graph and source facts.** Assemble
   a bounded method context with qualified symbol, complete signature, parent
   declaration, relevant member/constructor context, import bindings and
   file/revision/ranges. Reuse `contains`, `imports` and chunk entity links;
   do not duplicate graph truth in arbitrary metadata. Preserve unresolved
   dependencies explicitly. A new typed public context contract, if required,
   must start in the domain model and generated contracts. Acceptance: the
   example supplies `UserService`, `db: Database` and the `./db` binding when
   retrieving either method.

3. **Make large-code and parse-quality handling explicit.** The current
   8,192-character embedding prefix can miss later behavior. Introduce a
   model-aware token budget and AST-boundary segments for oversized symbols,
   linking segments to a complete source unit and its parent context. Trees
   with syntax errors are currently accepted without checking `has_error()`;
   track parse quality, preserve recovery/fallback coverage and expose it.
   Unsupported extensions use the legacy line-based chunker. Acceptance:
   late-method behavior is represented in indexed segments; every segment is
   marked as partial with source ranges; malformed/unsupported inputs do not
   masquerade as complete AST extraction.

4. **Add grounded Silver behavior summaries in the TS integration layer.**
   Reuse the injected `LlmProvider` and structured-output validation patterns
   from maintenance. Summarize selected source/context units, describing
   inputs, outputs, effects and explicit conditions. Link every claim to
   symbol/chunk IDs and revision; record model/prompt provenance and refresh
   derived results when source changes. Keep source authoritative and retain
   failure state without replacing source with guessed output. Reuse the
   existing `summary` field only after specifying write/provenance semantics;
   richer structured behavior may need a separate derived artifact. Keep
   Rust parsing deterministic and LLM-free. Acceptance: a deterministic stub
   exercises persistence/invalidation; a real configured LLM produces a
   traceable explanation of get/delete behavior without inventing business
   intent or external database guarantees.

5. **Retrieve explanations and source together.** Wire code implementation
   text/comments and derived summaries into appropriate retrieval adapters,
   rather than relying on the scan's entity-name lexical feed. Reuse retrieval
   composition and fusion. Exercise queries such as “retrieve a user by ID”
   and “delete a user,” returning summary, symbol, source and context. Any new
   runtime operation must reach Rust integration, N-API/TS and MCP, with honest
   capability reporting. Acceptance includes semantic and lexical retrieval,
   source changes, missing summary/vector lanes and a method whose decisive
   behavior is near its end.

Proposed medallion mapping: **Bronze** = revision-grounded source plus
deterministic AST chunks/graph; **Silver** = evidence-linked derived behavior
summaries; **Gold** = PM-facing feature/process/requirement views over those
summaries and graph links. Gold should distinguish observed behavior from
inferred requirements. An LLM reading one method cannot establish an entire
business process without supporting callers, dependencies and requirements.

These are scoped follow-up slices, not an accepted architecture decision or
an implementation spec. Start with source preservation; adding summaries
before that fix would make explanations confidently incomplete.

Accepted design: [RFC-0021: Structural code context](../rfcs/0021-structural-code-context.md),
including future language/parser registration and polyglot validation. This
approval does not change the audit's measured runtime behavior.
