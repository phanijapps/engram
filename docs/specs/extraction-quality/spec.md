# Spec: extraction-quality

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0020 (Accepted), RFC-0014 (canonical identity), RFC-0018 (retrieval quality), ADR-0022 (surface parity)
- **Brief:** none
- **Contract:** none — no new public domain contract or MCP tool; entity `name` is an existing domain field, `extract-knowledge` is surfaced through the existing `maintenance_run` tool, and `listChunksByDocument` extends the knowledge repository port + `@engram/node` transport (internal surface only)
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Code entities in the knowledge graph carry their **bare logical name** as `entity.name`
(the function/class/etc symbol, e.g. `makeRow`) and pass a code-tuned noise filter, so
code-intel queries (`architecture`, `symbol_context`, `change_impact`) return real
symbols instead of generic `new`/`clone`/`len` hubs. The repo/path/branch that
*disambiguate* a symbol are **not** jammed into the name — they live in `source_refs`
+ provenance/metadata. The git `branch`/`revision` are stamped as clean metadata keys
on each entity (provenance-only; one logical entity per function regardless of branch,
re-index updates the branch). Because the community graph used to key nodes by name
(which collapsed two functions that share a logical name — two `render`, two
`index.ts` — into one node), it now **keys nodes by entity `id`** (stable, unique per
`graph_id + name`); the display layer still shows the logical name, disambiguated by
id + path. A suffix/alias resolution step restores `symbol_context`/`change_impact`/
`search` for user-supplied bare names on the TS HTTP MCP and the Rust stdio MCP alike
(ADR-0022). Unstructured documents yield a separate **knowledge sub-graph** of valid
concepts, properties, and relationships via a new LLM `extract-knowledge` maintenance
op (replacing the naive heading-as-node rule), queryable independently of code topology
and consolidated across documents by canonical identity (RFC-0014).

This is RFC-0020 Phase 1. The code-symbol noise filter is already shipped; this spec
covers the remaining Phase 1 work: code-entity logical-name identity (with cross-file
call resolution preserved and query-tool matching restored on both surfaces),
community nodes keyed by entity id, git branch/revision as clean provenance metadata,
document `concept_name` removal, per-document chunk reads, and the `extract-knowledge`
LLM op.

## Boundaries

### Always do

- Suppress bare-generic + primitive symbols at extraction (code path via `is_noise_symbol`; concept path via `is_noise_concept`, ported to TS for `extract-knowledge`).
- Emit code-entity `name` as the **bare logical symbol** (`{bare_name}`); the repo/path/branch disambiguators live in `source_refs` + provenance/metadata, NOT in the name. The entity `id` (derived from `graph_id + name`) stays unique.
- Stamp clean git provenance keys `repository` (remote), `branch`, `revision` (SHA) on each code entity's metadata from the `detect_git` tuple — in addition to the enriched `source_name` + `STABLE_SOURCE_KEY`. Branch is provenance-only (re-index updates it; one logical entity per function).
- Key community-graph nodes by entity **id** (not name) so same-named functions stay distinct nodes; the display layer maps ids→logical names and disambiguates by id + path.
- Resolve user-supplied bare symbols against entities via suffix/exact match in every query tool that takes a symbol argument, on BOTH the TS HTTP MCP and the Rust stdio MCP.
- Run `extract-knowledge` as an idempotent, convergent op keyed on `(scope discriminator, canonical concept label)` with `graph_id = None`, so the same concept extracted from two documents converges to one entity (RFC-0014).
- Keep all LLM work behind `engram-maintain` (TS). The Rust core (`engram-domain`, the ingest extractor) stays LLM- and NLP-free.
- Read chunks **per-document** (`listGraphs` + `listChunksByDocument`) — one LLM call per document; never load the full scope-wide chunk set.

### Ask first

- Folding repo/path/branch back into `entity.name` (qualified identity was reverted; the name stays the bare logical symbol).
- Adding a new MCP tool — prefer extending `maintenance_run` with a new `op`.
- Any change to the cross-file call-resolution index semantics (extractor `name_index`).

### Never do

- Reintroduce the naive heading-as-node `concept_name` entity emission for documents.
- Introduce NLP or LLM dependencies in the Rust core (`engram-domain`, `engram-ingest` extractor).
- Add new store schema columns for provenance (use `record_json` metadata).
- Break cross-file `calls` edge resolution — `object.id` must still populate.
- Jam repo/path/branch into `entity.name` (qualified identity) — the name is the bare logical symbol; disambiguators go in `source_refs`/provenance/metadata.
- Key community nodes by name — two functions that share a logical name would collapse; key by entity id.
- Ship the suffix resolver on only one transport — both surfaces need it in the same PR (ADR-0022).
- Key Concept entity/edge ids on `graph_id` — that prevents cross-document consolidation.

