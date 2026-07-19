# Engram

> Contract-first agentic memory — structured, durable recall for AI agents that
> need more than a context window.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)
[![TypeScript](https://img.shields.io/badge/typescript-sdk-blue.svg)](packages)
[![Status](https://img.shields.io/badge/status-pre--1.0-yellow.svg)](#status)

Engram is an open-source **agentic memory layer**: a Rust core that owns
deterministic memory, knowledge-graph, belief, hierarchy, and retrieval
behavior, with TypeScript bindings — so agents get reliable, structured,
long-lived memory instead of opaque, disposable context windows.

It is **not a vector database** (vectors are one retrieval mode among six). It
is **not a knowledge store** (knowledge is source-grounded, not free-floating).
It is the layer where raw observations become **durable beliefs**, where
retrieval is **composed** (not single-mode), and where **policy, provenance, and
scope** govern every read and write — so memory is auditable, not accidental.

## What engram is for

For product, data, and strategy teams — the problems engram solves, in plain terms:

- **Long-horizon agent memory.** An agent that remembers decisions, constraints,
  and stakeholder preferences across sessions, days, or months — without
  re-reading its entire history every turn (the problem Microsoft Research's
  [Memora](https://www.microsoft.com/en-us/research/blog/memora-a-harmonic-memory-representation-balancing-abstraction-and-specificity/)
  names). Less context burned, better answers on multi-hop questions.
- **Source-grounded knowledge, not hallucinated facts.** Ask questions grounded
  in your code repositories and documents; every answer traces back to a chunk
  and its source. Memory (agent experience) stays distinct from knowledge
  (grounded content) so the two never blur.
- **Auditable, governed recall.** Every record carries policy, provenance, and
  scope — who saw it, where it came from, how long it lives, who may retrieve
  it. Redaction and deletion are first-class; nothing leaks through retrieval.
- **Code understanding.** A codegraph layer answers structural questions — "who
  breaks if I change this?", "find the call path from handler to database",
  "what changed recently that matters most?" — over any indexed repository.

## Concepts at a glance

Engram bakes in eight concepts beyond "store text, search by similarity." The
full version is in [The conceptual model](#the-conceptual-model); the pipeline
that connects them is in the [architecture overview](docs/architecture/overview.md).

1. **Memory as a first-class domain** — typed records with a lifecycle, not bare rows.
2. **Source-grounded knowledge graph** — facts traceable to a source, never free-floating.
3. **Belief synthesis** — derived, recomputable, *bi-temporal* stances over evidence.
4. **Hierarchy** — aggregate + navigate + compress context to the right granularity.
5. **Retrieval composition** — six modes fused (RRF) + cross-encoder rerank, never one.
6. **Consolidation** — reflection + decay as an explicit, auditable pipeline.
7. **Policy, provenance, scope** — governance checked on every read and write.
8. **Contract-first** — domain types outrank SQL; portable across storage engines.

---

## Why engram exists

Agent memory gets messy when storage, ranking, policy, provenance, and runtime
integration collapse into one service. The result is either a vector DB that
returns similar text without understanding *why* it's relevant, or an LLM
context window that forgets everything when the session ends.

Engram keeps these concerns **separate and explicit**:

- **Contract-first** domain types that rank above any SQL schema or storage
  engine — so agents depend on *semantics*, not *implementation*.
- **Rust-owns-deterministic-behavior** — the core traits, adapters, and
  conformance tests are in Rust; TypeScript wraps generated contracts.
- **Replaceable adapters** — storage (SQLite, SurrealDB), retrieval (vector,
  graph, lexical, associative), and embedding providers (FastEmbed, Ollama) are
  behind ports, never hardcoded.
- **Policy on every path** — scope, retention, allowed-uses, and provenance are
  checked on write, retrieve, ingest, consolidate, and forget — never hidden in
  a generic manager.

---

## The conceptual model

Engram bakes in eight concepts that go beyond "store text, search by
similarity." Each is a distinct domain axis with its own port, lifecycle, and
contract.

### 1. Memory as a first-class domain (not just rows)

A `MemoryRecord` is a canonical durable unit with explicit `kind`
(observation, fact, preference, episode, artifact, relationship, procedure),
`content`, `scope`, `provenance`, `policy`, `status`, and `links` — never a
bare row. Memory has a **lifecycle** (`active → archived → redacted → forgotten
→ expired`) and emits append-only `MemoryEvent`s (`written`, `retrieved`,
`consolidated`, `forgotten`, `belief_synthesized`, …). State and events are
separated, so the layer is **auditable and replayable** rather than a mutable
log.

A `MemoryRole` (working / episodic / semantic / procedural, CoALA-aligned) is
*derived* from kind + policy + scope + provenance — keeping the contract small
while preserving the cognitive-science taxonomy.

### 2. Knowledge graph, source-grounded (not free-floating facts)

Knowledge is a separate domain axis from memory:
`KnowledgeSource → SourceDocument → KnowledgeChunk → KnowledgeEntity /
KnowledgeRelationship → EmbeddingRef`, bounded by a named `KnowledgeGraph`.

Memory records agent **experience**; knowledge records are **source-grounded
content** from code repos, documents, URLs. A memory may *link* to a knowledge
chunk but cannot replace it — preventing hallucinated "facts" from masquerading
as grounded knowledge. Every chunk references its source and document (invariant).

### 3. Belief synthesis (what agents BELIEVE vs what they observed)

A `Belief` is a *derived stance* over evidence — never raw memory, never source
truth. It has a `subject`, declarative `content`, `confidence`, weighted
`BeliefSource`s, a lifecycle (`active / stale / superseded / retracted`), and a
`synthesizer` derivation ref. A belief is **recomputable** — when a source is
invalidated, a single-source belief retracts; a multi-source belief is marked
`stale` and resynthesized.

Beliefs are **bi-temporal**: `valid_from / valid_until` define when the content
was true *in the world* (valid time), distinct from `created_at / updated_at`
(when engram recorded it — transaction time). This "true as-of T" retrieval is
the whitespace no competitor owns.

`Contradiction` records are *reviewable signals*, never automatic truth changes
— they classify the tension (`logical`, `temporal`, `tension`, `duplicate`,
`policy`) and capture how a human or system resolved it.

### 4. Hierarchy (aggregation + navigation + context compression)

`HierarchyNode` organizes retrievable objects (memories, chunks, entities,
concepts, other nodes) into abstraction layers — `layer 0` = base retrievable,
`layer > 0` = aggregate (summary, schema, topic, cluster, domain). This is the
GraphRAG insight: a query may need a raw chunk, an episode summary, a workflow
schema, or a domain pattern; hierarchy lets retrieval return the right
granularity instead of N similar low-level fragments.

Hierarchy ≠ taxonomy. Taxonomy organizes *controlled concepts*; hierarchy
organizes *retrievable objects* for **navigation and context compression**.
Construction (building the tree) is separated from navigation (traversing it).

### 5. Retrieval composition (never one mode)

`RetrievalRequest` fans out across **six modes** — `temporal`, `cue`,
`hierarchical`, `semantic` (vector), `graph`, `keyword` (BM25) — with results
fused, reranked, budget-compressed, and policy-filtered into a `ContextPayload`.

The `RetrievalScore` is **multi-factor**: `relevance + recency + confidence +
cue_match + hierarchical_fit + policy_fit`. Fusion uses
[Reciprocal Rank Fusion](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)
(RRF) to unify heterogeneous sources without distributed write consistency.
An optional **cross-encoder reranker** reorders fused candidates for precision.

A `FusionTrace` is attached to every result: source, source rank, source score,
fusion score, rerank score, deduplicated-with — full explainability of how a
candidate moved through the pipeline. When sources degrade or results are
omitted (budget, policy), the agent is told *what* was dropped and *why*.

### 6. Consolidation (reflection + decay, as an explicit pipeline)

Consolidation is a **first-class operation pipeline**, not an incidental write
side-effect: `ConsolidationRequest → ConsolidationPlan (dry-run) →
ConsolidationRun (auditable execution) → ConsolidationTaskResult`s.

This is where **reflection** happens (working memory → episodic event → semantic
fact → optional taxonomy/graph/procedural update — Generative Agents /
Reflexion-style) and where **forgetting** happens (Ebbinghaus-style decay:
policy-expiry + memory curve). Both are explicit, both auditable. Without
consolidation, memory degrades into an undifferentiated log.

Task kinds: `fact_extraction`, `belief_synthesis`, `belief_contradiction_detection`,
`hierarchy_build`, `taxonomy_evolution`, `graph_evolution`, `decay`, `pruning`,
`procedure_extraction`, `evaluation`, and more.

### 7. Policy, provenance, and scope (governance baked into every path)

`Policy` (`visibility / retention / sensitivity / allowed_uses / expires_at /
delete_mode`) and `Provenance` (`source / actor / observed_at / evidence /
derivations / confidence / method`) are **required fields on every durable
record**. `Scope` (`tenant` required; `subject / workspace / session /
environment` optional) bounds every operation.

These are not optional metadata — they are **checked at runtime** on write,
retrieve, ingest, consolidate, and forget. Redacted records must not leak
through retrieval, evaluation, links, or explanations. Forgetting is a domain
concept (`delete / redact / tombstone / archive`), not a DB delete. Every
record's lineage is traceable via `EvidenceRef` and `DerivationRef`.

### 8. Contract-first design (domain types outrank SQL)

The human-readable domain model (`docs/domain-data-model.md`), the
machine-readable wire contract (`contracts/v1/`), and the behavior specs
(`docs/specs/`) are the **source of truth**. Rust types, generated TypeScript,
JSON Schema, database schemas, and API payloads all conform *downward* — never
the reverse. This is what makes engram portable across SQLite today and
SurrealDB / Postgres / Neo4j tomorrow without breaking agents that depend on it.

The contract freeze policy forbids renames, removals, or meaning changes within
a version; breaking changes require a new versioned contract. `metadata` maps
are allowed, but core semantics must use typed fields (no smuggling policy or
provenance through unstructured metadata).

---

## Storage backends — one crate per engine, swap by config

Each storage engine lives in a single crate holding **all** its database
operations. Consumers switch backends by changing a config string + Cargo feature
— no application code changes, no migration (fresh store on switch).

| Backend | Crate | Status |
|---|---|---|
| **SQLite** | `engram-store-sqlite` | ✅ Complete — memory, knowledge, belief, hierarchy, vector, consolidation glue, `SqliteOpenOptions` |
| **SurrealDB** | `engram-store-surreal` | ✅ All 5 capabilities — memory, knowledge, belief, hierarchy, vector (MTREE) |
| **Mixed** (future) | `engram-store-mixed` | 🔜 Compose multiple engines (e.g. lancedb + neo4j) |

```toml
# Select the backend in a profile file
[backend]
kind = "sqlite"        # or "surreal"
data_root = "/var/lib/engram"
```

```bash
# Compile with the backend feature
cargo build --features sqlite    # or --features surreal
```

Both backends share the **same ports, same DTOs, same facade** — the storage
engine is an adapter detail. An engine-neutrality lint
(`.codex/hooks/check-engine-neutrality.sh`) enforces that no engine type
(`Sql*`, `Surreal*`, raw SQL) leaks into the neutral facade or port crates.
Engine-agnostic adapters (lexical BM25, associative-graph PPR, community-summary,
cross-encoder rerank, decay, ingest) work with any backend — see
[Extend the storage layer](docs/guides/how-to/extend-storage.md) for the full
adapter list + how to add a new engine.

---

## Architecture

Data flows one way: an agent **writes** (or ingests a source) → engram
**processes and stores** across swappable engine cells (policy + provenance on
every row) → a separate **retrieval composition** layer fuses many recall modes
into one policy-filtered **context packet** returned to the agent. Storage and
retrieval are deliberately decoupled — the same "decouple what is stored from
how it is retrieved" insight as Memora.

The rule of thumb: `engram-domain` owns portable concepts; `engram-memory`,
`-knowledge`, `-belief`, `-hierarchy`, `-retrieval`, `-consolidation` own their
respective ports; `engram-integration` is the SDK facade (`EngramProvider`);
concrete infrastructure lives behind adapter crates; TypeScript wraps generated
contracts instead of redefining them.

The full pipeline diagram (write/ingest → process → storage cells → retrieval
composition → context packet), the layer responsibilities, and the research
grounding are in the **[architecture overview](docs/architecture/overview.md)**.

---

## Status

Engram is **pre-1.0** — demo-driven, not production-ready. The conceptual
model (memory, knowledge, belief, hierarchy, retrieval, consolidation) is
direction-fixed; the frozen v1 contract covers the memory + retrieval +
evaluation vertical. Belief, contradiction, hierarchy, and consolidation are
draft extension contracts — direction-fixed but not frozen.

Current validated surface:

- **Memory**: write, retrieve, forget, lifecycle events, scope isolation,
  policy enforcement — SQLite + SurrealDB.
- **Knowledge graph**: source → document → chunk → entity → relationship,
  taxonomy, ontology, graph traversal (neighbors) — SQLite + SurrealDB.
- **Belief**: put, get (valid-time), mark-stale, supersede, retract,
  contradictions — SQLite + SurrealDB.
- **Hierarchy**: put-node, put-relation, path navigation — SQLite + SurrealDB.
- **Vector retrieval**: embedding-space-validated insert/search, KNN — SQLite
  (sqlite-vec) + SurrealDB (MTREE).
- **Retrieval composition**: graph + lexical + vector lanes, RRF fusion,
  cross-encoder rerank, associative (PPR), community-summary (GraphRAG).
- **Consolidation**: reflection (derived beliefs) + decay (Ebbinghaus) +
  composite executor — wired into the `EngramProvider` facade via
  `require_consolidation()`.
- **MCP servers**: memory MCP (`write_memory`, `recall`, `forget`, `put_entity`,
  `put_relationship`, `consolidate`) + codegraph MCP (23 tools — `scan_repo`,
  `dead_code`, `blast_radius`, `dependency_path`, `central_symbols`,
  `call_communities`, temporal scoring, …; see the
  [MCP guide](docs/guides/how-to/connect-via-mcp.md)).
- **N-API binding**: full `EngramProvider` surface reachable from TypeScript.
- **Backend swap**: SQLite ↔ SurrealDB by config string + Cargo feature.
- **Engine-neutrality lint**: enforces no engine types in the facade/ports.
- **Codegraph layer** (RFC-0012): `dead_code`, `blast_radius`, `dependency_path`,
  `central_symbols` (PageRank), `bridge_symbols` (betweenness),
  `call_communities` (Louvain), temporal scoring — on top of engram, not in it.

---

## Repository layout

```text
contracts/           Portable JSON schemas and generated contract outputs.
core/                Storage-neutral Rust crates.
  domain/            Domain types, invariants, serde, version markers.
  runtime/           Shared errors, result type, clocks, ids, policy gates.
  memory/            Memory service + repository ports.
  knowledge/         Knowledge, graph, ontology, source, ingestion ports.
  belief/            Belief synthesis, contradiction, bi-temporal ports.
  hierarchy/         Hierarchy build, navigation, aggregate ports.
  consolidation/     Consolidation planning, gated mutation, decay, audit.
  reflection/        Reflection synthesizer + consolidation executor.
  retrieval/         Retrieval composition + fusion ports + VectorIndex.
  integration/       SDK facade: EngramProvider, EngramConfig, CapabilityReport.
  eval/              Deterministic fixtures + regression harness.
  graph-analytics/   Pure graph algorithms (PageRank, betweenness, communities).

adapters/            Replaceable infrastructure crates.
  sqlite/            engram-store-sqlite — ALL SQLite DB ops (one crate).
  surreal/           engram-store-surreal — ALL SurrealDB DB ops (one crate).
  ingest/            Filesystem/git ingestion adapter.
  retrieval/         sqlite-vec, tantivy-lexical, associative-graph,
                     community-summary, cross-encoder-rerank.
  consolidation/     Decay executor (Ebbinghaus curve).
  integration/       Backend recipe / conformance composition.

bindings/            Native language bridges (N-API for TypeScript).

codegraph/           On-top codegraph layer (RFC-0012).
  queries/           Dead-code, blast-radius, dependency-path, central/bridge.
  temporal/          Temporal scoring (recent / impact / compound).
  mcp-server/        MCP server exposing codegraph queries to AI agents.

memory/              Memory MCP server (agent-callable tools).

packages/            TypeScript workspace.
  contracts/         Generated TypeScript types + schemas.
  client/            Ergonomic application SDK.
  node/              Native binding package.
  adapters/          JS-side framework + gateway integrations.
  eval/              Fixture authoring helpers + CLI wrappers.

docs/                Architecture, ADRs, RFCs, research, specs, domain model.
```

---

## Quick start

### Prerequisites

- **Rust 1.85+** (edition 2024).
- **Node 22+** and **pnpm 10** (`corepack enable && corepack prepare pnpm@10 --activate`).
- Optional for LLM extraction + Q&A: an OpenAI-compatible endpoint.

### Build + test

```bash
# Install JS dependencies
pnpm install

# Build the Rust workspace (default: no backend feature)
cargo check --workspace

# Build + test with a backend
cargo test --workspace --features sqlite     # SQLite backend
cargo test -p engram-integration --features surreal   # SurrealDB backend

# TypeScript generation + typecheck + tests
pnpm run check
```

> For the demo, MCP server startup, validation hooks, and every feature
> combination, see the **[build guide](docs/guides/how-to/build-and-run.md)**.

### Use the SDK (Rust embedder)

```rust
use engram_integration::{EngramConfig, EngramProvider, CapabilityPolicy,
    EmbeddingProviderConfig, MigrationMode};
use engram_domain::types::ScopeMappingStrategy;

let config = EngramConfig::new(
    "/var/lib/engram", "/var/lib",
    ScopeMappingStrategy::Strict,
    EmbeddingProviderConfig {
        provider_type: "fastembed".to_string(),
        model: "BAAI/bge-small-en-v1.5".to_string(),
        dimensions: 384, prompt_profile: "query".to_string(),
        normalization: None,
    },
    MigrationMode::DryRun, CapabilityPolicy::FailClosed,
);

// open() selects the backend by compiled feature (sqlite or surreal)
let provider = EngramProvider::open(&config)?;

// Check what's supported
let report = provider.capabilities();
println!("memory: {:?}", report.memory);
println!("knowledge: {:?}", report.knowledge);

// Write + retrieve through the facade
let memory = provider.memory().expect("memory supported");
// ... memory.write_memory(request).await
```

### Profile-file configuration

```toml
# engram.toml — select the backend declaratively
[backend]
kind = "surreal"                    # "sqlite" or "surreal"
data_root = "/var/lib/engram"

[embedding_provider]
provider_type = "fastembed"
model = "BAAI/bge-small-en-v1.5"
dimensions = 384
prompt_profile = "query"
```

```rust
let config = EngramConfig::from_profile_file("engram.toml")?;
let provider = EngramProvider::open(&config)?;
```

---

## Connect via MCP

Engram ships two MCP servers exposing memory + codegraph operations as
agent-callable tools (stdio JSON-RPC 2.0), so any client — Claude Desktop,
Cursor, Copilot — can read and write engram with no code on your side:

| Server | Tools |
|---|---|
| **memory MCP** (`engram-memory-mcp`) | `write_memory`, `recall`, `forget`, `put_entity`, `put_relationship`, `consolidate` |
| **codegraph MCP** (`engram-codegraph-mcp`) | 23 tools — `scan_repo`, `dead_code`, `blast_radius`, `dependency_path`, `central_symbols`, `call_communities`, temporal scoring, … |

```jsonc
// e.g. .vscode/mcp.json or claude_desktop_config.json — stdio transport
{
  "mcpServers": {
    "engram-memory": {
      "command": "cargo",
      "args": ["run", "-p", "engram-memory-mcp", "--", "/path/to/store"]
    }
  }
}
```

Full tool lists, the codegraph index-then-query flow, per-client config
locations, and the MCP-vs-N-API-vs-Rust-facade choice are in the
**[MCP guide](docs/guides/how-to/connect-via-mcp.md)**.


---

## Contracts

The accepted v1 contract package lives in `contracts/v1/`. Domain types in
`engram-domain` are the Rust source of truth; TypeScript types are generated
from them and should not be edited by hand.

```bash
pnpm run contracts:generate     # generate TS types from Rust
pnpm run contracts:check-generated  # verify they match
python3 tools/scripts/validate_contracts.py
```

---

## Development workflow

Engram uses spec-driven implementation:

1. Record durable architecture decisions in `docs/adr/`.
2. Add or update specs under `docs/specs/` before behavior changes.
3. Run Rust, TypeScript, contract, docs, and engine-neutrality gates.

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm run check
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/check-engine-neutrality.sh   # ADR-0022 rule-1 gate
```

---

## Key documentation

**Guides:**

| Document | What it covers |
|---|---|
| [`docs/architecture/overview.md`](docs/architecture/overview.md) | The memory pipeline diagram + layer responsibilities |
| [`docs/guides/how-to/build-and-run.md`](docs/guides/how-to/build-and-run.md) | Prerequisites, build/test, demo, MCP startup |
| [`docs/guides/how-to/connect-via-mcp.md`](docs/guides/how-to/connect-via-mcp.md) | Both MCP servers, tool lists, client configs |
| [`docs/guides/how-to/extend-storage.md`](docs/guides/how-to/extend-storage.md) | How to add a new storage backend |
| [`docs/guides/how-to/build-a-surrealdb-store.md`](docs/guides/how-to/build-a-surrealdb-store.md) | SURQL patterns in the SurrealDB backend |
| [`docs/research/README.md`](docs/research/README.md) | Synthesized research index (concept → source map) |

**Sources of truth:**

| Document | What it covers |
|---|---|
| [`docs/domain-data-model.md`](docs/domain-data-model.md) | The source-of-truth domain model (2,400+ lines) |
| [`docs/CHARTER.md`](docs/CHARTER.md) | Mission, scope, six principles |
| [`docs/research/synthesis.md`](docs/research/synthesis.md) | Research → architecture direction |
| [`docs/research/engram-framing-synthesis.md`](docs/research/engram-framing-synthesis.md) | The "belief layer" positioning |
| [`docs/research/academic-research-findings.md`](docs/research/academic-research-findings.md) | CoALA, Tulving, MemGPT, ACT-R, GraphRAG citations |
| [`docs/adr/`](docs/adr/) | 25 architecture decision records |
| [`docs/rfcs/`](docs/rfcs/) | 13 design proposals (memory scope → context-graph packets) |
| [`docs/specs/`](docs/specs/) | Spec-driven implementation slices |
| [`AGENTS.md`](AGENTS.md) | Repository instructions, boundary rules, validation |

---

## Contributing

Contributions are welcome while the project is pre-1.0, but contract discipline
is strict:

- Start with an issue, ADR, RFC, or spec for behavior changes.
- Keep public contracts compatible unless a breaking change is explicitly
  accepted.
- Keep crate roots and package entry points as narrow facades.
- Do not add god modules, hidden infrastructure coupling, or provider-backed
  behavior in core/domain crates.

Read: [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) ·
[`SECURITY.md`](SECURITY.md) · [`GOVERNANCE.md`](GOVERNANCE.md) · [`AGENTS.md`](AGENTS.md)

---

## License

MIT. See [`LICENSE`](LICENSE).
