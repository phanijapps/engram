# Architecture sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.2.

## Charter

Where engram's semantics live: protect the real seams — composer fusion, adapter cells, registries — not a generic layered diagram.

## Knowledge — how engram is built

**The map:** ~35 Rust crates + 6 TS packages. `core/*` = storage-neutral
behavior (domain, runtime, memory, knowledge, belief, hierarchy,
consolidation, reflection, retrieval, procedures, code, integration...).
`adapters/*` = engine cells (sqlite, pgvector, retrieval/*, embedding/http).
`backends/*` = recipes composing cells into an EngramProvider (ADR-0022:
swap the backend by config, not rewrite). `codegraph/*` = on-top queries.
`bindings/node` = N-API. `packages/*` = TS workspace.

**The seams that matter:**
- Recall composes through the retrieval composer (RRF + lanes); no lane
  bypasses the service layer; failed lanes degrade via `source_failures`
- The capability×engine grid: engines are interchangeable cells behind ports;
  engram-domain and friends never name an engine type (gate-enforced)
- Surface parity: every capability reachable via Rust facade AND N-API AND
  MCP, reflected in CapabilityReport (ADR-0022)
- Domain SSOT: `docs/architecture/domain-data-model.md` until Rust types
  become the generated-contract source
The map and the golden path are absorbed below.

## Source: architecture overview + reference (absorbed)

<!-- absorbed from docs/architecture/overview.md + reference.md (retired 2026-11-01) -->
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

<p align="center"><img src="images/engram-pipeline.svg" alt="Engram — agentic memory pipeline" width="100%"></p>

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
│   ├── consolidation/  reflection/  retrieval/  procedures/  integration/  eval/
│   ├── code/             # deterministic tree-sitter code behavior (ADR-0028)
│   └── graph-analytics/  # pure graph algorithms (PageRank, betweenness, communities, reachability)
├── adapters/             # replaceable infrastructure crates (behind traits)
│   ├── sqlite/           # engram-store-sqlite — ALL SQLite calls (memory/knowledge/belief/hierarchy/vector)
│   ├── pgvector/         # Postgres/pgvector engine cells
│   ├── embedding/http/   # HTTP embedding provider cell (configurable models)
│   ├── ingest/           # filesystem/git ingestion + tree-sitter AST chunking
│   ├── retrieval/        # sqlite-vec · tantivy-lexical (BM25) · associative-graph (PPR) · community-summary (GraphRAG) · cross-encoder-rerank · mmr-rerank
│   ├── consolidation/    # decay executor (policy-expiry, Ebbinghaus)
│   └── integration/      # backend recipe / conformance composition (SQLite wiring home until backends/ split)
├── backends/             # backend recipe crates (ADR-0022): sqlite + pgvector
├── bindings/node/        # N-API JSON transport (engram-node) — a transport, not a second impl
├── mcp/engram-mcp/       # the ONE unified MCP server (RFC-0015): memory + knowledge + beliefs + hierarchy + code intel
├── codegraph/            # on-top codegraph layer (RFC-0012): code-specific crates on engram
│   ├── queries/  temporal/
├── packages/             # TypeScript workspace (contracts / client / node / adapters / eval)
├── prototype/            # enterprise knowledge-platform demo (RFC-0004): Hono backend + React frontend + MCP
├── engram-cc/           # web-based code-graph visualization workspace (engram-cc spec)
├── docs/                 # README (front door) · adr · architecture · constitution · contracts · guides · history · product · research · rfcs
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
  chat), `prototype/mcp` (the demo MCP server). `engram-cc/` is a separate
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

1. Read [`docs/product/engram.md`](../product/engram.md) — mission, scope, principles.
2. Read [`reference.md`](reference.md) (normative golden path) then this overview
   (descriptive map).
3. Read [ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md) (engine grid +
  one-crate-per-backend) and [ADR-0009](../adr/0009-retrieval-composition-seam.md)
  (the load-bearing retrieval-composition decision).
4. Skim [`docs/research/README.md`](../research/README.md) for the synthesized
   research that grounds the model.
5. Pick a recent spec in `docs/specs/` (e.g. `fetch-content-backend`) and read its
   `spec.md` + `plan.md` alongside the code they produced.
6. Run the demo: see the [build guide](../guides/how-to/build-and-run.md), then
   index a repo via the dashboard and watch the RRF-hybrid Q&A.


---

# Reference architecture

> **Normative.** This is engram's *golden path* — the stack, internal building
> blocks, component stereotypes, and cross-cutting standards that new work
> **conforms to**. A feature's low-level design (in its plan) names which blocks
> it reuses and which standards it follows, and justifies any deviation.
>
> This is the **normative** sibling of `overview.md`. `overview.md` *describes*
> how the code is organized today; `reference.md` *prescribes* how new code
> should be shaped. When they disagree, that gap is either drift to fix or a
> decision to record (ADR). Source of truth for the rules: `AGENTS.md` and the
> ADRs under `docs/adr/`.

## Constraints

- **Technical constraints.** Rust workspace (edition per workspace) + TypeScript
  workspace (pnpm). The Rust core is the deterministic brain; TypeScript owns
  integration ergonomics; the Node N-API binding (`engram-node`) is a JSON
  transport between them. Durable storage is SQLite in **WAL** mode (concurrent
  reads during writes); vectors in **sqlite-vec**; embeddings via **FastEmbed
  BGE-small**, feature-gated and opt-in. The LLM is reached through the **pi SDK**
  from TypeScript only — never from Rust. **Deployment platform:** demo-scale
  local deployment today (Hono backend on `:8787`, Vite React frontend); no
  managed runtime or orchestrator yet — production targeting is a future ADR.
- **Organizational / process constraints.** **Contract-first:** `docs/architecture/domain-data-model.md` is the source of truth until Rust domain types are accepted as the generated-contract source. **Boundary rules** in `AGENTS.md` outrank local preference (no god modules; crate roots are facades; store/vector/embedding/model/gateway integrations live in adapters). **ADR-gated:** no runtime manifests or implementation code until `docs/adr/0003-implementation-stack.md` exists; run `.codex/hooks/pre-implementation-check.sh` before implementation work. Compatible vs breaking contract changes are classified explicitly.
- **Constraints you cannot change here.** `engram-domain` must not depend on SQL, vector stores, embedding providers, async runtimes, Node, N-API, or TypeScript tooling. The LLM client stays in TypeScript. Embeddings live behind traits so tests use deterministic stubs. The retrieval-composition seam (RFC-0005 / ADR-0009) is **read-path only** — distributed cross-store write consistency is out of scope.

## Solution strategy

- **Architectural style.** Layered and contract-first: portable domain contracts → Rust deterministic core → replaceable infrastructure adapters → native binding (transport) → TypeScript packages (ergonomics) → demo. Memory, knowledge, belief, hierarchy, policy, provenance, and evaluation concepts are kept distinct unless an ADR merges them. State lives behind repository ports; behavior lives in focused modules named for their responsibility.
- **Key technology decisions.**
  - **Rust for deterministic behavior, TypeScript for integration ergonomics** — each owns what it's best at; neither re-implements the other.
  - **SQLite (WAL) + sqlite-vec** for durable, concurrent-read graph + vector storage — zero-ops, file-portable; the read path is backend-agnostic behind `RetrievalIndex` so Postgres/pgvector/Neo4j are additive later (ADR-0009).
  - **Reciprocal Rank Fusion** (`core/retrieval`) for cross-source hybrid retrieval — score-free, robust across incomparable backends; strength configurable, defaults when absent.
  - **N-API JSON transport** — the binding serializes round-trips into Rust behavior; it is a transport, not a second implementation.
  - **Lazy query-time embeddings** (feature-gated FastEmbed) — indexing stays embedding-free; embeddings amortize across queries and persist to `${ENGRAM_DB}.embeddings.db`.
- **Quality-goal strategy.** *Correctness* via contract-first + typed errors + traits-with-stubs. *Modularity* via small crates with explicit responsibilities and the `AGENTS.md` boundary rules. *Testability* via provider/store traits so deterministic stubs replace network/model calls.

## Building-block view / component catalogue

- **Component stereotypes.**
  - **Domain crate** (`engram-domain`) — portable types, invariants, serde, version markers. Depends on nothing infra.
  - **Runtime crate** (`engram-runtime`) — shared primitives: `CoreError`/`CoreResult`, clocks, id generation, scope matching, policy authorizer traits.
  - **Behavior crate** (`engram-memory`, `engram-knowledge`, `engram-retrieval`) — service + ports for one concern each. `engram-retrieval` owns fusion + composition ports and is **store-free**.
  - **Orchestration facade** (`engram-core`) — compatibility re-export layer above split behavior crates; must not re-become the canonical owner of memory/knowledge ports.
  - **Adapter crate** — replaceable infrastructure behind traits (`adapters/sqlite`, `adapters/pgvector`, `adapters/retrieval/*`, `adapters/ingest`).
  - **Binding** (`engram-node`) — N-API JSON transport over Rust behavior.
  - **Package** (`packages/*`) — TypeScript facades; `index.ts` files are narrow public surfaces.
- **Reusable building blocks.**
  - `engram-domain` types + `engram-runtime::{CoreError, CoreResult}` — the error/result surface every crate speaks.
  - **Retrieval-composition seam** (ADR-0009): `RetrievalIndex` (per source) → `RetrievalFusion` (`ReciprocalRankFusion` / `WeightedRetrievalFusion`) → `ContextComposer` → `ContextPayload`. New retrieval sources implement `RetrievalIndex`; fusion stays in `core/retrieval`.
  - The scope/tenant model (`Scope { tenant, workspace, environment, … }`) + `Policy` on write/retrieve/ingest/consolidate/forget paths.
- **Composition rules.** Dependency direction: `domain` ← `runtime` ← `{memory, knowledge, retrieval}` ← `orchestration` ← adapters ← binding ← packages/demo. `engram-domain` depends on nothing infra; `engram-retrieval` calls no stores/providers/policy engines; adapters implement core ports; the binding imports no behavior that duplicates Rust. A file mixing construction + validation + state + orchestration + scoring + persistence + error translation must be split by boundary before handoff.

## Crosscutting concepts / standards

- **Error handling.** Typed `CoreError` (variants: `Adapter`, `InvalidRequest`, …) and `CoreResult<T>` everywhere in Rust; no stringly-typed public error contracts. The binding translates `CoreError` to N-API errors; the demo translates to HTTP. Q&A + retrieval paths fail closed (degrade to a safe baseline) rather than throw.
- **Observability.** Today: console logging in the demo backend; no metrics/tracing pipeline yet — a known gap to record before any production target. Ground truth for a change is the demo's HTTP response + the benchmark output (`/bench`, `/bench/lazy`).
- **Security & data handling.** Scope/tenant isolation on every read/write. Policy checks visible on write, retrieve, ingest, consolidate, and forget paths. **API keys read server-side only** — never sent to the frontend, never logged, redacted from errors. Repo scanning canonicalizes paths and confines reads under root; secret-laden files excluded by name/extension. **LLM output is validated before graph writes**; LLM calls are bounded (~30 s, ~100 KB, input truncated). `.env` is gitignored; only `.env.example` is committed; the pi SDK is pinned and `pnpm audit` kept clean.
- **Configuration & environments.** `.env` for the demo (`ENGRAM_DB`, `ENGRAM_EMBEDDINGS_DB`, `ENGRAM_LLM_{BASE_URL,API_KEY,MODEL}`, `ENGRAM_LAZY_EMBEDDINGS`, `ENGRAM_LAZY_POOL`). Scope selects tenant/workspace/environment. FastEmbed is a **compile-time** Cargo feature (`--features fastembed`) with runtime guards so the demo degrades gracefully without it.
- **Testing standards.** Rust: focused unit tests for invariants + integration tests for vertical flows (`cargo test`); `#[ignore]` for model-dependent tests. TypeScript: type tests + compile checks. Contracts reproducible from source (`pnpm run contracts:generate`); never hand-edited. **Verification tooling** lives in `AGENTS.md` § Validation (`cargo fmt --all`, `cargo check --workspace`, `pnpm run contracts:generate / typecheck / test`, `pnpm run build`, `.codex/hooks/check-contracts.sh`, `.codex/hooks/check-docs.sh`) — the work-loop infra preflight reads these as the canonical gate set.


## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| ARC-101 | Portable domain types/invariants MUST stay in `engram-domain`, conforming to the accepted model/contracts; memory, knowledge/ontology, belief, hierarchy, retrieval and consolidation MUST keep separate behavioral owners. New work MUST NOT recreate the retired `engram-core` facade as a canonical port owner or persist knowledge state inside the memory repository for convenience. | refines: CI-001, CI-004, PR-003; enforced-by: check_implementation_registry.py (declared ownership/imports only); boundary review gap. | `docs/contracts/constitution/v1/implementation/registry.json:13-46`; `Cargo.toml:3-19`; `docs/architecture/reference.md:30,44-53`; `core/memory/src/lib.rs:19-35`. |
| ARC-102 | Engine operations MUST remain behind core ports in declared adapter/recipe zones. SQLite's existing `core/integration/src/sqlite/` zone MUST be identified as exempt composition, not evidence that neutral facade files may acquire SQL; a new engine MUST compose the same ports, not fork domain behavior. | refines: EN-001, EN-003, PR-004; enforced-by: check_engine_neutrality.py (listed paths/patterns only); recipe conformance/review. | `docs/adr/0022-engine-grid-vs-backend-recipe.md:8-29,89-111`; `core/integration/Cargo.toml:30-38`; `backends/sqlite/src/lib.rs:7-13`; `backends/pgvector/src/lib.rs:29-35`. |
| ARC-103 | Unified recall MUST obtain the facts lane through `MemoryService::retrieve`, other retrieval candidates through `RetrievalIndex` and beliefs through their repository, then reuse `ReciprocalRankFusion` and `compose_context`; no new lane or transport may substitute its own ranking/composer or preempt the final shared budget. | refines: CI-001, CI-002, PR-005; recall fixtures and focused review, not proven by neutrality. | `core/integration/src/recall.rs:21-30`; `core/integration/src/sqlite/recall.rs:195-219,249-278,401-415`; `docs/architecture/reference.md:49-53`. |
| ARC-104 | `engram-code` MUST own deterministic symbol identity, typed code edges and reference resolution without filesystem/git/storage/LLM behavior; `adapters/ingest` MUST own acquisition/persistence, and `engram-lang` additions MUST register grammars/kind maps rather than fork the chunker. Unresolved references MUST remain visible rather than become invented call edges. | refines: CI-001, PR-002, PR-004; code/language fixtures and dependency review gap. | `core/code/src/lib.rs:1-8,19-36`; `core/lang/src/lib.rs:1-20`; `docs/product/engram.md:54-69`. |
| ARC-105 | Graph algorithms MUST remain dependency-free in `engram-graph-analytics`; code-specific dead-code/blast-radius/dependency-path behavior MUST stay in the on-top `codegraph/` crates, consuming Engram contracts/algorithms rather than owning another store or MCP server. | refines: CI-001, CI-002, PR-003; manifest/ownership review; inventory gate is only partial proof. | `core/graph-analytics/Cargo.toml:1-6`; `codegraph/queries/Cargo.toml:8-12`; `docs/contracts/constitution/v1/implementation/registry.json:71-74,89-99`; `Cargo.toml:32-36`. |
| ARC-106 | N-API, TS packages and unified MCP handlers MUST delegate to canonical Rust ports/provider behavior; JSON translation, host configuration and optional agent-side extraction MUST NOT become a second memory/retrieval implementation or a Rust-hosted LLM client. | refines: CI-002, PR-002, SP-001; enforced-by: check_implementation_registry.py, check_production_completeness.py (imports/dependency patterns only), check_docs.py (TS delegation subcheck). | `docs/contracts/constitution/v1/implementation/registry.json:61-68`; `mcp/engram-mcp/src/app.rs:1-24`; `.agents/skills/engram-distill/SKILL.md:8-18`; `tools/ci/check_docs.py:57-65`. |
| ARC-107 | Ontology-pack conversion/slicing MUST be deterministic bytes-to-domain-record behavior in `core/packs`; fetch/cache/pin verification and launch-config I/O MUST remain in the MCP host path, while durable writes reuse provider ontology/taxonomy ports. Constitution packs MUST NOT be confused with these runtime vocabulary packs. | refines: CI-001, PR-002, PR-004; pack conversion/install fixtures; manual boundary review. | `core/packs/src/converter.rs:1-9,40-60`; `core/packs/src/slice.rs:1-12`; `mcp/engram-mcp/src/pack.rs:1-14`. |
| ARC-108 | Consolidation MUST remain an explicit request→plan/run operation through injected mutation executors, not an incidental write side effect; only `dry_run == Some(false)` may apply. Gated mutation MUST keep pre/post evaluation and apply authorization visible and return task-level audit outcomes; the default allow-all apply gate MUST NOT be presented as host policy enforcement or a post-run regression as automatic rollback. | refines: CI-001, PR-005, PC-001; consolidation planning/apply and production-wiring fixtures; recipe policy/evaluation coverage must be inspected separately. | `core/consolidation/src/consolidation/mutating.rs:56-105,109-150,165-267`; `adapters/integration/tests/consolidation_dry_run.rs:1-9,87-95`; `docs/product/engram.md:118-125`. |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.


## Root rails absorbed into this pack (IDs and gates unchanged)

### From 01-engine-neutrality.md

# Rail 01 — Engine neutrality

Rule classes: RULE-1 (single implementation), RULE-6 (no orphans).
Owner: core maintainers. Gate: `tools/ci/check_engine_neutrality.py` (CI: rust job).

## Rules

- **EN-001 (MUST)**: When CI scans the neutral boundary, the gate shall reject every
  engine type, engine import, or SQL literal matched by the committed lint patterns in
  that boundary.
- **EN-002 (MUST)**: When a pull request changes `GATED_PATHS` or the forbidden patterns,
  the rule review shall reject any unexplained reduction in enforcement scope.
- **EN-003 (MUST)**: When backend composition changes, implementation owners shall
  preserve core port contracts and keep engine operations in declared adapter or
  composition zones.
- **EN-004 (MUST)**: When a declared neutral path is absent, CI shall fail rather than
  silently skip that path.
- **EN-005 (SHOULD)**: When neutrality coverage expands to a presently excluded layer,
  maintainers shall declare its exact paths and measured obligations before asserting
  that layer is governed.

## Enforcement boundary (honest, as of 2026-10-31)

The executable `GATED_PATHS` covers domain, memory, knowledge, retrieval, belief,
hierarchy, consolidation, procedures, proposals, packs, selected `core/integration`
files, and `mcp/engram-mcp/src`. It does NOT scan all of `core/runtime`, `core/eval`,
`bindings/node`, or `core/integration/src/config.rs` — do not infer those are proven
neutral. A green run of this gate discharges EN-001 only; EN-002..EN-005 have known gaps
recorded below and are not discharged by a passing regex.

## Gate gaps (tracked, not waived)

- CLOSED 2026-10-31 (job 011): missing declared paths now fail as named violations,
  with a negative fixture in tools/ci/tests/test_engine_neutrality.py.
- EN-002: needs scope-regression comparison against an accepted baseline.
- EN-003: needs registry/dependency-direction validation and backend conformance tests
  rather than regex alone.

## Deviation path

Remove or relocate the forbidden dependency first. If temporary noncompliance remains,
record a scoped, approved, expiring exception in `DEVIATIONS.md` (RULE-5 failure stands).
A planned coverage expansion carries a ratified decay clause in `DECAY-WINDOWS.md` with a
measured baseline and terminal strict gate; a decay clause MUST NOT retroactively
legalize a new EN-001 regression.

## History

- 2026-10-31: rail locked at adoption (owner ratification, plan §7 ask 1).

### From 02-surface-parity.md

# Rail 02 — Surface parity

Rule classes: RULE-1, RULE-6. Owner: core + bindings maintainers.
Gates: `tools/ci/check_surface_parity.py` (existing), `tools/ci/check_bound_code_matrix.py` (job-scoped).

## Rules

- **SP-001 (MUST)**: Every runtime capability, operation, and retrieval mode exposed by
  the Rust `EngramProvider` facade MUST also be reachable through the N-API binding
  (`bindings/node`) and any other supported transport, and MUST be reflected in
  `CapabilityReport`.
- **SP-002 (MUST)**: A capability is not "shipped" until every surface can invoke it;
  wiring one surface and leaving another unwired (transport asymmetry) is a violation.
- **SP-003 (MUST)**: If a surface genuinely cannot carry a capability, the reason MUST be
  recorded in an ADR referenced from `implementation/registry.json` (transport_wrappers
  entry), not left implicit.
- **SP-004 (SHOULD)**: Parity checks SHOULD verify operation-level reachability, not only
  symbol-name presence.

## Enforcement boundary

`check_surface_parity.py` verifies that every facade `require_*` capability has an N-API
`require*Api` proxy (name-level check, SPECIAL_CASES ledger). Operation-level parity is
partially covered by `check_bound_code_matrix.py` (fail-closed operation × backend ×
transport matrix), currently run job-scoped rather than in CI.

## Gate gaps

- SP-004: promote the bound-code matrix into CI; extend name checks toward
  operation-level checks.

## Deviation path

Same as rail 01: fix the asymmetry; otherwise a scoped, expiring DEVIATIONS entry while
RULE-5 stands. Sequencing an adapter unit ahead of its wiring is legitimate (see
`lexical-keyword-retrieval` + `lexical-wiring` precedent) only with a decay window
declaring the wiring deadline.

## History

- 2026-10-31: rail locked at adoption.

### From 03-canonical-implementations.md

# Rail 03 — Canonical implementations

Rule class: RULE-1 (single implementation).
Gate: `tools/ci/check_implementation_registry.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/implementation/registry.json`.

## Rules

- **CI-001 (MUST)**: Every named capability MUST have exactly one canonical
  implementation (`ssot_module`) recorded in the implementation registry.
- **CI-002 (MUST)**: Transport wrappers (TS client, N-API, MCP tools) MUST import or
  delegate to the canonical implementation; a wrapper that reimplements the concept's
  logic is a violation.
- **CI-003 (MUST)**: A second implementation of an existing capability MUST NOT be added;
  if a legitimate replacement is intended, the registry entry's migration block MUST
  record status and deadline, and the old implementation MUST be removed by then.
- **CI-004 (MUST)**: Domain truth lives in `docs/architecture/domain-data-model.md` until Rust domain
  types are accepted as the generated-contract source; generated contracts
  (`docs/contracts/v1`, `packages/contracts`) MUST be reproducible from source and MUST NOT be
  hand-edited.

## Enforcement boundary

Registry seeded at adoption with the capability set (memory, knowledge, retrieval,
belief, hierarchy, consolidation, procedures, code-index, provider facade, MCP server,
launcher). The drift gate (planned) fails on: duplicate `concept_id`, a concept whose
`ssot_module` path does not exist, or a wrapper that does not import the SSOT package.

## Deviation path

None for CI-001 duplicates; migrations carry a decay window with a terminal removal date.

## History

- 2026-10-31: rail locked at adoption; registry seeded (L1 structural).

### From 08-coherent-integration.md

# Rail 08 — Coherent integration (no orphans)

Rule class: RULE-6 (coherent integration / no orphans).
Gate: `tools/ci/check_coherent_integration.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/components/registry.json`.

## Rules

- **OR-001 (MUST)**: Every named component — Rust workspace crate, TS package, transport
  binding, gate script, registry — MUST be declared in the component registry, and every
  declaration MUST resolve to an existing path (bidirectional bijection).
- **OR-002 (MUST)**: A component that exists on disk but is referenced by no registry is
  an orphan and a violation; a registry entry pointing at nothing on disk is a dangling
  declaration and equally a violation.
- **OR-003 (MUST)**: Documentation that declares repository shape (AGENTS.md tree
  diagrams) MUST agree with the registry inventory; drift between them is a violation of
  both this rail and documentation truth (tracked in DECAY-WINDOWS at adoption).
- **OR-004 (SHOULD)**: Factory job directories SHOULD contain only the canonical
  artifact set at top level; orchestration receipts (dispatch results, batch outputs,
  recovery files, rerun churn) MUST live under `runs/` subdirectories (see job-layout
  policy in the engram-factory skill; enforced by `check_job_artifacts.py` layout check).

## Enforcement boundary

Component registry seeded at adoption with the 35 workspace crates + 6 TS packages +
transports + gates. The bidirectional gate (planned) diffs registry ↔ disk. The
job-layout check ships now with `check_job_artifacts.py` (advisory on historical jobs —
see DECAY-WINDOWS for the job-010 sprawl window).

## Deviation path

None: an orphan is deleted or declared in the same change that introduces it.

## History

- 2026-10-31: rail locked at adoption; registry seeded.

