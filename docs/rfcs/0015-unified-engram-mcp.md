# RFC-0015: Unified Engram MCP — codegraph + memory + context-packets over one provider
<!-- Delivers RFC-0012 (codegraph layer) + RFC-0013 (context-packet framework) + the
generic memory/KG surface as ONE agent-facing MCP server; supersedes the two interim
servers. Ontology/taxonomy are multi-layer and passed as MCP launch config. -->

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps *(solo project)*
- **Date opened:** 2026-07-28
- **Decision weight:** standard
- **Related:** RFC-0012 (code-structural graph layer), RFC-0013 (context-graph packets), RFC-0014 (canonical KG identity), ADR-0008 (OntologyRepository), ADR-0009 (retrieval-composition seam), ADR-0022 (engine neutrality / surface parity), ADR-0020 (entity-kind vocabulary), `docs/domain-data-model.md`

## Reviewer brief

- **Decision:** Replace the two interim MCP servers (`codegraph/mcp-server`, `memory/mcp-server`) with **one unified agent-facing server** (`engram-mcp`) that routes every tool through a single `EngramProvider`, fuses code intelligence + generic memory/KG + multi-layer ontology into one per-project store, and delivers RFC-0013's `ContextSubgraph` packet as a first-class tool.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - New crate `mcp/engram-mcp` (binary `engram-mcp`): thin JSON-RPC loop + tool registry + focused per-group modules.
  - A consolidated **~20-tool functional surface** (down from the ~38 granular tools RFC-0012 M5 envisioned; raw graph primitives stay as library functions, composed inside functional tools).
  - Three ingestion modes: `scan_repo` (treesitter, code), `index_docs` (new `MarkdownChunker`, docs/notes), `store_knowledge` (agent-extracted structured knowledge — the distillation write).
  - **Multi-layer ontology + taxonomy passed as MCP launch config** (`--ontology`/`--taxonomy`, files or inline env), backed by the now-durable `OntologyRepository` (ADR-0008) + `TaxonomyRepository` + `HierarchyRepository`.
  - Deprecation of the two interim servers (kept building one release for transition, then removed).
- **Affected surface:** new `mcp/` workspace area; `adapters/ingest` (new `MarkdownChunker`); `AGENTS.md` repo-shape table; **no `core/domain` contract change required** for v1 (reuses RFC-0013's `ContextSubgraph`/`ontologyClassRefs` and ADR-0008's ontology port as-is). Optional additive `EntityKind` values (business/domain) ride ADR-0020 + the RFC-0013 framework/content-boundary ADR.
- **Stakes:** moderately costly-to-reverse (deprecates two shipped servers; new tool catalogue), but additive at the contract level — no data migration, no core rewrite. The riskiest part is the tool-surface consolidation (D2), which revises an aspect of RFC-0012's accepted M5 plan.
- **Review focus:** (1) **one server vs two** and the deprecation path (D1); (2) the **tool-surface consolidation** revising RFC-0012 M5 (D2); (3) **ontology/taxonomy as MCP launch config** and the multi-layer model vs the framework/content boundary (D3); (4) **distillation = agent-side extraction** (D4) — no LLM inside the server.
- **Not in scope:** domain ontology/taxonomy **content** (consumer-loaded, per RFC-0013 Q1); server-side LLM extraction (explicitly rejected — D4); a reusable `engram-mcp-core` library for third-party MCPs (YAGNI until a second consumer appears); SurrealDB backend parity (deferred — the SQLite adapter is the v1 target); enforced (write-rejecting) ontology validation (stays advisory per ADR-0008); replacing the deterministic ingestor; auto-promoting agent-extracted facts to beliefs (consolidation stays an explicit `consolidate` step).

## The ask

**Recommendation (BLUF):** Approve a **single unified `engram-mcp` server** that delivers the already-accepted codegraph layer (RFC-0012) and context-packet framework (RFC-0013) plus the generic memory/KG surface to agents over one `EngramProvider`, with multi-layer ontology + taxonomy supplied as MCP launch config. Record seven ratified-by-dialogue decisions (D1–D7) and a phased spec sequence.

