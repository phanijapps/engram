# RFC-0017: Three-module target architecture (Ingestion / Retrieval+Mutation / Maintenance) + pgvector backend

Status: Draft
Constrained by: ADR-0003 (implementation stack), ADR-0022 (engine neutrality, capability × engine grid, backend = recipe), RFC-0015/0016 (unified MCP, code as the final layer)

## Reviewer brief

- **What changes:** engram moves from a single mixed `engram-mcp` server (which today
  does bulk ingestion, retrieval, and maintenance in one process) to **three explicit
  modules** with one job each, all sharing one `EngramProvider` + fused-per-project scope.
  A **pgvector (Postgres)** backend is added as the second engine.
- **Affected surface:** a new `ingest` service/CLI (events / streaming / scheduled
  triggers); `engram-mcp` narrows to **query + light agent mutation** (retrieval +
  `write_memory`/`put_entity`/`belief_put`-class writes); a new `maintenance` service
  (consolidation / reflection / decay / hierarchy / taxonomy / GC); a `backends/pgvector`
  recipe + adapter cells.
- **Not in scope:** changing the domain model or the port traits (every module composes
  the same ports); a second non-code domain (the ontology/taxonomy swap already covers
  that); Neo4j (the split pgvector+Neo4j deployment stays a backlog alternative).

## The ask

Accept the **three-module target** as engram's structural direction and the **pgvector**
backend as the path to scale, so the individual slices (ingestion service, MCP narrowing,
maintenance service, pgvector adapter) can be specced against a shared target instead of
ad hoc.

## Problem & goals

Today `engram-mcp` is one process that does three different kinds of work with very
different operational profiles:

1. **Bulk ingestion** — `scan_repo`, `scan_protocols`, `scan_dependencies`,
   `scan_ownership` walk a whole repository. Heavy, long-running, batch-shaped.
2. **Agent retrieval + mutation** — `recall`, `get_context`, `write_memory`, `put_entity`,
   `belief_put`. Light, latency-sensitive, interactive.
3. **Maintenance** — `consolidate`, `belief` derivation, decay, hierarchy rebuild.
   Background, scheduled, whole-graph-shaped.

Mixing them in one stdio MCP means the agent's `recall` competes with a 60-second
`scan_repo` in the same event loop, the heavy tools are awkward to trigger from a
scheduler/stream, and SQLite (single writer) serializes all three. The goal is **one
module per concern**, each independently deployable and schedulable, over a storage layer
that handles concurrent writers (Postgres/pgvector).

## Proposal

### The three modules

```
                       ┌─────────────────────────────────────────┐
   Events   ─────────▶ │  1. INGESTION                           │
   Streaming ────────▶ │     events · streaming · scheduled      │
   Scheduled ────────▶ │     (scan_repo family, imports, ETL)    │
                       └──────────────────┬──────────────────────┘
                                          │  writes via ports
                                          ▼
   MCP (stdio/http) ──┐ ┌─────────────────────────────────────────┐
   Skills ────────────┼▶│  2. RETRIEVAL + MUTATION                 │
   API ───────────────┘ │     query + add/update memories          │
                        │     (recall, write_memory, beliefs, KG)  │
                        └──────────────────┬──────────────────────┘
                                           │  reads + light writes
                        ┌──────────────────┴──────────────────────┐
                        │  3. MAINTENANCE  (background / sleep)    │
                        │     consolidation · reflection · decay   │
                        │     hierarchy · taxonomy · GC · dedup    │
                        └─────────────────────────────────────────┘
                              all three compose one EngramProvider
```

- **Module 1 — Ingestion `[events, streaming, scheduled]`.** The write-path for getting
  data *in*. Three trigger types over the same ports:
  - **Scheduled** — periodic batch (the current `scan_repo` family becomes a scheduled
    job; periodic imports/ETL).
  - **Events** — discrete event-driven writes (webhooks, message signals, agent-action
    events).
  - **Streaming** — continuous ingestion (Kafka/Redpanda topics, log/event streams).
  Ingestion owns the heavy, batch/stream-shaped work; it does not serve interactive
  queries.

