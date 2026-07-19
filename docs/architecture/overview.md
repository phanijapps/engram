# Architecture Overview

> The map of this repo. Read this first when exploring the *current* structure.
> Updated whenever the directory layout or major dependencies change. The
> normative counterpart — what new code should *conform to* — is
> [`reference.md`](reference.md). For the functional, use-case-driven intro start
> at the [README](../../README.md); this page is the architectural map beneath it.

Engram is a **contract-first agentic memory layer**: a Rust core that owns
deterministic behavior, wrapped by TypeScript bindings/SDK and MCP servers so
any agent can read and write memory. It separates *what is stored* (rich memory
+ a source-grounded knowledge graph) from *how it is retrieved* (a multi-mode,
fused, policy-filtered pipeline) — the same separation Microsoft Research's
[Memora](https://www.microsoft.com/en-us/research/blog/memora-a-harmonic-memory-representation-balancing-abstraction-and-specificity/)
argues lets an agent scale to long-horizon tasks without re-reading its whole
history. The research grounding for every concept below lives in
[`docs/research/`](../research/README.md); the eight pillars are explained in
the [README's conceptual model](../../README.md#the-conceptual-model).

## How engram works — the memory pipeline

Data flows through engram in one direction. An agent **writes** (or ingests);
engram **processes and stores** across swappable engine cells; a separate
**retrieval composition** layer fuses many recall modes into one policy-filtered
**context packet** returned to the agent. Storage and retrieval are deliberately
decoupled — exactly Memora's "decouple what is stored from how it is retrieved."

![Engram memory pipeline: write/ingest → process → storage cells → retrieval composition → context packet](../architecture/images/pipeline-overview.png)

```
                Agent / host app / MCP client
                   │  write_memory · ingest source · put_entity/relationship
                   ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  WRITE & INGEST PATH   (policy + provenance stamped on every row)│
   │  extract → chunk → embed → persist memory / knowledge graph       │
   └─────────────────────────────────────────────────────────────────┘
        │ memory lifecycle        │ knowledge graph       │ belief synth   │ hierarchy
        ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  STORAGE CELLS   — one crate per engine, swapped by config       │
   │  engram-store-sqlite ◄────► engram-store-surreal ◄────► …        │
   │  memory │ knowledge/taxonomy/ontology │ belief │ hierarchy │ vec │
   └─────────────────────────────────────────────────────────────────┘
        │  consolidation (reflection → derived beliefs  ·  decay → expiry)
        │  runs as an explicit offline pipeline over the store
        ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  RETRIEVAL COMPOSITION   (never one mode)                         │
   │  graph + lexical + vector + cue + beliefs  ─►  RRF fusion         │
   │  ─► cross-encoder rerank  ─►  context packet (with provenance)     │
   └─────────────────────────────────────────────────────────────────┘
        │
        ▼
   Context packet ─► agent   (via Rust facade · N-API/TS · MCP server)
```

The pipeline is the architecture; the sections below name the crates that
implement each stage and the boundaries that keep them swappable.

## Layer responsibilities

Engram is layered so that **deterministic behavior is engine-neutral and lives
in the Rust core**, while **infrastructure (SQL, vectors, models, runtimes)
lives behind replaceable adapters**. A neutrality lint enforces that no engine
type (`Sql*`, `Surreal*`, …) leaks into the neutral layers — this is what makes
backend swap-by-config real ([ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md)).

### Core libraries (`core/`) — the deterministic brain, engine-neutral

Each crate owns one concern behind a trait; none may name SQL, a vector store,
an embedding provider, an async runtime, Node, or TypeScript.

| Crate | Owns | Key surface |
| --- | --- | --- |
| `engram-domain` | portable domain types, invariants, serde, version markers | `MemoryRecord`, `KnowledgeChunk`, `Belief`, `HierarchyNode`, `Policy`, `Provenance` |
| `engram-runtime` | shared primitives only | `CoreError`/`CoreResult`, clocks, ids, scope matching, policy authorizer traits |
| `engram-memory` | memory service + repository ports | `MemoryService`, `MemoryRepository` |
| `engram-knowledge` | source-grounded knowledge, graph, ontology, taxonomy, ingestion ports | `KnowledgeRepository`, `KnowledgeGraphRepository`, `TaxonomyRepository`, `OntologyRepository` |
| `engram-belief` | belief synthesis, contradiction, bi-temporal ports | `BeliefRepository`, `BeliefQuery` |
| `engram-hierarchy` | hierarchy build, navigation, aggregate ports | `HierarchyRepository`, `navigation::navigate` |
| `engram-consolidation` | consolidation planning, gated mutation, decay, audit | consolidation executors |
| `engram-reflection` | reflection synthesizer + consolidation executor (derived beliefs) | reflection pipeline |
| `engram-retrieval` | retrieval composition + fusion ports (store-free seam) | `VectorIndex`, `RetrievalIndex` → `RetrievalFusion` (RRF) → `ContextComposer` |
| `engram-integration` | the SDK facade | `EngramProvider`, `EngramConfig`, `CapabilityReport` |
| `engram-eval` | deterministic fixtures + regression harness | eval fixtures |
| `engram-graph-analytics` | pure, dependency-free graph algorithms | PageRank, betweenness, communities, reachability |

### Adapters (`adapters/`) — replaceable infrastructure, behind the core ports

Two kinds: **engine cells** (one crate per storage engine, implementing the
storage-backed ports) and **engine-agnostic adapters** (retrieval modes,
consolidation executors, ingestion — shared across all engines).

| Crate | Kind | Implements |
| --- | --- | --- |
| `engram-store-sqlite` | engine cell (SQLite) | memory + knowledge/taxonomy/ontology + belief + hierarchy + vector (sqlite-vec) — **all SQLite calls live here** |
| `engram-store-surreal` | engine cell (SurrealDB) | the same five ports over embedded SurrealKV (MTREE vectors) |
| `engram-store-lexical` | agnostic (Tantivy) | `RetrievalMode::keyword` (BM25) |
| `engram-store-associative-graph` | agnostic | `RetrievalMode::Graph` (Personalized PageRank, HippoRAG-style) |
| `engram-store-community-summary` | agnostic | community-summary retrieval (GraphRAG) |
| `engram-rerank-cross-encoder` | agnostic | `RerankStrategy::cross_encoder` (injected scorer) |
| `engram-consolidation-decay` | agnostic | policy-expiry + Ebbinghaus-curve decay |
| `engram-ingest` | agnostic | filesystem/git ingestion + tree-sitter AST chunking |

Adding a new engine is a new cell — see
[Extending the storage layer](../guides/how-to/extend-storage.md).

### Integration facade (`engram-integration`) — the one entry point

`EngramProvider` is the canonical Rust SDK entry: a thin facade that holds
`Arc<dyn>` handles to every port and reports what is wired via
`CapabilityReport`. `EngramConfig` selects the backend by a config string + Cargo
feature; `bootstrap_sqlite` / `bootstrap_surreal` compose the right cells into a
provider. A consumer changes **one config line** to swap SQLite for SurrealDB —
no code change, no migration (fresh store on switch). Every capability is
reachable from here.

### N-API binding (`bindings/node` → `engram-node`) — the TypeScript transport

The N-API bridge is a **transport over Rust behavior, not a second
implementation**: JSON in, JSON out, delegating to the same `EngramProvider`
surface. It is what `@engram/node`, `@engram/client`, and the demo consume.
Surface parity ([AGENTS.md](../../AGENTS.md)) requires every capability reachable
from the Rust facade to also be reachable from the N-API binding — wiring one
surface and leaving the other unwired is not allowed.

## Layout

```text
.
├── AGENTS.md             # canonical agent context + boundary rules (CLAUDE.md points here)
├── contracts/            # portable JSON schemas + generated contract outputs (v1/)
├── core/                 # storage-neutral Rust crates (the deterministic brain)
│   ├── domain/  runtime/  memory/  knowledge/  belief/  hierarchy/
│   ├── consolidation/  reflection/  retrieval/  integration/  eval/
│   └── graph-analytics/  # pure graph algorithms (PageRank, betweenness, communities, reachability)
├── adapters/             # replaceable infrastructure crates (behind traits)
│   ├── sqlite/           # engram-store-sqlite — ALL SQLite calls (memory/knowledge/belief/hierarchy/vector)
│   ├── surreal/          # engram-store-surreal — SurrealDB engine cell (SurrealKV embedded)
│   ├── ingest/           # filesystem/git ingestion + tree-sitter AST chunking
│   ├── retrieval/        # sqlite-vec · tantivy-lexical (BM25) · associative-graph (PPR) · community-summary (GraphRAG) · cross-encoder-rerank
│   ├── consolidation/    # decay executor (policy-expiry, Ebbinghaus)
│   └── integration/      # backend recipe / conformance composition (SQLite wiring home until backends/ split)
├── bindings/node/        # N-API JSON transport (engram-node) — a transport, not a second impl
├── codegraph/            # on-top codegraph layer (RFC-0012): code-specific crates on engram
│   ├── queries/  temporal/  mcp-server/
├── memory/mcp-server/    # memory MCP server: write_memory/recall/forget/put_entity/put_relationship
├── packages/             # TypeScript workspace (contracts / client / node / adapters / eval)
├── prototype/            # enterprise knowledge-platform demo (RFC-0004): Hono backend + React frontend + MCP
├── engram-viz/           # web-based code-graph visualization workspace (engram-viz spec)
├── docs/                 # CHARTER · CONVENTIONS · adr · rfcs · specs · architecture · product · guides · perf · research
└── .codex/  .claude/     # validation hooks + skills/agents for AI contributors
```

## Crates, packages, and apps

- **Core** — `engram-{domain,runtime,memory,knowledge,belief,hierarchy,
  consolidation,reflection,retrieval,integration,eval,graph-analytics}`. Each
  is small, one responsibility, facade-style `lib.rs`. **Look first:**
  `core/integration/src/` for the provider facade.
- **Engine cells** — `engram-store-sqlite` (all SQLite, including sqlite-vec +
  the feature-gated `FastEmbedBgeSmallQueryProvider`), `engram-store-surreal`
  (SurrealKV embedded, MTREE vectors).
- **Agnostic adapters** — `engram-store-lexical` (Tantivy BM25),
  `engram-store-associative-graph` (PPR), `engram-store-community-summary`
  (GraphRAG), `engram-rerank-cross-encoder`,
  `engram-consolidation-decay`, `engram-ingest`.
- **Binding + SDK** — `engram-node` (`bindings/node`); `@engram/node`,
  `@engram/contracts`, `@engram/client`, `@engram/adapters`, `@engram/eval`
  (`packages/*`).
- **Codegraph layer** (on top of engram, RFC-0012) —
  `engram-codegraph-queries`, `engram-codegraph-temporal`, and the codegraph MCP
  server. These depend only on `engram-domain` / `engram-graph-analytics`; they
  do not own storage.
- **Demo** — `prototype/backend` (Hono: ingest, graph, RRF-hybrid Q&A,
  benchmark), `prototype/frontend` (React + shadcn/ui: dashboard, WebGL graph,
  chat), `prototype/mcp` (the demo MCP server). `engram-viz/` is a separate
  web-based code-graph visualization workspace.

## Conventions you'll see across crates/packages

- Every Rust crate is small with one responsibility; crate `lib.rs` files are
  facades (module declarations + narrow re-exports + top-level docs only).
- Errors are typed (`CoreError`); no stringly public contracts.
- Store/vector/embedding/model integrations sit behind traits; tests inject
  deterministic stubs.
- One crate per storage engine; all calls for an engine live inside its cell.
- FastEmbed is a compile-time Cargo feature (`--features fastembed`); the demo
  degrades gracefully without it.
- TypeScript packages keep `index.ts` as narrow facades; the native binding is a
  transport, not a second implementation.
- Validation gates in `AGENTS.md` § Validation (`cargo fmt/check`,
  `pnpm contracts:generate/typecheck/test`, `.codex/hooks/*`).

## Where to start

1. Read [`docs/CHARTER.md`](../CHARTER.md) — mission, scope, principles.
2. Read [`reference.md`](reference.md) (normative golden path) then this overview
   (descriptive map).
3. Read [ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md) (engine grid +
  one-crate-per-backend) and [ADR-0009](../adr/0009-retrieval-composition-seam.md)
  (the load-bearing retrieval-composition decision).
4. Skim [`docs/research/README.md`](../research/README.md) for the synthesized
   research that grounds the model.
5. Pick a recent spec in `docs/specs/` (e.g. `surrealdb-backend`) and read its
   `spec.md` + `plan.md` alongside the code they produced.
6. Run the demo: see the [build guide](../guides/how-to/build-and-run.md), then
   index a repo via the dashboard and watch the RRF-hybrid Q&A.