**Why now (SCQA):**
- *Situation:* Engram today ships **two siloed MCP servers**. `engram-codegraph-mcp` (23 tools) bypasses `EngramProvider` and opens `SqlKnowledgeStore` directly under `workspace:"codegraph"`; `engram-memory-mcp` (6 tools) goes through the full `EngramProvider::open` bootstrap under `workspace:None`. They cannot see each other's data. Separately, RFC-0012 (codegraph) and RFC-0013 (context packets) are accepted, `OntologyRepository` is now durable (ADR-0008), and the retrieval-composition seam (ADR-0009) already emits a `ContextPayload`.
- *Complication:* The two servers are siloed **by accident, not by necessity** — the substrate for one unified server already exists in `EngramProvider` (it exposes both memory and knowledge handles). Meanwhile the codegraph tool surface is a bag of ~23 graph-algorithm names an agent must orchestrate itself, there is no Markdown/doc ingestion, no first-class context-packet tool, and ontology/taxonomy are not yet reachable from the agent surface as configurable layers.
- *Question:* Do we unify into one server that delivers codegraph + memory + context-packets over a fused per-project store, with multi-layer ontology/taxonomy as MCP config — and retire the two interim servers?

**Decisions requested** (ratified in design dialogue; recorded here for the record):

| ID | Question | Decision | Why | Decide by |
| --- | --- | --- | --- | --- |
| D1 | One unified server, or keep two? | **One** — new `engram-mcp`, supersedes both | `EngramProvider` already exposes memory + knowledge; two servers can't share scope. Routing through one provider is the only boundary-clean path (ADR-0022). | RFC acceptance |
| D2 | Tool surface: ~38 granular primitives (RFC-0012 M5) or consolidated functional tools? | **Consolidated** — ~6 functional code-intel composites; raw primitives stay as library functions | Agents want "what breaks if I change X", not "run betweenness centrality". Compose primitives inside goal-oriented tools. **Revises RFC-0012 M5's tool catalogue.** | RFC acceptance |
| D3 | Where do ontology/taxonomy live and how are they supplied? | **Multi-layer, passed as MCP launch config** (`--ontology`/`--taxonomy`); backed by ADR-0008 `OntologyRepository` + taxonomy/hierarchy | RFC-0013 framework/content boundary: engram ships mechanism, consumer loads content. Per-instance config → each project its own layers. | RFC acceptance |
| D4 | Where does the LLM extraction ("distillation") live? | **Agent-side** — an agent skill (the MCP *client*) extracts and writes via `store_knowledge`; the server **never** calls an LLM | Keeps engram deterministic; models stay out of the server (boundary rule). Aligns with RFC-0013 D5's "extraction behind ports" but pushes it to the client. | RFC acceptance |
| D5 | How do code and distilled knowledge share the store? | **Fused per project** — `workspace` = project; one searchable space; `recall` fuses by default with a `lanes` filter | "Best of both worlds" = ask once, get code + concepts + docs together. | RFC acceptance |
| D6 | First-class doc ingestion? | **Yes** — `index_docs` via a new `MarkdownChunker` in `adapters/ingest`, peer to treesitter | Docs that relate to code are half the value; chunkers belong in the ingest adapter. | RFC acceptance |
| D7 | Deprecation of the two interim servers? | **Transitional** — kept building one release, migration note, then removed | Gives clients a migration window; avoids stranding users. | RFC acceptance |

## Problem & goals

**Problem.** An agent working in a repository today must talk to two separate MCP servers that cannot see each other's data, must orchestrate two dozen graph primitives to get a useful answer, cannot ingest the Markdown docs that explain the code, cannot retrieve a coherent task-typed context packet in one call, and cannot have the server classify knowledge against a project-specific business/domain ontology. The accepted designs that would fix this (RFC-0012, RFC-0013, ADR-0008) are not yet delivered as a single coherent agent surface.