- **Module 2 — Retrieval + mutation `[MCP, Skills, API]`.** The agent-facing surface.
  Its primary job is **query** (`recall`, `get_context`, `graph_neighbors`,
  `resolve_entity`, `search`). It also does **light, agent-initiated memory mutation**
  (`write_memory`, `put_entity`, `put_relationship`, `belief_put`, `forget`) — the
  "remember this" class, synchronous and small. It does **not** run bulk ingestion; the
  `scan_*` tools migrate to Module 1. Exposed as **stdio MCP**, **http MCP** (query
  mode), **Skills**, and a direct **API**.

- **Module 3 — Maintenance.** The "sleep-time" keeper that keeps the knowledge layer
  healthy: **consolidation** (memory→fact→belief), **reflection** (derived beliefs),
  **decay** (retention/forgetting), **contradiction** resolution, **hierarchy** rebuilds,
  **taxonomy** evolution, and **GC/dedup**. Runs on schedule/triggers, whole-graph-shaped.
  Builds on the existing `engram-consolidation` / `engram-reflection` / decay-adapter
  foundation, formalized as a service.

### The separation principle (the load-bearing idea)

> **Bulk ingestion (Module 1) is separated from interactive retrieval+mutation (Module 2).**

Concretely: today `scan_repo` is an MCP tool (a Module-2 concern doing Module-1 work). In
the target, `scan_repo` and friends are **Module-1 ingestion jobs**; the MCP narrows to
"just query + light writes." So *"when I expose it as a stdio/http MCP it will just query"*
— the heavy lifting is out of the request path. An agent that wants to scan invokes
Module 1 (or a thin MCP action that *enqueues* a Module-1 job), never blocks the query loop.

### Realization: a TS operational layer over a held N-API provider

The three modules are realized as **TypeScript entry points over one long-lived
`EngramProvider` handle**, not as three Rust binaries:

- **Rust core** owns deterministic operations only — `scan`, `ingest`, `recall`,
  `write_memory`, `consolidation.plan` / `run_maintenance_step` / `apply_decay`,
  `build_hierarchy`. No scheduler, no network, no LLM, no async runtime (the
  existing boundary rules + ADR-0022 engine neutrality).
- **TypeScript layer** (`packages/` + a runtime/app) owns the operational glue —
  the Kafka/queue consumer, the webhook receiver, the lightweight HTTP MCP, the
  consolidation worker/scheduler, and the distill LLM. These are JS-ecosystem
  strengths and AGENTS.md assigns model integrations + transport to TypeScript.

> **Keystone invariant (surface parity, ADR-0022).** A capability is reachable
> from the TS modules only if it crosses the N-API boundary on a **single held,
> engine-routed provider** — `NativeProvider` (`bindings/node/src/provider.rs`),
> which holds an `EngramProvider` and reaches every capability through typed
> handle proxies, **including consolidation execution** (`consolidate_json` →
> `ConsolidationService::consolidate`). The surface-parity gate passes with **zero
> acknowledged debt**. What is *not* yet done: no TS facade consumes
> `NativeProvider` (`packages/` still uses the flat `NativeKnowledgeEngine`), and
> `scan` is composed at the call site (knowledge + graph handles) rather than
> exposed as a dedicated method. The `engram-mcp` Rust binary stays as the **stdio
> MCP** — the single-agent default and deterministic fallback — while the TS
> modules are the deployable multi-worker surface.

### One write-path, many triggers (consolidating ingestion + streaming)

"Consolidate ingestion and streaming" is **not** three things to build — it is
three trigger shapes over one Module-1 write-path. Engram ships the write entry
point (`scan` / `ingest` / `store_knowledge` over the held provider); the TS
layer ships reference transport adapters (cron, a queue consumer, a webhook
receiver) that each call that entry point per record. Engram does **not** ship a
Kafka framework, a cron daemon, or a worker runtime — the **deployment owns the
scheduler, the transport, and the LLM** (mirroring the agentzero integration
rule: engram = pure library of operations; the consumer owns timers/triggers).

### pgvector as a store (Module 0 — the substrate)

Add **Postgres + pgvector** as the second backend:

