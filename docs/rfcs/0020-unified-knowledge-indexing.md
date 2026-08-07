# RFC-0020: Unified Knowledge Indexing — Code Graph + Unstructured Concepts

**Status:** Accepted (2026-08-07)
**Author:** engram-cc session
**Related:** RFC-0012 (codegraph layer), RFC-0015 (unified MCP), the [AgentZero indexing guidance](https://github.com/phanijapps/agentzero/blob/main/docs/architecture/engram-code-graph-indexing.md), RFC-0014 (canonical identity), RFC-0018 (retrieval quality)

## Problem

The engram scanner conflates two fundamentally different knowledge types — **code
graph topology** (callers, callees, containment, blast radius) and **unstructured
knowledge** (documents, observations, concepts, beliefs) — into a single flat
entity pool with unqualified identities. This produces:

1. **Noise entities** — `new`, `clone`, `}`, `str`, `type: string` enter the graph as
   entities with no stable identity, polluting centrality/recall.
2. **No source identity in results** — recall items lack `repository`, `path`, `symbol`,
   `kind`, `revision`, `provenance`. Agents can't validate or trace results.
3. **Untyped edges** — all edges are `calls` (inferred from name occurrence). No
   `contains`, `implements`, `routes_to`, `reads`, `writes`, `tests`, `emits`, and no
   semantic edges for document concepts (`depends_on`, `has_property`).
4. **Document headings as graph nodes** — the naive "first heading line = concept node"
   rule turns `Architecture`, `type: string`, and changelog headings into graph nodes
   with the same status as a function that persists state.

## Proposal

A **unified indexing model** with two entity **kinds** in one graph — **code topology
entities** (functions, modules, traits, routes) and **knowledge concept entities**
(valid concepts, properties, and relationships extracted from unstructured documents)
— plus typed edges, kind-scoped queries, and git provenance carried as metadata.

The two kinds share a graph store but never conflate. Code-topology queries
(centrality, callers/callees, blast radius) run over code entities only; knowledge
queries (concept relationships) run over concept entities. A document concept never
masquerades as a code function, and a code function never pollutes concept queries.

### Two entity kinds, one graph

#### Kind 1 — Code topology entities

Only code-derived entities participate in code-topology queries. These are the nodes
for centrality, callers/callees, blast-radius traversal.

| Kind | Qualified identity format | Extracted by |
|---|---|---|
| Module/Package | `{repo}/{path}::{module}` | treesitter |
| Function/Method | `{repo}/{path}::{module}::{receiver}::{name}` | treesitter |
| Struct/Class/Type | `{repo}/{path}::{module}::{name}` | treesitter |
| Trait/Interface | `{repo}/{path}::{module}::{name}` | treesitter |
| API route | `{repo}::{method}::{path}` (e.g., `zbot::POST::/api/agents/{}`) | route scanner |
| WebSocket event | `{repo}::ws::{event_type}` | protocol scanner |
| Test case | `{repo}/{test_path}::{test_name}` | test scanner |
| Configuration key | `{repo}::config::{key}` | config scanner |

**Identity rule**: every code entity has a qualified identity — `{repo}/{path}::{qualified_name}`.
`{repo}` is a **stable repo identifier** (the existing stable-source-key), NOT the git
remote (see *Repository provenance*). A bare name like `get`, `new`, `clone` is never a
sufficient identity. Entities without a qualified identity are suppressed at extraction
time (not post-filtered).

**Noise suppression** (at extraction, not recall):
- Bare generics + language primitives: `str`, `int`, `bool`, `void`, `None`, `Vec`, `Option`, `Result`, `new`, `clone`, `log`, `fmt`, `send`, `name`, `len`, `main`, `get`, `set`, `run`, `read`, `write`, `append`, etc.
- Single-char or punctuation-only names; non-alphabetic-leading names.
- Generated/vendored paths (`node_modules/`, `target/`, `.git/`).

#### Kind 2 — Knowledge concept entities (from documents)

Unstructured documents (markdown, text, RFCs, ADRs, notes, transcripts) yield a
**knowledge sub-graph**: valid concepts as nodes, properties and typed relationships as
edges. These are produced by **semantic triple extraction**, not by the naive
heading-as-node rule. Concepts pass the same noise filter as code symbols.

| Extracted | Stored as | Example |
|---|---|---|
| Concept | `Concept` entity | `engram`, `sqlite-store`, `pi-mono` |
| Property | typed edge `has_property` | `engram --has_property--> language: Rust` |
| Relationship | typed semantic edge | `engram --depends_on--> rust-core` |

`EntityKind::Concept` is also still emitted by distillation (RFC-0014 canonical
identity), retrieval fusion, and the associative/community indexes. Code-centrality
queries exclude Concept entities automatically because they are a distinct kind.

### Document extraction approach

Unstructured→triples is an LLM-strength task; deterministic NLP (noun-phrase,
co-occurrence) is low quality and would pull NLP dependencies into the deterministic
Rust core. The repo already runs an LLM maintenance layer (pi-mono in `engram-maintain`,
used for belief synthesis and contradiction detection). Document extraction reuses it:

- **Ingest (deterministic, Rust):** documents are **chunked only**. The naive
  `concept_name` heading-as-node path is **removed** (it was the noise source). No graph
  entities are emitted from documents at ingest.
- **Maintenance `extract-knowledge` op (LLM, TS — new):** mirrors `reflect`/`contradict`.
  Pages document chunks → pi-mono with a structured-output tool emitting `concepts`,
  `properties`, and `relationships` (typed subject–predicate–object triples) → writes
  `Concept` entities + typed relationship edges via `putEntity`/`putRelationship`.
  Idempotent and convergent on re-run; every concept passes the noise filter. Concept
  identity is the canonical concept label, consolidated across sources by RFC-0014.

This keeps ingest fast and deterministic, and puts quality semantic extraction where the
LLM already lives. Concept entities are typed distinctly from code entities, so the two
kinds never conflate.

**Rejected alternative:** deterministic dependency-parse / OpenIE triple extraction in
the Rust extractor — low quality, drags NLP deps into the deterministic core, and
duplicates the LLM layer's purpose.

### Typed edges with provenance

Replace the current single `calls` edge with typed, directed edges. Code edges:

| Edge type | From → To | Example |
|---|---|---|
| `calls` | function → function | `runner → response_resolver` |
| `contains` | module → function, module → type | `crate → module → symbol` |
| `implements` | struct → trait | `SqlStore → StoreTrait` |
| `routes_to` | API route → handler function | `POST /api/agents → handle_create` |
| `reads` / `writes` | function → persistent entity | `persist_turn → AssistantRow` |
| `emits` | function → event type | `runtime → StreamEvent` |
| `tests` | test case → function/route | `test_respond → respond` |
| `configured_by` | function → config key | `bootstrap → ENGRAM_STORAGE` |

Document semantic edges (from `extract-knowledge`):

| Edge type | From → To | Example |
|---|---|---|
| `depends_on` | concept → concept/entity | `engram → rust-core` |
| `has_property` | concept → value | `engram → language: Rust` |
| `relates_to` | concept → concept | `sqlite-store → knowledge-graph` |
| `describes` | concept → code entity | `readme-architecture → Architecture struct` |

Each edge carries: `{source_file, revision, extraction_method: "treesitter"|"heuristic"|"llm"|"manual", confidence}`.

### Repository provenance (metadata, not identity)

Every entity and edge carries `{repository, revision}` provenance metadata in `record_json`.
`repository` is the git remote URL and `revision` is the commit SHA; both are **metadata
properties**, not part of the qualified identity (the identity uses the stable repo id as
`{repo}`). This enables **optional** `repository`/`revision` filtering in recall/search —
useful for "show me only zbot code" — but is NOT a hard isolation boundary.

A workspace is a **visibility boundary** that intentionally contains multiple repos (e.g.,
a microservices monorepo with shared libs, service A depending on service B). Cross-repo
dependencies are the norm, not contamination. The ingester (`scan_repo` / the caller) is
responsible for what enters the store; the store doesn't second-guess the ingester's scope.

**Optional filtering**: recall/search/graph tools accept `repository` + `revision` params.
When provided, results are restricted. When absent (default), all workspace entities are
visible — correct for cross-cutting queries ("how do these services interact?").

### Query behavior defaults

1. Resolve exact qualified symbols BEFORE semantic recall.
2. For code questions, return code entities + direct tests FIRST, document/concept
   evidence SECOND (separately labelled).
3. Default to the latest indexed revision per repository.
4. Centrality/ranking computed over the **code entity kind only**. Concept entities are a
   distinct kind and are excluded from code-centrality by construction (kind-scoped), not
   by a runtime exclusion list.
5. Every result exposes: `{kind, repository, path, symbol, score, revision, provenance}`.

## Implementation phases

### Phase 1: Extraction quality (immediate)
- **Code qualified identity at extraction**: `parse_symbol` produces `{repo}/{path}::{name}` identities instead of bare names.
- **Code-symbol noise filter**: `is_noise_symbol` suppresses bare generics + primitives (DONE — shipped with tests).
- **Document `concept_name` removal + `extract-knowledge` LLM op**: the naive heading-as-node path is removed from the deterministic extractor; a new `engram-maintain extract-knowledge` op extracts valid concepts/properties/relationships into Concept entities + typed edges.

### Phase 2: Typed code edges
- Extend the `GraphExtractor` to emit typed code edges (`contains`, `implements`, `routes_to`) instead of only `calls`.
- Requires deeper treesitter analysis (module containment, trait impls, route definitions).

### Phase 3: Kind-scoped centrality
- `architecture` / `code_health` compute centrality over code entities only. Because
  document concepts are a distinct kind, code-centrality excludes them by construction.
- Concept-entity relationships power knowledge queries independently.

### Phase 4: Git provenance metadata + optional filtering
- Stamp `repository` (git remote) + `revision` (SHA) on entity + relationship records as `record_json` metadata (not a schema change).
- `scan_repo` stamps the git remote + SHA on every entity/edge it creates.
- Recall/search/get_context accept OPTIONAL `repository`/`revision` filter params (default: all workspace entities visible).

## What this does NOT change

- The domain model (entities, relationships, chunks, memories, beliefs) stays as-is — no contract break. `EntityKind::Concept` is reused, not added.
- The MCP tool surface stays as-is (36 tools). `extract-knowledge` is a new `engram-maintain` op, surfaced through the existing `maintenance_run` tool, not a new tool.
- The store schema stays as-is — provenance goes in `record_json` metadata, not new columns.
- The scanner CLI stays as-is — the extraction logic changes, not the pipeline.

## Acceptance criteria (from the zbot guidance)

1. `symbol_context(assistant_turn_content)` → callers/callees, no unrelated repos or generic callees.
2. `search("respond assistant row continuation")` → the response resolver + its tests, before planning docs.
3. A route-to-storage trace: handler → gateway-execution → runtime → persisted content → emitted events.
4. `architecture` → qualified AgentZero boundaries, not `get` or `clone`.
5. Re-scanning a repo converges (upserts current, tombstones stale) without leaving orphaned entities from prior revisions.
6. `extract-knowledge` on a markdown doc yields valid concepts + typed relationships (no `Architecture`/`type: string` noise), queryable separately from code topology.

## Unstructured knowledge support

This RFC is not code-only. Unstructured documents produce a knowledge sub-graph via the
`extract-knowledge` LLM op: valid concepts (nodes), properties, and typed relationships
(edges). These are `Concept` entities — the same kind distillation and retrieval fusion
already use — and they are queryable independently of code topology:

- **Agent-written observations** (`write_memory`) → `memories` table → facts lane in recall.
- **Distilled concepts** (from docs, transcripts, notes) → `Concept` entities via `extract-knowledge`, consolidated by RFC-0014.
- **LLM-synthesized beliefs** → `beliefs` table → belief lane in recall (with confidence + provenance).
- **Manual knowledge** (procedures, preferences, episodes) → `memories` (typed by `kind`).

A memory observation like "zbotd provides HTTP access to the agent runtime" stays in the
memories surface; an `extract-knowledge` pass over zbotd's docs yields `Concept` entities
(`zbotd`, `agent-runtime`) and a `provides` relationship. When an agent asks about
`zbotd`, recall fuses the **graph lane** (the `zbotd` code entity + callers/callees + the
concept sub-graph) with the **facts lane** (the memory observation).

## Out of scope

- New store schema columns (provenance goes in record_json).
- The `engram-codegraph-temporal` crate (whats_changed — deferred).
- Embedding pipeline changes (Phase 1–4 don't change vector indexing; the 442k chunk vectors are untouched).
- Cross-workspace federation (multiple tenants seeing each other's data).
- Deterministic/NLP triple extraction in Rust (rejected — see Document extraction approach).