**Goals.**
1. **One server, one provider, one store per project** — code intelligence + generic memory/KG + multi-layer concepts in a single fused, recall-able space.
2. **Functional, agent-usable tools** — a small set of goal-oriented tools that compose the raw graph/retrieval primitives.
3. **Deliver RFC-0013's `ContextSubgraph` packet** as a first-class `get_context` tool — task-typed, graph-expanded, budgeted, provenance-stamped.
4. **Multi-layer ontology + taxonomy as MCP launch config** — technical / business / domain / custom layers, consumer-loaded, grounded in the durable ontology/taxonomy/hierarchy ports.
5. **Agent-side distillation** — an agent skill extracts knowledge and writes it; the server stays deterministic and LLM-free.
6. **Stay engine-neutral and contract-additive** — everything routes through `EngramProvider`; no `core/domain` break required for v1.

**Non-goals** (deliberately dropped).
- **No domain ontology/taxonomy content in core** — classes, predicates, business/domain vocabularies are consumer-loaded via config (RFC-0013 Q1 / framework-content boundary). A minimal generic default ships in the binary so it runs zero-config.
- **No server-side LLM extraction.** The MCP server never calls a model. Extraction is the agent's job (an agent skill); the server only stores what it is given.
- **No reusable `engram-mcp-core` library** for third-party MCPs in v1 — YAGNI until a confirmed second consumer (Zbot consumes engram as a *library*, not via MCP).
- **No SurrealDB backend parity in v1.** New exposure lands on the SQLite adapter. The Surreal backend may be out of sync with capabilities added here; reconciling those deltas is explicitly deferred to future work.
- **No enforced ontology validation.** Stays advisory per ADR-0008.
- **No auto-promotion of agent-extracted facts to beliefs.** Consolidation remains an explicit `consolidate` step.
- **No new vector store, no general policy language.**

## Proposal

### Architecture & crate structure

New top-level workspace area `mcp/` (a unified server spans memory + knowledge + code + analytics, so it belongs under neither `codegraph/` nor `memory/`). Crate `engram-mcp`, binary `engram-mcp`:

```text
mcp/engram-mcp/
  Cargo.toml   # bin engram-mcp; deps: engram-integration(sqlite), engram-memory,
               #   engram-knowledge, engram-belief, engram-hierarchy, engram-ingest,
               #   engram-codegraph-{queries,temporal}, engram-store-lexical,
               #   engram-domain, engram-runtime
  src/
    main.rs          # thin: parse config -> Server::run()
    server.rs        # JSON-RPC 2.0 stdio loop + dispatch via ToolRegistry
    registry.rs      # ToolRegistry: one record per tool (name, schema, handler);
                     #   single source of truth for tools/list + capability_report
    bootstrap.rs     # EngramProvider::open(config); load scope + ontology/taxonomy
    config.rs        # McpConfig: storage path, project, scope policy,
                     #   ontology/taxonomy (file paths or inline), embedding provider
    scope.rs         # project-as-workspace resolution + lane helpers
    protocol.rs      # JSON-RPC envelope, tool-schema helpers, typed errors
    tools/
      mod.rs         # register_all()
      scan.rs        # scan_repo (treesitter)                 [from codegraph]
      docs.rs        # index_docs (MarkdownChunker)           [NEW]
      knowledge.rs   # put_entity, put_relationship, store_knowledge (distill write) [NEW bulk]
      memory.rs      # write_memory, forget                   [from memory]
      recall.rs      # recall (fused, lanes) + get_context (ContextSubgraph) [extended + RFC-0013]
      consolidate.rs # consolidate                            [from memory]
      analytics.rs   # symbol_context, change_impact, code_health, architecture, api_topology, whats_changed
      config_tools.rs# ontology_read, taxonomy_read
      capability.rs  # capability_report (unified)
```