## Testing Strategy

- **Logical-name identity + cross-file resolution (Rust extractor):** TDD — unit tests on extraction for the bare-name identity, local-callee resolution, and cross-file index lookup.
- **Query-tool suffix matching (TS + Rust):** TDD — a bare symbol resolves to the entity (and its callers/callees) via suffix/exact match, on both the TS tools (`tools.ts`/`codegraph.ts`) and the Rust stdio MCP (`mcp/engram-mcp/src/codegraph.rs`).
- **Code-symbol noise filter:** TDD — already green (`is_noise_symbol` unit tests).
- **Document `concept_name` removal (Rust extractor):** TDD — a non-code document yields zero entities from the extractor (chunks only); `concept_name()` is removed.
- **Per-document chunk reads (surface-parity):** TDD — `listChunksByDocument(document_id, scope)` returns the document's chunks across facade + binding + `@engram/node`.
- **`extract-knowledge` op (TS, LLM):** TDD for the tool-schema parse + request shaping, and a **goal-based integration** check injecting `completeOverride` with a fixture carrying real `concepts[]`/`properties[]`/`relationships[]` arguments (mirroring `maintenance.llm.test.ts`) — asserting Concept entities + typed edges are written, a re-run is idempotent, and the same concept in two documents converges. (`PI_DRY_RUN=1` returns empty arguments and is NOT used.)
- **MCP surfacing:** goal-based — `maintenance_run` with `op=extract-knowledge` dispatches to the op.

## Acceptance Criteria

- [x] Bare-generic symbols (`new`, `clone`, `len`, `fmt`, primitives) are suppressed at code extraction. *(shipped — `is_noise_symbol` tests green)*
- [x] **(rev)** Code entities extracted from a source carry the **bare logical name** as `entity.name` (e.g. `alpha`, not `my-repo/src/lib.rs::alpha`); repo/path/branch live in `source_refs`/provenance/metadata. *(RFC-0020 rev: `adapters/ingest/tests/extractor.rs` `code_entities_carry_logical_names`)*
- [x] **(rev)** Git provenance keys `repository` (remote), `branch`, `revision` (SHA) are stamped on each code entity's metadata from the `detect_git` tuple and are reachable on the cc entity-detail route (`/graph/entity/:id` → `git.branch`). *(RFC-0020 rev: `adapters/ingest/src/scanner.rs` + `extractor.rs`)*
- [x] **(rev)** The community graph keys nodes by entity **id** (not name) so same-named functions stay distinct nodes; the display layer still shows logical names. *(RFC-0020 rev: `core/integration/src/sqlite/community.rs` `key_of` prefers id)*
- [x] `symbol_context`/`change_impact`/`search` resolve a user-supplied **bare** symbol (e.g. `parse_symbol`) via suffix/exact match, on BOTH the TS HTTP MCP and the Rust stdio MCP — no empty-result regression on either transport. *(T7: `packages/runtime/test/codegraph.test.ts` + `mcp/engram-mcp/src/codegraph.rs` tests)*
- [x] Cross-file `calls` edges still resolve (`object.id` populated) after qualified identities land. *(T2: `adapters/ingest/tests/extractor.rs` `cross_file_calls_resolve_after_qualification`)*
- [x] The naive document `concept_name` path emits zero entities from non-code documents at ingest; `concept_name()` is dead code and removed. *(T3: `adapters/ingest/tests/extractor.rs` `non_code_documents_emit_no_graph_entities`)*
- [x] `extract-knowledge` over a fixture yields `Concept` entities + typed edges (`depends_on`/`has_property`/`relates_to`) for valid concepts, with doc-heading noise (`Architecture`, `overview`, `introduction`) excluded. *(T5: `packages/runtime/test/maintenance.extract-knowledge.test.ts`)*
- [x] `extract-knowledge` is idempotent — a second run writes no duplicate entities or edges; the same concept from two documents converges to one entity (keyed on scope + canonical label, `graph_id = None`). *(T5: same test — idempotency + convergence cases)*
- [x] `extract-knowledge` is reachable through the existing `maintenance_run` MCP tool (`op=extract-knowledge`), with no new tool added (36-tool count unchanged). *(T6: `packages/runtime/test/mcp.test.ts` — dispatch + 36-tool parity)*
- [x] `extract-knowledge` reads chunks per-document (`listGraphs` + `listChunksByDocument`); it never loads the full scope-wide chunk set (no OOM on the 442k-chunk store). *(T4: `packages/node/test/transport.test.ts` `listChunksByDocument`; T5 op uses it per-document)*