- **One Postgres** holds graph + chunks + embeddings (the `pgvector(graph+vector)`
  backlog item, RFC-0005 §Target deployments). Relational tables for entities/edges/
  chunks/sources; `pgvector` for the embedding index; `tsvector` (or `pg_trgm`) for the
  keyword lane.
- Per **ADR-0022**, this is *additive adapter cells* + a **backend recipe**, not a core
  change: `adapters/<capability>/pgvector` cells behind the existing ports, composed by a
  `backends/pgvector` recipe. SQLite stays the local/single-user default; backend is
  chosen by config.
- **Why now:** the three-module target has *concurrent writers* (ingestion + mutation +
  maintenance writing at once). SQLite's single-writer model serializes them; Postgres
  handles concurrent writers + larger scale natively. The architecture motivates the
  store.

## Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Adopt the **3-module** boundary as the target (Ingestion / Retrieval+Mutation / Maintenance). | Different operational profiles (batch vs interactive vs background) shouldn't share one event loop; one module per concern is maintainable. |
| D2 | **MCP narrows to query + light agent mutation.** The `scan_*` (bulk) tools migrate to Module 1. | Keeps the agent's request path latency-bounded; heavy ingestion is schedulable/streamable, not synchronous. |
| D3 | Module 1 supports **events / streaming / scheduled** triggers over the same ports. | Covers webhook-driven, continuous, and periodic ingestion uniformly; the ports don't care about the trigger. |
| D4 | Module 3 **formalizes** consolidation/reflection/decay/hierarchy/taxonomy/GC as a service (builds on existing crates). | The maintenance foundation exists; it needs a service shell + scheduler, not new domain logic. |
| D5 | **pgvector** is the second backend (graph+vector in one Postgres), via ADR-0022 adapter cells + recipe; SQLite stays default. | Concurrent writers + scale; backend swap-by-config preserved; no core/contract change. |
| D6 | All three modules share **one `EngramProvider`** + fused-per-project scope; no module owns storage. | Modules compose adapter cells; storage neutrality stays intact. |
| D7 | The modules are realized as **TS entry points over the existing held `EngramProvider` N-API handle (`NativeProvider`)**, not three Rust binaries. That handle + consolidation execution already exist and pass the parity gate (0 debt); the remaining work is the TS facade + a scan entry point the modules sit on. | Kafka/HTTP-MCP/scheduler/LLM are JS-ecosystem strengths + AGENTS.md assigns them to TS; one held provider preserves engine routing + surface parity; the stdio `engram-mcp` binary stays as the single-agent default. |
| D8 | The **scheduler, transport, and LLM live out-of-process** in the TS layer; engram ships the write/query/maintain operations + reference adapters, never a framework. | One write-path / many triggers; mirrors the agentzero rule (engram = pure library of operations); avoids a god-module that couples timers, queues, and model wiring. |

## Current state → target

| | Current | Target |
|---|---|---|
| N-API | `NativeProvider` holds the provider + reaches all 20 capabilities incl. consolidation **execution** (parity gate: 0 debt); BUT `packages/` still use the flat `NativeKnowledgeEngine`, and `scan` is composed at the call site rather than a dedicated method | TS facade over `NativeProvider` (flat engine retired); scan reachable as a first-class entry point |
| Ingestion | `scan_*` live as MCP tools; no event/stream/scheduled framework | TS Module 1 over the held provider; cron first, then queue/webhook adapters |
| Retrieval + mutation | MCP ~37 tools (query + mutation + ingestion mixed) | TS HTTP MCP + the stdio binary, narrowed to query + light mutation; `scan_*` moved out |
| Maintenance | `consolidate` + reflection + decay wired into the provider, not driven as a job | TS Module 3 worker; `plan` → `run_maintenance_step` → `apply_decay` |
| Storage | SQLite single-file WAL (single writer) | SQLite default **+ pgvector recipe** (concurrent writers) |

## Phased plan

Each phase → its own spec under `docs/specs/`, run through `work-loop`.

