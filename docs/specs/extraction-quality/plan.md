# Plan: extraction-quality

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Done -->

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Four Rust changes in the deterministic extractor (`adapters/ingest`), one internal
surface exposure (per-document chunk reads), one dual-surface TS+Rust query-tool fix,
and one new TS LLM maintenance op (`packages/runtime`). The code-symbol noise filter is
already shipped.

1. **Qualified code identity** — extraction emits `{repo}/{path}::{bare_name}` (Phase 1
   ships bare name; receiver enrichment deferred — treesitter anchors carry no receiver).
   Changes entity IDs (graph-layer only; the 442k chunk vectors are untouched) → needs a
   re-scan to rebuild entities/relationships.
2. **Cross-file `calls` resolution preserved** — the global name→id index keys on bare
   names; qualify local callees via the document's symbol table, and keep a secondary
   bare→id index so cross-document callees resolve (collision-degradation until a Phase 2
   scope-wide symbol table).
3. **Query-tool suffix matching on BOTH transports** — `symbol_context`/`change_impact`/
   `search` seed BFS with the user-supplied bare symbol; add suffix/alias resolution in
   the TS tools (`tools.ts`/`codegraph.ts`) AND the Rust stdio MCP (`mcp/engram-mcp/src/
   codegraph.rs`) so qualified identities don't empty results on either surface (ADR-0022).
4. **Document `concept_name` removal** — the naive heading-as-node prose path emits no
   entities; `concept_name()` is removed; the scanner-side `describes` bridge becomes a
   no-op and is removed.
5. **Per-document chunk reads** — expose the existing `list_chunks_by_document` (port +
   binding + `@engram/node`); the op iterates the already-bound `listGraphs`, one LLM call
   per document. Avoids the unbounded `listChunks` (OOM on 442k chunks) and keeps concept
   provenance per-document by construction.
6. **`extract-knowledge` LLM op** — a new `engram-maintain` op (mirroring
   `reflect`/`contradict`) that reads each document's chunks, asks pi-mono for
   concepts/properties/relationships, and writes `Concept` entities + typed edges,
   consolidated across documents by canonical identity. Surfaced via the existing
   `maintenance_run` MCP tool.

Riskiest parts: #2 (cross-file resolution under qualified identities) and the
dual-surface #3.

## Constraints

- RFC-0020 (Accepted) Phase 1; RFC-0014 (canonical concept identity); RFC-0018 (retrieval quality); ADR-0022 (surface parity).
- AGENTS.md: `engram-domain`/the Rust extractor stays LLM- and NLP-free; LLM work lives in `engram-maintain` (TS).
- No new store schema columns — provenance goes in `record_json` metadata.
- No new MCP tool — extend `maintenance_run` with `op=extract-knowledge`.

## Construction tests

- **Integration (cross-file):** a two-document Rust fixture where doc A calls a symbol defined in doc B; after qualified-identity extraction the `calls` edge has `object.id` populated. Spans T1 + T2.
- **Integration (query-tool suffix match, both surfaces):** after T1 lands qualified names, a bare-symbol `symbol_context`/`search` call returns the qualified entity + neighbors on the TS HTTP MCP and the Rust stdio MCP. Spans T1 + T7.
- **Integration (extract-knowledge):** `completeOverride` injection with a fixture carrying real `concepts[]`/`properties[]`/`relationships[]` writes Concept entities + typed edges; a second run writes nothing new; the same concept in two fixture documents converges to one entity. Spans T4 + T5 + T6.
- **Manual verification:** after a real re-scan + one `extract-knowledge` run against the agentzero store, `architecture` returns qualified boundaries (no `new`/`clone`) and a concept query returns document-derived concepts.

## Design (LLD)