**Two structural fixes baked in by design** (the biggest reconciliations the codegraph/memory explorer identified):
- **One provider, no bypass.** Every tool draws handles from a single `EngramProvider::open`. The codegraph tools stop opening `SqlKnowledgeStore` directly and use the provider's knowledge/graph handles. This is what makes the merge engine-neutral and surface-parity compliant (ADR-0022).
- **No god-`main.rs`.** The current servers are 500–850-line monoliths; the registry + per-group modules replace that. `tools/list` and `capability_report` both read from the registry, so the "README says 17, install.sh says 14, actual 23" staleness cannot recur.
- **Exposure & storage discipline.** Every new capability is exposed **through `engram-integration`** (`EngramProvider` / `EngramConfig` / `CapabilityReport`) — the SDK facade — never by reaching into adapter internals, and is reflected in `CapabilityReport` (surface parity, ADR-0022). Storage targets the **SQLite adapters** (`engram-store-knowledge-sqlite`, `engram-store-sqlite`, `engram-store-lexical`, …). The **SurrealDB backend is out of scope for v1** and may be out of sync with capabilities added here; reconciling those deltas is deferred (see Non-goals, Q5).

### Tool surface (~20 functional tools)

| Group | Tools | Origin |
| --- | --- | --- |
| **Ingest** | `scan_repo` (code→treesitter), `index_docs` (md→MarkdownChunker, NEW), `store_knowledge` (agent-extracted bulk write, NEW) | codegraph / new / new |
| **Memory lifecycle** | `write_memory`, `forget`, `consolidate` | memory |
| **Retrieve** | `search` (lexical/BM25, lanes), `recall` (semantic/hybrid fused, lanes), `get_context` (task-aware `ContextSubgraph` packet) | codegraph / memory / RFC-0013 |
| **Code intelligence (consolidated)** | `symbol_context`, `change_impact`, `code_health`, `architecture`, `api_topology`, `whats_changed` | codegraph (composed) |
| **KG write** | `put_entity`, `put_relationship` | memory |
| **Config** | `ontology_read`, `taxonomy_read` | new |
| **Meta** | `capability_report` | codegraph (unified) |

**What the consolidated code-intel tools compose (raw primitives stay as library functions in `engram-codegraph-queries` / `engram-graph-analytics`):**
- `symbol_context` — callers, callees, community for one symbol.
- `change_impact` — blast radius + dependency paths + execution flow from a change site.
- `code_health` — dead code (zero in-degree) + complexity hotspots, ranked.
- `architecture` — central symbols (PageRank) + bridges (betweenness) + communities (Louvain) + entry points + repo stats.
- `api_topology` — endpoints + call sites + cross-service matches.
- `whats_changed` — temporal recency + impact + growth direction (folds all five `temporal_*`).

**New/changed specifics:**
- `store_knowledge` — the distill-write tool the agent skill calls after extraction. Takes bulk `{facts, entities, relationships}` in one call, writes through memory + knowledge handles, provenance-stamped (agent skill + source). Best-effort-surfaced per the `atomic-batch-ingest` invariant.
- `get_context` — **delivers RFC-0013's `ContextSubgraph`** (D1): given a `focus` (symbol/file/concept/free-text) + optional `intent` (edit/implement/explain/review/debug) + `layers` filter + `token_budget` + `depth`, it composes a connected subgraph — code neighborhood + governing concepts (all layers) + doc chunks + memories + beliefs — via engram's `compose_context` + `ReciprocalRankFusion` (reused, not reimplemented, per the unified-recall invariant). Agent-agnostic: a coding agent focuses a symbol + technical layer; any agent focuses a concept + relevant layers.
- `recall` — default-fused across memory + knowledge/code + beliefs; optional `lanes` restricts.
- `put_entity` — stop hard-coding `EntityKind::Concept`; honor the `kind` arg against the extended vocabulary (ADR-0020) + ontology class refs (RFC-0013 D3).

### Data & scope model (fused per project)

