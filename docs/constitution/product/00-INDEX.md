# Product sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.1.

## Charter

What engram promises: identity, the four substrates (memory / source-grounded knowledge / derived belief / code intelligence), observable write-recall-forget guarantees, freshness semantics, honest capability status.

## Knowledge — what engram IS

Engram is a contract-first, local-first **agentic memory and knowledge engine**.
Its customer is an agent/host that must remember across sessions, retrieve
grounded context, revise evidence-backed beliefs, and understand code without
confusing observations with truth.

**The four substrates (distinct — never merged):**
1. **Memory** — governed experiential records: write/recall/forget with scope,
   policy, lifecycle events, tombstones (not SQL deletes)
2. **Knowledge** — source/document/chunk-grounded graph: entities, relations,
   taxonomy + ontology packs (FIBO, schema.org, +33); source-bound supersession
   retires stale assertions
3. **Belief** — derived, revisable stances with valid-time ≠ record-time;
   contradictions are reviewable signals, never silent overwrites
4. **Code intelligence** — generation-bound indexes (engram-code tree-sitter):
   symbols, typed edges, blast radius; branch-bound per project

Rust owns all deterministic behavior behind ports; TypeScript (N-API + SDK)
owns ergonomics; one unified MCP server (`mcp/engram-mcp`) is the agent
surface; `packages/launcher` ships the installable agent bundle. Retrieval
fuses all substrates into a bounded, explainable ContextPayload.

**Engram is NOT:** an LLM host, a vector database, an app server, or a hosted
control plane. Vector search is a retrieval mechanism, not the identity.
Full product document absorbed below.

## Source: product/engram.md (absorbed)

# Engram — Product Overview

> The single consolidated view of the engram product **today**, synthesized
> from the shipped implementation (the historical feature specs were consolidated
> here). The code map is the
> [architecture overview](../architecture/overview.md); direction is the
> [roadmap](roadmap.md). This is a *living* doc — drift is a bug.

**Status:** pre-1.0. SQLite is the reference backend and is production-shaped
across the core capabilities. A second engine (SurrealDB) and the agent-host
adapter are in flight. (Legend: ✅ shipped · 🚧 draft / in-progress)

## What engram is

Engram is a **contract-first agentic memory & knowledge layer**: a Rust core
that owns deterministic memory, knowledge-graph, bi-temporal belief, hierarchy,
retrieval, and consolidation behavior, exposed through TypeScript bindings and
MCP servers so any agent can read and write it. It is built for **agents that
make decisions** — agents that remember across sessions, ground answers in
sources, form and revise beliefs, and recall under governance — not for BI or
analytics over a warehouse.

