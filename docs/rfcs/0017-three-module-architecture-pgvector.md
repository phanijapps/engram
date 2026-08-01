# RFC-0017: Three-module target architecture (Ingestion / Retrieval+Mutation / Maintenance) + pgvector backend

Status: Accepted
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

## Current state → target

| | Current | Target |
|---|---|---|
| Ingestion | `scan_*` live as MCP tools; no event/stream/scheduled framework | Module-1 service/CLI; scheduled first, then events + streaming |
| Retrieval + mutation | MCP ~32 tools (query + mutation + ingestion mixed) | MCP = query + light mutation only; `scan_*` moved out |
| Maintenance | `consolidate` + reflection + decay adapter exist, not a service | Module-3 scheduled service |
| Storage | SQLite single-file WAL (single writer) | SQLite default **+ pgvector** (concurrent writers) |

## Phased plan

Each phase → its own spec under `docs/specs/`, run through `work-loop`.

| Phase | Scope | Decisions | Depends on |
|---|---|---|---|
| **A — pgvector backend** | extract `backends/sqlite` recipe; add pgvector adapter cells (memory/knowledge/graph/vector/lexical/belief/hierarchy) + `backends/pgvector` recipe + conformance | D5, D6 | ADR-0022 (done) |
| **B — Ingestion module (scheduled)** | move `scan_*` into an `ingest` CLI/service (scheduled trigger); MCP keeps query + light mutation (D2) | D1, D2, D3 | — |
| **C — Maintenance service** | formalize consolidation/reflection/decay/hierarchy/taxonomy/GC behind a scheduler (Module 3) | D1, D4 | — |
| **D — Events + Streaming** | webhook/queue consumer ingestion triggers (Module 1) | D3 | B |

**Sequencing rationale.** pgvector (A) unblocks the concurrent-writer requirement the
three modules create, so it lands first. B is the cheapest structural win (move scan out
of the MCP). C builds on existing crates. D (streaming/events) is the furthest out and
the most operationally involved.

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

1. **One binary, three modes** vs three binaries? (Leaning: one binary, `--mode` flag —
   simplest to operate, deployable as one or three.)
2. Does Module 2's **API** transport (alongside MCP) carry the full mutation surface, or
   read-only? (Leaning: full mutation, since "add/update memories" is in scope.)
3. **Streaming** substrate — Kafka/Redpanda/NATS/Redis Streams? Defer the choice to
   Phase D with a probe.
4. pgvector **keyword lane** — `tsvector`, `pg_trgm`, or keep Tantivy behind a lexical
   adapter cell? (Leaning: Tantivy adapter cell reused, to avoid a second keyword impl.)

## What "done" looks like (for the accepted scope)

- A pgvector backend that passes the same conformance suite as SQLite, swappable by
  config (D5).
- A Module-1 ingestion entry point that runs the `scan_*` family on a schedule, with the
  MCP narrowed to query + light mutation (D1, D2, D3).
- A Module-3 maintenance entry point running consolidation/reflection/decay on a
  schedule (D1, D4).
- All three sharing one `EngramProvider` + fused-per-project scope, engine-neutral (D6).
