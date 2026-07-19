# Engram — Product Overview

> The single consolidated view of the engram product **today**, synthesized
> from all 120 feature specs in [`docs/specs/`](../specs/). Per-feature
> contracts live in the specs; the code map is the
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
never free-floating. Ingest from filesystem/git; **tree-sitter AST** symbol
extraction (10+ languages); deterministic graph extraction; opt-in
**LLM** entity/relationship extraction. Re-ingest **converges** the graph
(retraction). **Cross-repo linkage** via stable source keys + shared OpenAPI
contract nodes. *Specs: knowledge-ingestion, knowledge-graph-extractor, ast-symbol-extraction, scale-repo-ingestion, background-repo-indexer, structured-repo-identity, contract-first-ingestion, knowledge-graph-retraction.*

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
`engram-store-sqlite`). **SurrealDB** (embedded SurrealKV, graph-native) is a
draft sibling. No cross-engine migration (fresh store on switch).
Engine-agnostic adapters (lexical / associative / community / decay / ingest)
are shared across engines. *Specs: sqlite-consolidation, sqlite-open-options,
sqlite-file-backed-construction, surrealdb-backend.*

### Rust SDK + host-SDK ✅
`EngramProvider` — the canonical Rust entry: a facade holding typed repository
handles, with `CapabilityReport` (19 capability areas), an engine-neutrality gate, an embedding-provider abstraction
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
(kg-redesign, dashboard-tenant-view). *Specs: engram-viz, demo-reimagine,
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

This is a **living product view** synthesized from the 120 feature specs in
[`docs/specs/`](../specs/). Per-feature contracts, plans, and acceptance
criteria live there; this doc rolls them up by **capability** and tracks
product-level status. When a spec ships or a capability's status changes, update
the relevant line here in the same PR — drift between this doc and the specs is
a bug. (See [`docs/CONVENTIONS.md`](../CONVENTIONS.md) document-lifecycle.)

## See also

- [README](../../README.md) — project overview, use cases, quick start.
- [Architecture overview](../architecture/overview.md) — the pipeline + layer responsibilities.
- [Roadmap](roadmap.md) · [Changelog](changelog.md).
- [Specs index](../specs/README.md) — the 120 feature specs.
- [Research synthesis](../research/README.md) — prior-art + concept grounding.
- [`CHARTER.md`](../CHARTER.md) — mission + scope.