It separates *what is stored* (rich memory + a source-grounded knowledge graph +
taxonomy/ontology) from *how it is retrieved* (a multi-mode, fused,
policy-filtered pipeline) — the same decoupling Microsoft Research's
[Memora](https://www.microsoft.com/en-us/research/blog/memora-a-harmonic-memory-representation-balancing-abstraction-and-specificity/)
argues lets agents scale to long-horizon tasks without re-reading their history.

## Who it's for

- **Long-horizon agents** that need durable, structured memory across sessions.
- **Knowledge-grounded agents** that answer from source-grounded facts, not
  hallucinations — over code, docs, or any ingestible corpus.
- **Decisioning agents** that decide over a *belief* substrate (what the agent
  believes is true, as-of T) rather than a raw log.
- **Governed/enterprise agents** where policy, provenance, scope, and auditable
  recall are required.

## Capabilities

### Memory lifecycle ✅
Durable agent memory as a first-class domain: typed `MemoryRecord` (kind,
content, scope, provenance, policy, status, links) with an explicit lifecycle
(`active → archived → redacted → forgotten → expired`) and append-only
`MemoryEvent`s — auditable and replayable, not a mutable log. Entity **cue
anchors** are auto-extracted at write time for multi-hop retrieval. **Forget**
is a domain concept (delete / redact / tombstone / archive), not a DB delete.
*Specs: memory-cue-anchors, memory-knowledge-boundaries, forget-mode-contract-examples, memory-mcp, retire-memory-inmem.*

### Source-grounded knowledge graph ✅
`KnowledgeSource → SourceDocument → KnowledgeChunk → KnowledgeEntity /
KnowledgeRelationship`, bounded by a named `KnowledgeGraph` — source-grounded,
never free-floating. Ingest from filesystem/git via the dedicated
**`engram-code` behavior crate** (ADR-0028): **tree-sitter AST** symbol
extraction (10+ languages) with **receiver-qualified identities**
(RFC-0020 Phase 2 — `Impl::method`, path/repo as disambiguators) resolved
through a **scope-wide multi-candidate symbol table** (no silent collision
overwrites); **typed structural edges** — `calls`, `imports`, `contains`,
`extends`, `implements`, `routes_to` (the closed `CodeEdgeKind` set) — with
**framework patterns** (Express/NestJS/FastAPI/Flask/Spring routes + React
callbacks); cross-file references either resolve or land in the
**unresolved-refs ledger** (re-attempted by the ingest **orphan sweep**
when the defining file arrives later); code chunks carry **docstring +
signature** for the lexical lane. Query surface: `file_dependencies` (the
file-level import graph) and `explore` (natural-language seeded bounded
subgraph) on every transport — facade, N-API, `@engram/node`, both MCP
servers — reported as the `code_graph` capability. Deterministic
throughout; opt-in **LLM** entity/relationship extraction stays TS-side.
Re-ingest **converges** the graph (retraction, ledger rows included).
**Cross-repo linkage** via stable source keys + shared OpenAPI
contract nodes. *Specs: knowledge-ingestion, knowledge-graph-extractor, ast-symbol-extraction, scale-repo-ingestion, background-repo-indexer, structured-repo-identity, contract-first-ingestion, knowledge-graph-retraction, engram-code.*

### Project-bound code indexes ✅
A code index can bind one authorized scope and opaque `RepoId` to one exact `refs/heads/...` ref.
Registration records an approved locator, binding is idempotent for the same ref, and a different ref denies with `BindingConflict`.
Bound ingestion retains immutable source snapshots and does not use caller checkout paths or `HEAD` as authority.
Generation publication is receipt-gated and fenced; status keeps the indexed OID separate from an explicit on-request ref observation.
Code reads, retrieval, graph operations, and targeted reset use the same binding authority.
Reset follows preview → owner gate → apply/resume with an exact durable journal, preservation digests, and ambiguous-ownership denial.
MCP stdio and one-shot, Rust, N-API, and TypeScript hosts expose the scoped registration, bind, status, and refresh operations.
Hosts inject `codeAuthority.launchScopes`; recipe defaults deny code operations and the legacy native bind path directs callers to the provider path.
The matrix runner records disposable SQLite/pgvector and native/MCP receipts; see [`docs/guides/operator-bound-code-runbook.md`](../operator-bound-code-runbook.md).

### Taxonomy & ontology ✅
A governed, evolving concept vocabulary (SKOS-aligned) and a typed ontology
(classes / properties / axioms) as first-class layers — turning a bag of
entities into a decision-grade, governed knowledge structure. Provenance on
concept changes; advisory validation; drift detection. *Specs: ontology-it-org.*

### Bi-temporal belief synthesis & contradiction ✅
`Belief`s are **derived, recomputable, bi-temporal** stances over evidence —
distinct from memory and source truth: synthesized from assertions,
retracted/superseded when a source invalidates, with **contradiction detection
+ reviewable resolution + contradiction-aware ranking**. Authority-aware
source-assertion reconciliation. Valid-time (when true in the world) is
separate from record-time. *Specs: belief-network, belief-contradiction-bitemporal,
source-assertion-reconciliation, reflection-operator, in-memory-contradiction-\*.*

### Hierarchy (context compression) ✅
`HierarchyNode` organizes retrievable objects into abstraction layers
(base → aggregates/summaries) so retrieval returns the right granularity
instead of N similar low-level fragments (the GraphRAG insight). Construction
is separated from navigation; durable (SQLite). *Specs: durable-hierarchy,
hierarchy-navigation, in-memory-hierarchy-aggregate-\*.*

### Retrieval composition ✅ · 🚧 (two wiring items)
Six retrieval modes — `temporal`, `cue`, `hierarchical`, `semantic` (vector),
`graph`, `keyword` (BM25) — plus **associative** (Personalized PageRank,
HippoRAG-style) and **community-summary** (GraphRAG). Candidates are fused via
**Reciprocal Rank Fusion** (configurable), optionally cross-encoder reranked
(adapter shipped; live wiring 🚧), budget-compressed and policy-filtered into a `ContextPayload` with a
full `FusionTrace` (explainability). The composition seam is storage-neutral.
*Specs: backend-agnostic-retrieval, hybrid-retrieval-fusion, associative-graph-retrieval,
community-summary-retrieval, lexical-keyword-retrieval, cross-encoder-rerank,
predictive-retrieval, temporal-cue-retrieval. 🚧 live-pipeline wiring for lexical
and cross-encoder rerank.*

### Consolidation (reflection + decay) ✅
An explicit, auditable pipeline (`ConsolidationRequest → dry-run Plan → audited
Run`): **reflection** (observations → derived beliefs), **compaction** (dedupe),
**decay** (policy-expiry + Ebbinghaus forgetting-curve), **taxonomy evolution**.
Mutating consolidation runs behind pre/post evaluation gates with full audit,
via an injected executor port (composite executors supported). *Specs:
consolidation-sleep, mutating-consolidation-gates, reflection-operator,
spaced-repetition-decay.*

### Governance — policy · provenance · scope ✅
`Policy` (visibility / retention / sensitivity / allowed-uses / expires /
delete-mode), `Provenance` (source / actor / observed_at / evidence /
derivations / confidence / method), and `Scope` (tenant + subject / workspace /
session / environment) are **required on every durable record and checked at
runtime** on write, retrieve, ingest, consolidate, and forget. Redacted records
do not leak through retrieval, links, or explanations. Cross-cutting across all
capabilities. *Specs: forget-mode-contract-examples, provenance-confidence-viz,
+ embedded in every record contract.*

### Codegraph layer (on top of engram) ✅
An on-top layer ([RFC-0012](../rfcs/)) that answers structural questions over
indexed repos: `dead_code`, `blast_radius`, `dependency_path`, central/bridge
symbols (PageRank/betweenness), communities (Louvain), and temporal scoring
(recent / impact / compound). Built on engram + a pure `engram-graph-analytics`
crate; exposed through a codegraph MCP server. Lives **on top of** engram, not
in core. *Specs: graph-analytics, codegraph-queries, codegraph-temporal.*

### Storage backends — ✅ SQLite · 🚧 SurrealDB
Engine-neutral by design ([ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md)):
one crate per backend, swapped by config. **SQLite** is the shipped reference
(memory / knowledge / belief / hierarchy / vector + consolidation glue, all in
`engram-store-sqlite`), and **pgvector** (Postgres) is the second recipe.
The former SurrealDB draft sibling was retired (ADR-0030). No cross-engine
migration (fresh store on switch).
Engine-agnostic adapters (lexical / associative / community / decay / ingest)
are shared across engines. *Specs: sqlite-consolidation, sqlite-open-options,
sqlite-file-backed-construction, pgvector-backend.*

### Rust SDK + host-SDK ✅
`EngramProvider` — the canonical Rust entry: a facade holding typed repository
handles, with `CapabilityReport` (20 capability areas), an engine-neutrality gate, an embedding-provider abstraction
(FastEmbed / Ollama), and migration/import with dry-run gating. The
**host-SDK brief (S1–S7)** ships all seven slices (each with named per-slice
deferrals linked from its spec): capability report, episode/evidence
query, atomic batch ingest, unified recall, export/import, observability, and a
non-SQLite backend-conformance proof that backend swap is config, not rewrite.
*Specs: rust-crate-integration, provider-sdk-capability-report, episode-evidence-api,
atomic-batch-ingest, unified-recall-api, export-import-api, observability-api,
backend-conformance-coverage.*

### N-API & TypeScript ✅
The N-API binding (`engram-node`) is a **transport over Rust**, not a second
implementation: JSON in/out over the same `EngramProvider` surface. The TS
workspace ships generated contracts, a native-binding package, an ergonomic
client, and framework-neutral runtime adapters. **Surface parity** — every
facade capability is reachable from TypeScript. *Specs: napi-bridge-completion,
typescript-native-surface, node-bindings-analysis, runtime-adapters,
local-runtime-examples.*

### MCP servers — ✅ memory + codegraph · 🚧 backend HTTP
Two MCP servers expose engram to any agent client (Claude Desktop, Cursor,
Copilot): a **memory MCP** (`write_memory` / `recall` / `forget` /
`put_entity` / `put_relationship` / `consolidate`, stdio — shipped) and a
**codegraph MCP** (23 tools — shipped). A backend **HTTP** MCP server
(index / search / agentic_search) is draft. *Specs: memory-mcp, mcp-server.
(The codegraph MCP server ships with the codegraph layer — no dedicated spec.)*

### Evaluation ✅
Deterministic fixtures + a regression harness: accepted recall / leakage /
budget-omission / no-result fixtures replayable by any adapter; serializable
evaluation reports for CI/CLI; code-intelligence eval suites and a warm-up
benchmark. Quality is a system behavior, measured. *Specs: accepted-retrieval-fixtures,
evaluation-report-generation, benchmark-lazy-embeddings, local-benchmark-smoke.*

### Visualization & demo — ✅ core · 🚧 polish
A web code-graph visualization workspace plus a demo app (Hono backend +
React/shadcn-ui frontend) exercising the full memory / knowledge / belief /
retrieval surface end-to-end on a durable shared SQLite. 3D graph, whole-graph
explorer, community clustering. Several UI-polish items remain draft
(kg-redesign, dashboard-tenant-view). *Specs: engram-cc, demo-reimagine,
engram-demo-app, enterprise-3d-graph, graph-explorer, demo-ui-shell.*

## Maturity at a glance

| Capability | Status |
| --- | --- |
| Memory lifecycle | ✅ shipped |
| Source-grounded knowledge graph + ingestion | ✅ shipped |
| Taxonomy & ontology | ✅ shipped (advisory validation) |
| Bi-temporal belief & contradiction | ✅ shipped |
| Hierarchy (context compression) | ✅ shipped |
| Retrieval composition (6 modes + RRF fusion) | ✅ shipped · 🚧 lexical + cross-encoder-rerank live-wiring |
| Consolidation (reflection + decay) | ✅ shipped |
| Governance (policy · provenance · scope) | ✅ shipped (cross-cutting) |
| Codegraph layer | ✅ shipped |
| Project-bound code indexes | ✅ shipped |
| Storage — SQLite (reference) | ✅ shipped |
| Storage — SurrealDB (2nd engine) | 🚧 draft |
| Rust SDK + host-SDK (S1–S7) | ✅ shipped |
| N-API / TypeScript | ✅ shipped |
| MCP — memory + codegraph | ✅ shipped · 🚧 backend HTTP |
| Evaluation | ✅ shipped |
| Visualization & demo | ✅ core · 🚧 polish |
| Agent-host adapter (AgentZero) | 🚧 draft |

> **Spec-lag note:** `codegraph-temporal` and `cross-encoder-rerank` ship in
> code, but their specs still read `Draft` — a spec-maintenance lag, not a
> capability gap.

## Architecture (in one paragraph)

A two-row memory pipeline: a **build/store path** (one-way writes — agent
ingests/writes → memory & knowledge construction → durable store, with
consolidation maintaining the store) and a **retrieve path** (bidirectional —
agent ↔ context path ↔ retrieve ↔ store), with **governance** spanning every
read and write. See the [architecture overview](../architecture/overview.md)
for the full diagram and layer responsibilities.

## Roadmap highlights

- **Now:** codegraph parity, retrieval wiring polish, demo polish.
- **Next:** SurrealDB (2nd engine), lexical live-wiring, cross-encoder rerank,
  the agent-host (AgentZero) adapter.
- **Not in scope:** being an LLM host; eager index-time embeddings; distributed
  cross-store write consistency; a second TypeScript implementation of the core.

Full direction in [`roadmap.md`](roadmap.md); user-visible changes in
[`changelog.md`](changelog.md).

## How this doc is maintained

This is a **living product view** of the shipped implementation. It rolls
features up by **capability** and tracks product-level status. When a capability
ships or its status changes, update the relevant line here in the same PR — drift
between this doc and reality is a bug. New feature work is specced in
[`docs/specs/`](../specs/); open items live in [`docs/history/backlog.md`](../backlog.md).
(See [`docs/guides/CONVENTIONS.md`](../CONVENTIONS.md) document-lifecycle.)

## See also

- [README](../../README.md) — project overview, use cases, quick start.
- [Architecture overview](../architecture/overview.md) — the pipeline + layer responsibilities.
- [Roadmap](roadmap.md) · [Changelog](changelog.md).
- [Backlog](../backlog.md) — open / deferred items.
- [ADRs](../adr/README.md) · [RFCs](../rfcs/README.md) — durable decisions + proposals.
- [Research synthesis](../research/README.md) — prior-art + concept grounding.
- Charter retired 2026-10-31 (content drifted); mission + scope live in this document.


## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| PRD-101 | Engram's product scope MUST remain governed agent memory, source-grounded knowledge, derived belief and code intelligence, delivered by the Rust engine through SDK/MCP surfaces; vector search MUST remain a retrieval mechanism, not the product identity. Engram MUST NOT become an LLM host or application server/hosted control plane without an explicit scope decision. | refines: PR-002, PR-004, CI-001; manual product/ADR review gap. | `docs/product/engram.md:15-37,241-242`; `README.md:20-31`; `docs/architecture/reference.md:16-26`. |
| PRD-102 | Memory writes MUST produce governed `MemoryRecord` state and auditable lifecycle events; forget MUST preserve the declared delete/redact/tombstone/archive semantics rather than mean an undifferentiated SQL delete. | refines: PR-001, PR-005; enforced-by: check_contracts.py (shape/examples only); runtime lifecycle fixtures. | `core/domain/src/memory.rs:83-92,185-207,219-250`; `core/memory/src/lib.rs:96-105`; `docs/product/engram.md:41-47`. |
| PRD-103 | `store_knowledge`/semantic batches MUST distinguish source-derived memory facts from graph entities/relationships, expose `BestEffort` per-step outcomes and retain succeeded steps after failure. Sourced revisions MUST use stable source identity plus a new exact-material key; an immutable retry MUST NOT be represented as a new extraction or a changed payload under the old key. | refines: PR-001, PR-005, PC-001; batch/supersession fixtures, not a dedicated registered gate. | `core/integration/src/batch.rs:1-30`; `.agents/skills/engram-distill/SKILL.md:76-89,119-127`; `adapters/integration/tests/batch_ingest.rs:252-303,353-405`. |
| PRD-104 | Beliefs MUST remain derived, revisable stances distinct from memory and source truth, with valid time separate from record time; contradiction records MUST remain reviewable signals, not silent truth overwrites. Hosts MUST NOT advertise record-time history or fuzzy belief recall where only valid-time/exact-subject lookup is implemented. | refines: PR-001, CI-001, PC-001; belief fixtures and manual claim review. | `core/domain/src/belief.rs:1-5,68-95`; `core/integration/src/recall.rs:24-38`; `adapters/integration/src/fixtures/belief.rs:22-81`. |
| PRD-105 | Recall MUST return a policy-filtered `ContextPayload` that explains fused provenance, omissions and unavailable sources; a failed lane MUST degrade through `source_failures`, not erase surviving lanes or turn an empty degraded result into a claim that nothing relevant exists. | refines: PR-001, PR-005, PC-001; recall/evaluation fixtures; manual interpretation review. | `core/integration/src/recall.rs:21-38`; `core/integration/src/sqlite/recall.rs:195-278,401-415`; `docs/product/engram.md:106-116`. |
| PRD-106 | Bound-code answers MUST identify the authorized scope/`RepoId`, exact `refs/heads/...` binding and published generation; freshness MUST distinguish indexed OID from explicitly observed ref OID, reporting unknown when unobserved rather than inferring authority from checkout `HEAD`. | refines: PR-001, PR-005, SP-001; bound-source/status and transport fixtures. | `docs/architecture/domain-data-model.md:59-85`; `core/integration/src/code_binding/status.rs:8-57,108-132`; `docs/product/engram.md:73-82`. |
| PRD-107 | A capability MUST NOT be called shipped merely because an adapter exists: it MUST be callable through the provider, N-API/TS and supported MCP surface or carry the root-required explicit exception, and `CapabilityReport` MUST describe the selected recipe's actual support. The launcher bundle's host limitations MUST be disclosed, not sold as uniform host behavior. | refines: SP-001, SP-002, SP-003, CI-002; enforced-by: check_surface_parity.py (accessor names only); transport/install fixtures and review. | `docs/constitution/02-surface-parity.md:8-25`; `core/integration/src/capability.rs:10-15,24-131`; `README.md:64-69`; `mcp/engram-mcp/src/app.rs:1-7`. |
| PRD-108 | Ontology/taxonomy MUST remain first-class governed vocabulary, not opaque metadata or a claim of automatic truth enforcement. Pack installation MUST distinguish full structural vocabulary from the bounded launch slice, expose sliced/available counts, and preserve advisory graph validation unless a separately declared policy makes it blocking. | refines: PR-001, PR-005, PC-001; pack fixtures and manual claims review. | `core/knowledge/src/ontology.rs:7-12,16-38`; `mcp/engram-mcp/src/pack.rs:1-14,58-70`; `core/packs/src/slice.rs:1-23,83-106`. |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