- **One file-backed SQLite store** via `EngramProvider` (no in-memory default — distilled knowledge is persistent; also fixes the codegraph in-memory-default divergence).
- **Scope = project.** `workspace` ← the project/repo (from `scan_repo` path or an explicit `project` arg); `tenant` ← configured host/agent; `subject` ← optional agent identity (RFC-0014 / engram's identity capability). Treesitter code entities and agent-distilled knowledge land in the **same workspace** → one searchable space → fused recall works. Exact `Scope` field mapping finalized at implementation against `engram-domain::Scope`.
- **Two node kinds, linked:** *concept nodes* (agent-extracted from unstructured knowledge, typed by ontology) and *technical-artifact nodes* (treesitter-extracted symbols/files). Cross-layer predicates (`realized_by`, `described_in`, `governs`, `derived_from`, …) bridge them, generalizing RFC-0013 D3's `instance_of` / `ontologyClassRefs` link to a full concept↔concept↔artifact graph.

### Multi-layer ontology + taxonomy as MCP launch config

- **Layers coexist:** technical (Service, API, Module, DataModel, Endpoint, Invariant…), business (Customer, Order, Product, BusinessRule, Process, Kpi…), domain (problem-space; e.g. fintech: Account, Transaction, Ledger…), and custom. A *layer* is just a named set of classes + predicates, so the mechanism for one vs N is identical (the richness is in configured content, not the MCP).
- **Passed at server launch** via `--ontology`/`--taxonomy` (file paths) or inline env (`ENGRAM_ONTOLOGY`/`ENGRAM_TAXONOMY`):
  ```jsonc
  "engram": {
    "command": "engram-mcp",
    "args": ["--storage","/path/store","--project","myapp",
             "--ontology","/path/ontology.toml","--taxonomy","/path/taxonomy.toml"]
  }
  ```
  ```toml
  # ontology.toml — multi-layer
  [[layer]]
  name = "technical"
  classes = ["Service","Api","Module","DataModel","Endpoint","Invariant"]
  [[layer]]
  name = "business"
  classes = ["Customer","Order","Product","BusinessRule","Process","Kpi"]
  [[layer]]
  name = "domain"
  classes = ["Account","Transaction","Ledger","Instrument"]
  [predicates]
  within  = ["depends_on","part_of","is_a"]
  across  = ["realized_by","governs","describes","derived_from"]
  ```
  Multiple `--ontology` files merge (so engineering and business teams can own different files). Taxonomy is per-layer (broader/narrower + labels).
- **Zero-config default:** a baked-in minimal generic ontology + taxonomy ships in the binary; passing configs overrides/extends.
- **Backed by durable ports:** `OntologyRepository` (ADR-0008, now implemented — corrects the stale "deferred" note in AGENTS.md), `TaxonomyRepository`, `HierarchyRepository` (LeanRAG layered summaries). `ontology_read`/`taxonomy_read` expose the *active* config; runtime reload = server restart (deferred).
- **Ontology modeled as configured content, not a core engine** — consistent with RFC-0013's framework/content boundary. Enforced/rejecting validation stays advisory (ADR-0008).

### Distillation = agent-side extraction

The agent skill (running in the MCP *client* — Claude Code / Codex) extracts entities/relationships/facts from source material (docs, transcripts, code) and writes them via `store_knowledge` / `put_entity` / `put_relationship`. The MCP server **never calls an LLM**. This is cleaner than RFC-0013 D5's adapter-behind-ports model (it pushes extraction fully to the client) and keeps the server deterministic. The agent skill itself is a separate artifact (out of scope for this RFC) but is the intended primary caller; the chunked docs from `index_docs` are available for it to read and extract from.

## Options considered

*D1 — server topology (axis: how many servers).*
- **(a) One unified server** ✅ — `EngramProvider` exposes memory + knowledge; one scope; one deploy.
- (b) keep two + add bridges — preserves the silo the user wants dissolved; scope isolation blocks fused recall.
- (c) new standalone superset alongside the two — additive but leaves three servers to maintain; rejected for v1.

*D2 — tool surface (axis: granularity).*
- **(a) consolidated functional composites (~6 code-intel)** ✅ — agents ask goals, not algorithms; primitives stay as library functions.
- (b) ~38 granular primitives (RFC-0012 M5 as-written) — maximally expressive but forces agents to orchestrate; poor UX.
- (c) one uber-tool — loses composability and observability.

*D3 — ontology/taxonomy supply (axis: how content reaches the server).*
- **(a) MCP launch config (files/inline), multi-layer** ✅ — portable across every MCP host; per-instance; matches framework/content boundary.
- (b) hardcoded in core — violates RFC-0013 Q1.
- (c) runtime governance tool only — no zero-config default; harder to bootstrap.

*D4 — distillation location (axis: where the LLM runs).*
- **(a) agent-side (agent skill extracts, server stores)** ✅ — server stays deterministic and LLM-free; cleanest boundary.
- (b) server-side auto-distill (Zbot-style) — one-call UX but puts an LLM inside an engram server by default; conflicts with the "models behind injected providers, never in core/server" rule.
- (c) hybrid (both) — deferred; revisit if a provider is wired.

*D6 — doc ingestion (axis: chunker placement).*
- **(a) `MarkdownChunker` in `adapters/ingest`, peer to treesitter** ✅ — chunkers belong in the ingest adapter; keeps core clean.
- (b) reuse `PlainTextChunker` only — loses heading/section structure that makes doc chunks retrievable and ontology-mappable.

## Risks & what would make this wrong

**Pre-mortem (assume it shipped and failed).**
- *Tool consolidation hides a primitive an advanced user needed.* Mitigation: raw primitives remain callable as Rust library functions; a power-user escape-hatch tool can be added later without redesign. Falsifiable: name the primitive that no composite covers.
- *Deprecating the two servers stranding a user.* Mitigation: transitional build for one release + migration doc; the new server's surface is a functional superset.
- *Multi-layer ontology config grows into a general policy engine.* Mitigation: ontology stays class+predicate vocabulary (ADR-0008 advisory); a later RFC would have to propose expanding it.
- *Fused-per-project scope leaks across projects.* Mitigation: `workspace` is the isolation boundary (RFC-0008); scope-isolation conformance tests must show records from unrelated projects never blend.
- *`MarkdownChunker` is the wrong split granularity.* Mitigation: header/section-based with configurable size/overlap; `engram-eval` recall fixtures for doc chunks.

**Key assumptions (falsifiable).**
- "`scan_repository` accepts the provider's knowledge/graph handles as its `KnowledgeRepository + KnowledgeGraphRepository`" — verified by the codegraph/memory explorer (it takes any such store).
- "`OntologyRepository` is durable and writable" — true per ADR-0008 (Accepted); contradicts the stale AGENTS.md invariant text, which this RFC flags for update.
- "Routing codegraph tools through `EngramProvider` changes no runtime behavior" — the existing codegraph tests are the regression net (per the `sqlite-consolidation` relocation invariant pattern).

**Drawbacks.**
- One server to rule them all raises the stakes of any tool-surface mistake; mitigated by the registry making the surface explicit and tested.
- Markdown chunking adds an ingest path to maintain.
- Deprecation is real work (docs, migration, eventual removal).

## Evidence & prior art

**Repo precedent (read this session).**
- RFC-0013 (Accepted) — `ContextSubgraph` packet (D1), `ontologyClassRefs`+`instance_of` (D3), framework/content boundary (Q1), agentic population behind ports (D5). `get_context` delivers this; multi-layer ontology respects the boundary.
- RFC-0012 (Draft) — codegraph on-top layer; M5 planned a codegraph MCP (~38 tools). This RFC supersedes the interim `codegraph/mcp-server` and **revises M5's tool catalogue toward consolidation (D2)**. M5's open Q2 (MCP host: Rust vs TS) is resolved here as **Rust** (parity + perf + single-binary deploy).
- ADR-0008 (Accepted) — `OntologyRepository` is durable in `engram-store-knowledge-sqlite`; advisory validation; the basis for multi-layer ontology.
- ADR-0009 — retrieval-composition seam; `compose_context` emits `ContextPayload` → `get_context`'s packet.
- ADR-0022 — engine neutrality + surface parity; mandates routing through `EngramProvider` and parity across surfaces.
- ADR-0020 — entity-kind vocabulary extension; the mechanism for technical (and, under the framework/content ADR, business/domain) entity kinds.
- RFC-0014 — canonical KG identity/consolidation; the `subject`/identity dimension of scope.

**External prior art** (surveyed in the repo's research corpus under RFC-0012/0013): PuppyGraph "context graph" (KG + time + provenance + governance + traces, packet = serialized subgraph); Microsoft GraphRAG (extraction + community summaries + subgraph retrieval); MCP `resources`/config conventions (argv + env as the portable server-config mechanism across Claude Code / Codex / Cursor).

## Open questions

- **Q1 — v1 tool phasing.** Ship the full ~20-tool surface in one spec, or phase: **generic core first** (ingest via `store_knowledge`/`index_docs`, memory, recall, consolidate, ontology/taxonomy config) → **code intelligence second** (scan_repo + the six composites) → **`get_context` third** (composes the rest)? *Default: phased, three specs.* Owner: Approver.
- **Q2 — business/domain `EntityKind` values.** Do layered ontologies need additive `EntityKind` values (e.g. `business_capability`, `domain_concept`), or are concept types carried entirely via `ontologyClassRefs` (RFC-0013 D3) without new enum values? *Default: via `ontologyClassRefs` only for v1; new enum values gated by the RFC-0013 framework/content-boundary ADR.* Owner: Approver.
- **Q3 — scope fields.** Exact mapping of project/agent/tenant onto `engram-domain::Scope` (workspace/tenant/environment/subject) — finalized in the spec against the domain type. *Default: workspace=project, tenant=host, subject=agent identity.*
- **Q4 — `get_context` intent weighting.** How strongly should `intent` (edit/implement/explain/review/debug) re-weight lanes/layers, and is it configurable? *Default: light built-in weighting; configurable later.*
- **Q5 — SurrealDB deltas.** New exposure targets the SQLite adapter. When/how do we reconcile the Surreal backend — feature-by-feature as capabilities land, or a dedicated parity pass later? *Default: deferred; track deltas as they arise, reconcile in a dedicated future effort.* Owner: Approver.

## Follow-on artifacts

- **ADR (to file on acceptance):** ADR-NNNN — Unified Engram MCP (one server supersedes two; tool-surface consolidation revises RFC-0012 M5; ontology/taxonomy as MCP launch config).
- **Doc updates:** `AGENTS.md` "Target Repository Shape" (add `mcp/engram-mcp/`, mark `codegraph/mcp-server` + `memory/mcp-server` deprecated→removed); correct the stale "OntologyRepository is deferred" invariant text (ADR-0008 supersedes it); `docs/rfcs/README.md` table.
- **Specs (phased; one at a time through `new-spec` → `work-loop`):**
  ```
  Phase 1 — generic core: crate skeleton + registry + EngramProvider bootstrap +
            scope config + ontology/taxonomy launch config + store_knowledge/
            put_entity/put_relationship/write_memory/forget/recall/consolidate +
            index_docs (MarkdownChunker)
  Phase 2 — code intelligence: scan_repo (route through provider) + the six
            consolidated composites + search
  Phase 3 — context packets: get_context (ContextSubgraph via compose_context) +
            capability_report + deprecation/migration of the two interim servers
  ```
  Phase 1 unblocks 2 and 3; Phase 3 depends on 1 (and benefits from 2).

## Verification commands (per `AGENTS.md`)

```bash
cargo fmt --all
cargo check --workspace
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/check-engine-neutrality.sh   # must pass: everything routes through EngramProvider
```

Plus: per-phase `engram-eval` fixtures — fused recall across code+docs+concepts; doc-chunk recall; scope-isolation (unrelated projects never blend); ported-tool parity against the old servers' tests.