## Assumptions

- Technical: The bare logical name is the treesitter anchor's symbol (`"fn bar"` → `bar`); `document.path` and `source.metadata[STABLE_SOURCE_KEY]` stay in `source_refs`/metadata, NOT in `entity.name` (source: `adapters/ingest/src/extractor.rs`). The entity `id` is derived from `graph_id + name` and stays unique without qualification.
- Technical: Three user-facing tools seed their BFS/match with the **user-supplied bare** symbol string on BOTH transports — TS (`flattenEdges` keys edges by name, `symbolContextBFS`/`changeImpactBFS` take the symbol as the BFS seed, `search` does `entityName === query`: `packages/runtime/src/mcp/tools.ts:40-55,620-682`) and Rust stdio MCP (`mcp/engram-mcp/src/main.rs:379,574,580,592` → `mcp/engram-mcp/src/codegraph.rs`) → the suffix/exact resolver (Task T7) keeps a bare query resolving to the right entity.
- Technical: The 442k FastEmbed vectors are `EmbeddingTargetType::Chunk` embeddings, not entity/concept embeddings (source: `adapters/sqlite/src/vector/index.rs:258,540,587`) → qualified identity and doc-Concept changes need no re-embedding, only a graph-layer re-scan.
- Technical: Cross-file call resolution keys on bare `entity.name` against a global name→id index (source: `adapters/ingest/src/extractor.rs:367-380`) → must be preserved when names are qualified; the plan qualifies local callees via the document's symbol table and keeps a secondary bare→id index.
- Technical: Existing same-key convergence precedents key on the scope discriminator with `graph_id = None` — `contract_entity_id` (`adapters/ingest/src/contract_entities.rs:39-52`), `repo_entity_id` (`extractor.rs:660-673`) → Concept entities/edges from `extract-knowledge` must follow the same pattern to consolidate across documents (RFC-0014).
- Technical: `EntityKind::Concept` is still emitted by distillation (`core/knowledge/src/identity.rs:241`), retrieval fusion (`bindings/node/src/knowledge_fusion.rs:196`), and the associative/community indexes — this spec adds the `extract-knowledge` emitter and removes only the naive extractor prose path (`adapters/ingest/src/extractor.rs:144`).
- Technical: pi-mono errors surface as **throws from `complete()`** (handled in the LLM provider wrapper, `packages/runtime/src/maintenance/llm.ts:144-149`); ops perform no per-call `errorMessage` check — `reflect.ts`/`contradict.ts` simply `await llm.complete(...)` (source: `reflect.ts:61-66`, `contradict.ts:90-95`).
- Technical: `listGraphs` is already bound to TS (binding `list_graphs_json` → transport `listGraphs`: `bindings/node/src/knowledge_graph.rs:46`, `packages/node/src/transport.ts:192`); `list_chunks_by_document` exists in the port (SQLite-implemented, used for retraction: `core/knowledge/src/repository.rs:92-101`) but is NOT exposed to TS → expose it for per-document chunk reads (Task T4). This avoids the unbounded `listChunks` (returns all 442k chunks: `bindings/node/src/knowledge.rs:276`, `transport.ts:212`).
- Process: RFC-0020 is Accepted (2026-08-07) and is the constraining design doc (source: user confirmation 2026-08-07).
- Product: Documents yield a knowledge concept sub-graph via LLM `extract-knowledge` (not removed); valid concepts/properties/relationships only (source: user confirmation 2026-08-07).
- Product: `entity.name` is the bare logical symbol; repo/path/branch are disambiguators in `source_refs`/provenance/metadata, NOT identity. Git `branch`/`revision` are provenance-only metadata (re-index updates the branch; one logical entity per function). The community graph keys nodes by entity id so same-named functions stay distinct (source: user reversal 2026-08-07, RFC-0020 rev).
- Technical/design: `extract-knowledge` is an LLM `engram-maintain` op mirroring `reflect`/`contradict`; deterministic Rust ingest chunks documents only and emits no doc entities (source: my recommendation, user-delegated 2026-08-07).
