# Engram MCP — Disconnected Knowledge Graph & Multi-File Storage

> **Status:** Research + plan (not yet implemented). Date: 2026-07-29.
> **Scope:** The `engram-mcp` server (`mcp/engram-mcp`) as deployed to agentzero, its
> engram-ingest scanner (`adapters/ingest`), and its compatibility with the agentzero
> memory layer (`agentzero/stores/zbot-engram-adapter`).
> **Problem sources:** problem-statement screenshots (`Pictures/enhancement/img_1.jpeg`,
> `img_2.jpeg`) + on-disk evidence at `~/.engram/agentzero/`.

This document records three related defects found while dogfooding the unified engram-mcp
(RFC-0015) against the agentzero repo, and proposes a sequenced fix plan. Implementation
proceeds through the repo's spec + work-loop process once this plan is reviewed.

---

## TL;DR

1. **Disconnected knowledge graph.** Scanning a repo produces two subgraphs that never meet
   directly: a *code* subgraph (function / class / struct + `calls` / `belongs_to`) and a
   *concept* subgraph (concept + `mentions`). They only touch at the per-source `Repository`
   node (a star), with **no concept→code edges** and **no cross-document concept→concept edges**.
   Fix: a deterministic bridging pass in the scanner + a stronger agentic distillation skill.
2. **Five SQLite files instead of one.** `engram-mcp` calls `EngramConfig::new(...)`
   *without* `.with_sqlite_storage_layout(...)`, so it inherits engram core's
   `MultiFileDirectory` default and writes `memory.db / knowledge.db / belief.db /
   hierarchy.db / vectors.db`. Fix: default the MCP to `SingleFile { "engram_data.db" }`.
3. **MCP ↔ agentzero adapter divergence.** The agentzero adapter (`zbot-engram-adapter`)
   already defaults to `SingleFile { "engram_data.db" }`; the MCP does not. So a DB written
   by the MCP is **not consumable** by the agentzero gateway and violates
   `agentzero/docs/specs/engram-provider-adoption/spec.md`. Fix: same as (2) — make the MCP
   match the adapter's single-file invariant.

---

## 1. Problem statement (from the screenshots)

The engram knowledge graph at `~/.engram/agentzero/knowledge.db` contains two completely
disconnected subgraphs that cannot be traversed between:

- **Code subgraph** — `function` / `class` / `struct` entities, connected by `calls` and
  `belongs_to` edges. Sourced from code files (951 files: `.rs`, `.ts`, `.py`, …).
- **Concept subgraph** — `concept` entities, connected by `mentions` edges. All 2,278
  concepts are sourced from text/markdown files (372 files).

The screenshot's framing attributes the gap to *concepts not being "promoted to graph
nodes"* — their `sourceRefs` point at `knowledge_documents` records that "are never promoted
to a node in `knowledge_entities`", making `sourceRefs` a dead end for the visualizer.

The investigation below corrects that framing slightly (see §3.2) and locates the true root
cause, but the **visible symptom is exactly as described**: the graph is siloed, and
`engram-viz` cannot render concept↔document or concept↔code links.

Supporting SQL from the screenshot (confirmed plausible against the schema):

```sql
-- All concept entities source back to text/markdown documents.
SELECT json_extract(d.record_json, '$.kind') AS doc_kind, COUNT(*) AS n
FROM knowledge_entities e
JOIN knowledge_documents d
  ON json_extract(e.record_json, '$.sourceRefs[0].targetId') = d.id
WHERE json_extract(e.record_json, '$.kind') = 'concept'
GROUP BY doc_kind;
-- result: text|2278
```

---

## 2. Evidence on disk

```
~/.engram/agentzero/
  belief.db       20,480 B
  hierarchy.db    20,480 B
  knowledge.db    131,354,624 B   (the big one — 951 code + 372 text docs indexed)
  memory.db          94,208 B
  vectors.db       118,784 B
```

Five files = the `MultiFileDirectory` shape. `knowledge.db` owns:
`knowledge_sources, knowledge_documents, knowledge_chunks, knowledge_entities,
knowledge_relationships, knowledge_graphs, concept_schemes, concepts, concept_relations,
ontologies, ontology_classes, ontology_properties, ontology_axioms`.

The MCP server attached to this storage is registered in `~/.claude.json` /
`~/.codex/config.toml`:

```jsonc
"engram": { "command": ".../mem-alpha/target/release/engram-mcp",
            "args": ["--storage", "/home/videogamer/.engram/agentzero",
                     "--project", "agentzero",
                     "--ontology", ".../agentzero-ontology.json",
                     "--taxonomy", ".../agentzero-taxonomy.json"] }
```

---

## 3. Root-cause analysis