### Design decisions
- Qualified identity = `{repo}/{path}::{bare_name}`; `{repo}` = stable-source-key (fallback to a scope-derived id when absent). Receiver/impl-ctx nesting deferred (anchors lack it). Traces to: AC "qualified identities".
- Query tools take a user-supplied bare symbol → resolve to qualified NAME(s) via `name === query || name.endsWith("::" + query) || name.endsWith("/" + query)` before BFS/match, on both transports. Traces to: AC "suffix matching both surfaces".
- Cross-file resolution keeps a secondary **bare→id** index (last-write-wins) so cross-document callees resolve; bare-name collisions may pick one target — a documented degradation, removed by a Phase 2 scope-wide symbol table (qualified callee at extraction time, distinct from typed edges). Traces to: AC "cross-file calls still resolve".
- Concept entity id = `concept_entity_id(scope, canonical_label)` with `graph_id = None`; relationship id = `(scope, subject_id, predicate, object_id)` — mirroring `contract_entity_id`/`repo_entity_id` so concepts consolidate across documents (RFC-0014). Traces to: AC "idempotent + consolidates".
- `extract-knowledge` iterates documents (one `complete()` per document) rather than paging a mixed-document chunk stream — concept provenance (`has_property`, `describes`) attaches to the source document cleanly. Traces to: AC "per-document chunk reads".
- `extract-knowledge` is an LLM maintenance op, not an ingest-time extractor. Rejected: deterministic NLP in Rust. Traces to: AC "extract-knowledge yields concepts/edges".
- Reuse `EntityKind::Concept`; do not add a new kind.

### Data & schema
- No schema change. Entity `name` carries the qualified identity (existing `String` field). `entity_id`/`relationship_id` hashes incorporate the qualified name → IDs change for existing data (re-scan rebuilds; chunk vectors untouched).
- `Concept` entities/edges from `extract-knowledge` carry `record_json` provenance (`extraction_method: "llm"`, `document_id`); `graph_id = None`; git remote/revision land in `record_json` in Phase 4.

### Interfaces & contracts
- No new public contract or MCP tool. `maintenance_run` gains `op=extract-knowledge`. `listChunksByDocument(document_id, scope)` is exposed across the knowledge repository port (method already exists for retraction) + N-API binding + `@engram/node` transport (ADR-0022 internal surface). `listGraphs` is already bound.

### Component / module decomposition
- Rust: `adapters/ingest/src/extractor.rs` (qualified identity, cross-file index, `concept_name` removal); `adapters/ingest/src/scanner.rs` (remove `describes` bridge); `mcp/engram-mcp/src/codegraph.rs` (Rust suffix resolver).
- Rust binding: `bindings/node/src/knowledge.rs` (`list_chunks_by_document_json`).
- TS: `packages/runtime/src/maintenance/extract_knowledge.ts` (new); `packages/runtime/src/maintenance/cli.ts` (`MaintainOp`/`OPS`/dispatch); `packages/runtime/src/mcp/tools.ts` + `codegraph.ts` (TS suffix resolver); `packages/node/src/transport.ts` (`listChunksByDocument`).

### Behavior & rules
- Noise filter applies to code symbols (`is_noise_symbol`) and to extracted concepts — `is_noise_concept` is ported to TS for `extract-knowledge` and gains a doc-heading-generic blocklist (`architecture`, `overview`, `introduction`, `background`, `summary`, `conclusion`, `references`).
- `extract-knowledge` idempotency + consolidation: stable ids keyed on `(scope discriminator, canonical label)` / `(scope, subject_id, predicate, object_id)` with `graph_id = None` → re-runs upsert, same concept across docs converges.
- Non-code documents emit zero entities from the deterministic extractor.