| Phase | Scope | Decisions | Depends on |
|---|---|---|---|
| **A — TS facade + scan entry point** | The held `NativeProvider` + consolidation execution **already exist** (parity gate: 0 debt). Add a TS facade over `NativeProvider` in `packages/` (replacing flat `NativeKnowledgeEngine` usage) + make `scan` reachable as a first-class entry point (compose knowledge+graph handles, as the MCP does, or a dedicated binding method). | D6, D7, D8 | ADR-0022 (done) |
| **B — pgvector recipe** | promote the existing 8 adapter cells into a `backends/pgvector` recipe crate + conformance (cells + postgres bootstrap exist; the recipe crate does not); SQLite stays local default | D5 | ADR-0022 (done) |
| **C — TS Module 1: ingest** | TS ingest entry point over the held provider; cron first, then queue/webhook adapters | D1, D2, D3, D8 | A |
| **D — TS Module 2: HTTP MCP** | lightweight HTTP MCP + query/mutation API over the held provider; MCP narrowed to query + light mutation | D2 | A |
| **E — TS Module 3: maintenance** | TS maintenance worker over `consolidate_json` (`ConsolidationRequest` with `dry_run`/`since`/`until`); scheduler/timer owned by the deployment | D1, D4, D8 | A |
| **F — Events + streaming** | queue-consumer + webhook transport adapters feeding Module 1's write-path | D3, D8 | C |

**Sequencing rationale.** The held provider + consolidation execution are **already
built** — Phase A is mostly *adoption*: a TS facade over `NativeProvider` (retiring the
flat `NativeKnowledgeEngine`) + a scan entry point. C/D/E are the three TS modules, each
one PR-width on top of A's facade. pgvector-as-recipe (B) unblocks the concurrent-writer
requirement the three workers create; it is independent of A and can land in parallel.
F (streaming/events) is the furthest out and the most operationally involved; it needs a
transport probe (NATS/Redpanda/Redis Streams) before speccing.

## Risks

- **pgvector breadth** — porting every capability to Postgres is a lot of adapter cells.
  Mitigation: scope the first pgvector recipe to the read/write hot path (memory +
  knowledge/graph + vector + lexical), add belief/hierarchy after.
- **MCP narrowing is a behavior change** — agents/tools calling `scan_repo` over MCP
  break until they call Module 1. Mitigation: keep a thin MCP `enqueue_scan` that hands
  off to Module 1 during the transition; deprecate explicitly.
- **Three services to operate** — more moving parts than one process. Mitigation: all
  three are the same binary with different entry points / configs (a `--mode
  ingest|query|maintain` flag), deployable as one or three.

## Open questions

1. ~~**One binary, three modes** vs three binaries?~~ **Resolved (D7):** neither — the
   modules are **TS entry points over a held N-API provider**. The Rust `engram-mcp`
   binary stays as the stdio MCP (single-agent default); the three modules are TS
   programs/apps the deployment runs as one or three processes.
2. ~~Does Module 2's **API** transport carry the full mutation surface?~~ **Resolved (D2):**
   yes — query + light agent mutation (`write_memory`, `put_entity`, `belief_put`, `forget`).
   Bulk ingestion (`scan_*`) is excluded; it lives in Module 1.
3. **Streaming substrate** — Kafka/Redpanda/NATS/Redis Streams? Defer to Phase F with a
   transport probe before speccing.
4. pgvector **keyword lane** — `tsvector`, `pg_trgm`, or keep Tantivy behind a lexical
   adapter cell? (Leaning: Tantivy adapter cell reused, to avoid a second keyword impl.)

## What "done" looks like (for the accepted scope)

- A **TS facade over the existing `NativeProvider`** on which query, mutation, scan, and
  consolidation **execution** are all reachable — retiring the flat
  `NativeKnowledgeEngine` usage in `packages/` (D6, D7, D8).
- A pgvector backend that passes the same conformance suite as SQLite, swappable by
  config, shaped as a `backends/pgvector` recipe (D5).
- A TS Module-1 ingest entry point that runs the `scan_*` family on a schedule, with the
  MCP narrowed to query + light mutation (D1, D2, D3).
- A TS Module-3 maintenance worker running `plan` → `run_maintenance_step` →
  `apply_decay`, scheduler owned by the deployment (D1, D4, D8).
- All three sharing one held `EngramProvider` + fused-per-project scope, engine-neutral
  (D6).