### 3.1 Five SQLite files — the MCP ignores the layout option

- **engram core default is `MultiFileDirectory`.**
  `core/integration/src/config.rs:41-60` defines `SqliteStorageLayout` with
  `#[default] MultiFileDirectory`. `EngramConfig::new(...)` (`config.rs:187-204`, hard-coded
  at L202) sets it. The only override is the builder `.with_sqlite_storage_layout(layout)`
  (`config.rs:214-218`). `bootstrap_sqlite` then materializes per-store paths
  (`core/integration/src/sqlite/bootstrap.rs:62-98`) — branching on the layout, so it *can*
  fold everything into one file when asked.
- **The MCP never asks.** `mcp/engram-mcp/src/bootstrap.rs:13-32` `open_provider()` calls
  `EngramConfig::new(...)` with **no** `.with_sqlite_storage_layout(...)`, inheriting the
  multi-file default. `McpConfig` (`mcp/engram-mcp/src/config.rs:14-77`) has no layout field
  at all — it only parses `--storage/--project/--ontology/--taxonomy`.

### 3.2 Disconnected graph — edge topology, not "promotion"

The ingestion-pipeline investigation corrects the screenshot's premise:

> **There is no `sourceRefs → graph-node` promotion step anywhere — for code *or* concepts.**
> Both kinds of entities carry an identical `source_refs` shape (`extractor.rs:154-167`),
> each pointing at a `knowledge_documents.id`. Neither is ever turned into a
> `KnowledgeEntity` node representing the document.

The real divergence is **which edges get emitted**:

| Edge type | Code path | Text/markdown path |
|---|---|---|
| `calls` (intra-doc, from AST) | ✅ `extractor.rs:185-223` | ❌ no AST for `.md`/`.txt` |
| `calls` (cross-doc resolution) | ✅ `scanner.rs:428-442` (**gated** `predicate == "calls"`) | ❌ |
| `mentions` (intra-doc co-occurrence) | n/a | ✅ `extractor.rs:224-253` |
| `mentions` (cross-document) | ❌ | ❌ co-occurrence only iterates the **per-document** `index` (`extractor.rs:230`) |
| `belongs_to` → Repository | ✅ | ✅ (`extractor.rs:276-331`, unconditional) |
| `describes` / `realized_by` / `implements` → code | ❌ | ❌ |

So both subgraphs technically meet at the per-source `Repository` node (a star), but there are
**no concept→concept edges across documents** and **no concept→code-entity edges at all**.
That is the disconnection the visualizer shows. The screenshot's "dead-end `sourceRefs`" is
the *rendering* consequence: with no concept→code or concept→doc-node edges to traverse, a
concept node has nothing to render against.

**The contract already supports bridging** — nothing schema-level blocks the fix:

- `KnowledgeRelationship.predicate` is a free-form `String`
  (`core/domain/src/knowledge.rs:232-248`).
- The MCP's default ontology defines `across` predicates `describes`, `realized_by`
  (`mcp/engram-mcp/src/ontology.rs:47-66`) — explicitly for cross-layer bridging.
- The `engram-distill` skill already *instructs* the agent to use across predicates to bridge
  layers (`extensions/engram-distill/SKILL.md:46-52`).
- `store_knowledge` (`tools.rs:339`) and `put_relationship` (`tools.rs:223`) persist arbitrary
  caller-supplied predicates through `KnowledgeRepository::put_relationship`.

**Two divergence points are where bridging is currently absent:**

1. `adapters/ingest/src/scanner.rs:428-442` — the cross-file resolver hard-codes
   `if rel.predicate == "calls"`. This is the single most relevant spot: a concept→code
   `describes` resolution pass would slot in here, after all per-file graphs are extracted,
   using the global `name_index`.
2. `adapters/ingest/src/extractor.rs:224-253` — the co-occurrence loop only iterates the
   per-document `index`. A cross-document index would let a concept link to entities named
   in *other* documents (including code entities).

**Bonus gap:** `adapters/ingest/src/markdown_chunker.rs` exists (emits heading anchors like
`"# Title"`) but is **not wired into the scanner** — `.md` files go through `PlainTextChunker`
(`scanner.rs:153-155`). The heading anchors are a high-precision signal for matching a concept
section to a code symbol of the same name.

### 3.3 MCP ↔ agentzero adapter divergence

The agentzero memory layer does single-file correctly:

- `agentzero/stores/zbot-engram-adapter/src/config.rs:51-70,417-419` —
  `AdapterSqliteStorageLayout` whose `Default` is `SingleFile { "engram_data.db" }`.
- `agentzero/stores/zbot-engram-adapter/src/config.rs:336` — `to_engram_config()` chains
  `.with_sqlite_storage_layout(self.sqlite_storage_layout.to_engram())`.