### Failure, edge cases & resilience
- LLM errors surface as **throws from `complete()`** (handled in the provider wrapper, `llm.ts:144-149`); the op performs no per-call `errorMessage` check — match `reflect.ts`/`contradict.ts` exactly.
- Partial writes: each entity/edge is an independent upsert; a mid-op failure leaves a partial-but-valid sub-graph; re-running completes it (idempotent).
- Per-document reads bound memory (one document's chunks at a time, not the 442k scope-wide set) and bound prompt size per LLM call.

## Tasks

### T1: Code entities carry qualified identities

**Depends on:** none

**Tests:**
- Unit: a code fixture extracts an entity whose `name` is `{repo}/{path}::{bare_name}` (repo = stable-source-key; fallback when absent).
- Unit: `entity_id` derives from the qualified name, so a re-scan upserts the same id. Verifies AC "qualified identities".

**Approach:**
- In `extract_with_calls`, build the qualified prefix once from `document.path` + `source.metadata[STABLE_SOURCE_KEY]`.
- Compose each symbol's stored name as `{repo}/{path}::{bare_name}`. (No receiver nesting — anchors lack it.)
- Update `entity_id`/`relationship_id` hashing to key on the qualified name.

**Done when:** `cargo test -p engram-ingest` green and the qualified-identity unit test asserts the `{repo}/{path}::{bare_name}` format.

### T2: Cross-file `calls` resolution preserved under qualified identities

**Depends on:** T1

**Tests:**
- Unit (intra-doc): a call from A to B in the same document resolves `object.id`.
- Unit (cross-doc): a call to a symbol defined in another scanned document resolves `object.id`. Verifies AC "cross-file calls still resolve".

**Approach:**
- Build a local `bare→qualified` map from the document's own symbols; qualify local callees before forming edges.
- Maintain a secondary global `bare→id` index alongside the qualified one so cross-document bare callees resolve (last-write-wins). Document the collision-degradation in a comment.

**Done when:** the two-document cross-file fixture asserts `object.id` populated for both intra-doc and cross-doc `calls`.

### T3: Remove naive document `concept_name` emission + dead `describes` bridge

**Depends on:** none

**Tests:**
- Unit: a non-code (markdown) document yields **zero** entities from `GraphExtractor::extract`.

**Approach:**
- In `extract_with_calls`, replace the non-code `else` branch: emit no symbols/entities (drop the `concept_name` Concept path).
- **Preserve the graph-record creation + `DOCUMENT_ID_KEY` metadata stamping (`extractor.rs:93-96,100-115`) for non-code documents.** T3 removes ONLY entity emission, not graph persistence — do NOT early-return before the `KnowledgeGraph` construction. T5 relies on `listGraphs` returning graph records (with `DOCUMENT_ID_KEY`) for non-code docs to discover them.
- Remove the now-dead `concept_name()` (`extractor.rs:425-434`). (`is_noise_concept()` stays — reused, ported to TS in T5.)
- Remove the scanner-side `describes` bridge (`adapters/ingest/src/scanner.rs:562-575`) — with no document Concept entities it is a no-op. Contract-entity extraction (`contract_entities.rs`, `EntityKind::Api`) is unaffected.

**Done when:** the markdown→0-entities unit test is green and `cargo test -p engram-ingest` passes.

### T4: Expose per-document chunk reads to TS

**Depends on:** none

**Tests:**
- Integration (TDD): `listChunksByDocument(document_id, scope)` returns the document's chunks across facade + binding + `@engram/node`. Verifies AC "per-document chunk reads".

**Approach:**
- Expose the existing `list_chunks_by_document` (`core/knowledge/src/repository.rs:92-101`, already implemented by the SQLite adapter for retraction) to the N-API binding (`list_chunks_by_document_json`) + `@engram/node` transport.
- Reuses the existing port method — no new SQL query. Bounded per-document (not the 442k scope-wide set). The unbounded `listChunks` stays for other callers but is not used by the op.

**Done when:** the per-document chunk-read test is green; the op reads one document's chunks at a time.

### T5: `extract-knowledge` LLM maintenance op

**Depends on:** T3, T4 *(T3: non-code docs still produce a graph record with `DOCUMENT_ID_KEY` so `listGraphs` can discover them; T4: per-document chunk reads)*

**Tests:**
- Unit (TDD): the pi-mono structured-output tool schema parses a fixture response into `concepts[]`/`properties[]`/`relationships[]`.
- Integration (goal-based, `completeOverride`): running `extract-knowledge` over a two-document fixture writes exactly **3 Concept entities + 4 typed edges**; a second run writes nothing new; the same concept in both documents converges to one entity. Verifies ACs "yields concepts/edges", "idempotent + consolidates". (`PI_DRY_RUN=1` returns empty arguments — not used.)

**Approach:**
- New `packages/runtime/src/maintenance/extract_knowledge.ts`, mirroring `reflect.ts`/`contradict.ts`.
- Iterate documents via the already-bound `listGraphs` (each graph is per-document; `document_id` from `graph.metadata[DOCUMENT_ID_KEY]`, stamped by the extractor at `extractor.rs:93-96`). For each document, fetch its chunks via `listChunksByDocument` → one `complete()` call per document (per-document provenance; no mixed-document prompts). `await llm.complete(...)` — errors throw from the wrapper, no per-call `errorMessage` check.
- pi-mono `complete` with a TypeBox tool emitting `concepts`/`properties`/`relationships`.
- Port `is_noise_concept` to TS and add the doc-heading-generic blocklist; filter concepts.
- Write `Concept` entities + typed edges via the provider transport with id keys `(scope, canonical_label)` (`graph_id = None`) / `(scope, subject_id, predicate, object_id)`.

**Done when:** the `completeOverride` integration test asserts exactly 3 Concept entities + 4 typed edges, idempotency, and cross-doc consolidation.

### T6: Surface `extract-knowledge` via `maintenance_run` + CLI

**Depends on:** T5

**Tests:**
- Goal-based: `maintenance_run` with `op=extract-knowledge` dispatches to the op; the 36-tool list is unchanged. Verifies AC "reachable via maintenance_run".

**Approach:**
- Register `extract-knowledge` in the MCP `maintenance_run` handler (`packages/runtime/src/mcp/tools.ts`); lazy-import the LLM module (same pattern as `contradict`).
- CLI: `MaintainOp` and `OPS` (`packages/runtime/src/maintenance/cli.ts:12-13`) gain `'extract-knowledge'`; the `runMaintain` dispatch (`cli.ts:120-131`) and the `runMaintainFromArgs` LLM-seeding branch (`cli.ts:177-179`) gain the case.

**Done when:** the MCP parity test lists 36 tools unchanged and `maintenance_run op=extract-knowledge` dispatches.

### T7: Suffix/alias match for user-supplied symbols (TS + Rust stdio MCP)

**Depends on:** T1

**Tests:**
- Unit (TDD, TS): given qualified entity names, a bare query `parse_symbol` resolves to the qualified entity for `search`; `symbol_context`/`change_impact` return its neighbors.
- Unit (Rust): the Rust stdio MCP codegraph resolver returns matching results (parity with TS). Verifies AC "suffix matching both surfaces".

**Approach:**
- TS (`packages/runtime/src/mcp/tools.ts` + `codegraph.ts`): add a resolver — given a bare symbol, match entities where `name === query || name.endsWith("::" + query) || name.endsWith("/" + query)`; **seed BFS with the resolved qualified NAME(s)** (not ids — the adjacency maps are keyed by name strings). Multi-match policy: `symbol_context`/`change_impact` union callers/callees (and blast radius) across matches, de-duped; `search` returns all matches.
- Rust (`mcp/engram-mcp/src/codegraph.rs`): add the same suffix resolver so `symbol_context({symbol})` on the stdio MCP matches the TS surface. T1 qualifies names on both surfaces, so both need the resolver in this PR (ADR-0022).

**Done when:** both the TS and Rust suffix-match tests are green; a bare-symbol `symbol_context` returns callers/callees on both surfaces.

### T8: Phase 1 acceptance verification + re-index guidance

**Depends on:** T1, T2, T3, T4, T5, T6, T7

**Tests:**
- Manual: after a re-scan, `architecture` returns qualified boundaries (no `new`/`clone`); after one `extract-knowledge` run, a concept query returns document-derived concepts. Verifies ACs "qualified identities", "concept_name removal", "extract-knowledge yields concepts".

**Approach:**
- Run gates: `cargo fmt --all`, `cargo check --workspace`, `pnpm typecheck`, `pnpm test`.
- Document the re-index step (graph-layer re-scan; no re-embedding) in `docs/guides/how-to/extend-storage.md`.

**Done when:** all gates green and the re-index guidance is committed.

## Rollout

- **Delivery:** big-bang behind the existing scan + maintenance ops. Reversible — a re-scan with the prior extractor restores prior entity IDs (graph layer only; chunk vectors untouched). No irreversible item (no schema migration, no published event).
- **Infrastructure:** none — reuses the pi-mono LLM runtime already deployed for `reflect`/`contradict`.
- **External-system integration:** pi-mono (Anthropic default, Ollama via env) — already live.
- **Deployment sequencing:** Rust identity (T1+T2+T3) + TS query-tool fix (T7) ship together (coupled by the identity change); T7's Rust stdio MCP resolver ships in the same PR (T1 qualifies names on both surfaces). T4+T5+T6 (TS op + per-doc reads) ship together. Stored graph is rebuilt by a re-scan; existing naive document concepts vanish on re-scan, replaced by `extract-knowledge` output on demand.

## Risks

- **Cross-file call resolution precision:** the secondary bare→id index may resolve a colliding bare callee to the wrong target. Accepted for Phase 1 (qualified identity still disambiguates the entities); removed by a Phase 2 **scope-wide symbol table** (qualified callee at extraction time) — distinct from Phase 2 typed edges, which are a separate concern.
- **ID churn on existing data:** qualified identities change entity IDs; any consumer that persisted entity IDs externally breaks until re-scan. Mitigation: re-scan is the documented migration; no external ID consumers known.
- **LLM cost/latency:** `extract-knowledge` over a large doc corpus is slow/costly (one `complete()` per document — like reflection). Mitigation: explicit, on-demand op; per-document reads bound prompt size per call.

## Changelog

- 2026-08-07: initial plan (headers + full body) for RFC-0020 Phase 1.
- 2026-08-07: adversarial-review pass 1 — added T4 (bounded chunk paging, hard prereq not "if"), T7 (query-tool suffix match — Blocker: qualified identities broke `symbol_context`/`change_impact`/`search`); fixed idempotency key to `(scope, canonical_label)` graph_id=None (RFC-0014 consolidation); dropped T1 `::receiver::name` (anchors lack receiver); `completeOverride` not `PI_DRY_RUN`; `errorMessage` throws from `complete()` not per-call; T3 removes dead `describes` bridge + `concept_name()`; T5 ports `is_noise_concept` + doc-heading blocklist; T6 lists `MaintainOp`/`OPS`/dispatch touch points; removed Phase-4 git AC from this spec; reworded cross-file risk (symbol table ≠ typed edges); pinned fixture counts (3/4).
- 2026-08-07: adversarial-review pass 2 — T7 extended to the Rust stdio MCP (`mcp/engram-mcp/src/codegraph.rs`) so both transports get the suffix resolver in-PR (ADR-0022; was a deferred "follow-up" risk); T4 re-scoped from a new scope-wide `listChunksPaged` to exposing the existing `list_chunks_by_document` (smaller surface; per-document by construction); T5 iterates the already-bound `listGraphs`, one `complete()` per document (fixes per-document provenance + OOM); nits — seed BFS with qualified NAME(s) not ids; pin multi-match policy (union callers/callees/blast radius, de-duped; search returns all); Contract line clarifies `listChunksByDocument` is internal surface; align `{qualified_name}` = `{bare_name}` (Phase 1) terminology + fix "callees" typo; deployment label "Rust identity (T1+T2+T3) + TS query-tool fix (T7)".
- 2026-08-07: adversarial-review pass 3 — pinned the T3↔T5 coupling invariant: T3 preserves graph-record creation + `DOCUMENT_ID_KEY` stamping for non-code docs (T5 discovers documents via `listGraphs`); T5 `Depends on:` now lists T3, T4. Reviewer confirmed graph `metadata` round-trips through `listGraphs` and `list_chunks_by_document` is a real SQLite impl.
- 2026-08-07: T8 executed (T1–T8 all committed + gated). Re-index doc landed in `docs/guides/explanation/how-repos-get-indexed.md` (not `docs/guides/how-to/extend-storage.md` as the T8 body named — the former is the topically-correct home; it already explains the indexing pipeline). Implementation adversarial review: Blocker (spec status/ACs) fixed; Concerns — enforced `has_property` on property edges, removed the dead scanner `mentions` block; Concern 2 (scope-discriminator delimiter) was a false positive (the join already uses ``, rendered invisibly).