- `agentzero/gateway/gateway-memory/src/lib.rs:709-715` — gateway default is also
  `SingleFile`.
- `agentzero/gateway/src/state/persistence_factory.rs:460-477` (test) asserts the five
  per-store files **do not exist** in single-file mode.

The engram-mcp server is a *separate binary* (the mem-alpha `mcp/engram-mcp` crate) that
bypasses `AdapterConfig` / `to_engram_config()` entirely. Because it leaves the layout at the
multi-file default, it writes a shape the adapter's contract forbids — violating
`agentzero/docs/specs/engram-provider-adoption/spec.md:23` ("`engram_data.db` single-file
layout") and its acceptance criterion at `:56` ("…without … additional SQLite files"). A
gateway that later reopens `~/.engram/agentzero/` expecting one `engram_data.db` will not find
the data laid out the way it expects.

---

## 4. Plan

Three phases, sequenced so the cheapest, highest-certainty fix lands first and unblocks
shared DB consumption, before the graph-topology work. Each phase becomes its own spec under
`docs/specs/` and runs through `work-loop`.

### Phase 1 — Single-file storage (fixes §3.1 + §3.3) · low risk, ~half a day

**Goal:** the MCP writes exactly one `engram_data.db`, matching the agentzero adapter
invariant, so agentzero's gateway can consume the same DB.

- **P1-T1** — Default the MCP to single-file. In `mcp/engram-mcp/src/bootstrap.rs`
  `open_provider`, chain `.with_sqlite_storage_layout(SqliteStorageLayout::SingleFile {
  file_name: "engram_data.db".to_owned() })` onto the `EngramConfig::new(...)` call. Reuse
  the exact file name the adapter uses (`engram_data.db`) for cross-consumer compatibility.
  *Mode: goal-based check — open a provider, assert one file exists, assert the five names do
  not.*
- **P1-T2** — Expose a `--layout single|multi` flag (default `single`) in `McpConfig`
  (`config.rs`) plumbed through `open_provider`, with an optional `--db-file <name>` override.
  Keeps the multi-file path available for tests / advanced users without making it the
  default. *Mode: TDD — `from_args` parses `--layout`/`--db-file`; reject unknown values.*
- **P1-T3** — Migrate the existing deployment. The current `~/.engram/agentzero/` holds five
  files; the data is fully regenerable via `scan_repo` + distillation, so the clean move is:
  back up the dir, delete it, re-point the MCP (now single-file), and re-index. Document the
  one-time re-index in the MCP README. *Mode: manual QA — re-index agentzero, confirm one
  `engram_data.db`, confirm `capability_report` + `search` work.*
- **P1-T4** — Mirror the adapter's single-file assertion as a regression test in the MCP
  bootstrap tests (the `persistence_factory.rs:460-477` shape): after `open_provider`, the
  five per-store file names must not exist. *Mode: TDD.*

**Boundary / honesty traps:** do not attempt to *merge* the five existing files into one
in place (cross-store ATTACH to fake atomicity is explicitly forbidden by AGENTS.md
`atomic-batch-ingest`). The data is regenerable; re-index instead. Do not change engram
core's default (`MultiFileDirectory` stays the core default for backward compatibility) —
override it at the MCP boundary only.

### Phase 2 — Bridge the subgraphs, deterministic floor (fixes §3.2) · medium risk

**Goal:** a bare `scan_repo` (no LLM, no distillation) yields a connected graph — concepts
link to code entities and to concepts in other documents.

Two complementary mechanisms; ship both, deterministic first.

- **P2-T1** — Generalize the cross-file resolver beyond `calls`. At
  `adapters/ingest/src/scanner.rs:428-442`, after per-file extraction, add a resolution pass
  that fills name-only object refs for `mentions` (and, later, `describes`) against the global
  `name_index` — not just `predicate == "calls"`. This alone creates **cross-document
  concept→concept** edges when a concept body names a concept defined elsewhere.
  *Mode: TDD — seed two docs whose concepts co-reference each other; assert a cross-doc
  `mentions` edge with a resolved `object.id`.*
- **P2-T2** — Concept→code `describes` bridge. When a concept (or its doc chunk) lexically
  references a code-symbol name present in the global `name_index`, emit a `describes` edge
  from the concept to that code entity. Start with **high-precision** matching: exact
  symbol-name word-boundary match in the chunk text, *or* a `MarkdownChunker` heading anchor
  equal to a symbol name. Co-occurrence-in-prose is opt-in (noisy) and kept behind a flag.
  *Mode: TDD — a `.md` section titled `# remember` plus a `fn remember()` produces a
  `concept(remember) -[describes]-> function(remember)` edge.*
- **P2-T3** — Wire `MarkdownChunker` into the scanner. Replace the `PlainTextChunker` for
  `.md`/`.markdown` at `scanner.rs:153-155` so headings become first-class anchors (the signal
  P2-T2 keys on). Keep `PlainTextChunker` for `.txt`/`.rst`/etc. *Mode: goal-based — a `.md`
  file yields `DocumentSection` chunks with heading anchors.*
- **P2-T4** — Update the `engram-distill` skill to *require* (not just encourage) bridging:
  after `scan_repo`, the skill lists code-entity names (via `search` / `KnowledgeQuery`) and
  emits at least one `across`-predicate edge (`describes`/`realized_by`/`governs`) per concept
  that maps to a scanned artifact. This is the "agentic way" enrichment on top of the
  deterministic floor. *Mode: manual QA — run the skill against a doc + a scanned repo;
  confirm a `concept realized_by function` edge lands and is traversable from both ends.*

**Boundary / honesty traps:** keep the bridge **additive** — new edges only; never drop
existing `calls`/`belongs_to`/`mentions` semantics. Do not change the v1 contract fields or
generated TypeScript types (`sqlite-knowledge-graph` invariant). The deterministic bridge is
*lexical* and best-effort — surface its precision limits in the MCP README rather than
claiming semantic correctness. Predicate strings stay free-form (`KnowledgeRelationship`); do
not introduce a predicate enum that would constrain the agentic layer.

### Phase 3 — Verify end-to-end via the visualizer · low risk

**Goal:** prove the graph is connected and the storage is single-file by rendering it.

- **P3-T1** — Re-index agentzero into a fresh single-file `engram_data.db`, then drive
  `engram-viz` over it. Confirm: (a) one DB file, (b) concept nodes now render links to code
  nodes, (c) cross-document concept↔concept traversal works. Record the observed output
  (not internal state). *Mode: visual / manual QA.*
- **P3-T2** — Add a graph-connectivity smoke to the MCP tests: after `scan_repo` over a
  fixture that mixes code + a markdown section sharing a symbol name, assert the concept and
  the function are in the same connected component (reachability via the new `describes`
  edge). *Mode: TDD.*

---

## 5. Sequencing & dependencies

```
Phase 1 (storage)  ── P1-T1 ──► P1-T2 ──► P1-T3 (re-index) ──► P1-T4
                                              │
Phase 2 (graph)    P2-T1 ──► P2-T2 ──► P2-T3   (P2-T4 parallel, skill-only)
Phase 3 (verify)   depends on P1-T3 + P2-T2  ──► P3-T1, P3-T2
```

Phase 1 and Phase 2 are independent and could proceed in parallel; Phase 3 needs both. The
recommended order is 1 → 2 → 3 because Phase 1 is the smallest, highest-certainty change and
immediately removes the cross-consumer incompatibility, while Phase 2 is the meatier design
work that benefits from a stable storage baseline.

---

## 6. Risks & open questions

- **Deterministic bridge precision.** Lexical concept→code matching will produce false
  positives (e.g. a concept "render" matching an unrelated `fn render`). Mitigation: start
  with heading-anchor + exact symbol-name matches only (P2-T2); keep prose co-occurrence
  behind a flag. *Open Q: is heading/anchor-only precision acceptable, or do we want a
  tunable match threshold?*
- **Re-index cost.** `knowledge.db` is 131 MB; a full re-index of agentzero takes minutes.
  Acceptable as a one-time migration, but worth confirming the scan is idempotent across runs
  before automating it.
- **Version skew.** The MCP is built from mem-alpha's workspace `core/integration`; the
  agentzero adapter is built from a git checkout of engram. The `SqliteStorageLayout` +
  `with_sqlite_storage_layout` API is stable across both today, but a single-file DB written
  by one version must remain readable by the other. *Open Q: pin the adapter's engram
  checkout to a known-compatible ref, or add a cross-consumer conformance test.*
- **`MarkdownChunker` readiness.** It exists but is untested in the scanner path (P2-T3).
  Verify it preserves the chunk kinds + line-span provenance the `docs` lane relies on.
- **SurrealDB.** Out of scope per the user's direction; this plan is SQLite-only. Any
  SurrealDB deltas are tracked separately and reconciled later.

---

## 7. What "done" looks like

- `~/.engram/agentzero/` contains exactly **one** `engram_data.db` (the five names absent).
- That same DB is openable by the agentzero gateway without format errors.
- A `scan_repo` over a mixed code + markdown fixture yields a **single connected component**
  (concept and function reachable from each other via `describes`).
- `engram-viz` renders concept↔code links (no dead-end concept nodes).
- The `engram-distill` skill emits cross-layer `across` edges as a required step, verified by
  a manual run + a connectivity smoke test.
