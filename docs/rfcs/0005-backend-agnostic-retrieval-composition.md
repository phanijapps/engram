# RFC-0005: Backend-agnostic retrieval composition (graph + vector fusion)

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-07-01
- **Date closed:**
- **Decision weight:** heavy
- **Related:** ADR-0003 (implementation-stack — Accepted; the stack gate AGENTS.md makes mandatory is in place, so the two de-risk spikes are compliant), ADR-0005 (storage-adapter-semantics), ADR-0006 (first-sql-adapter-sqlite), ADR-0007 (napi-binding-surface-extension); spec [`benchmark-lazy-embeddings`](../specs/benchmark-lazy-embeddings/spec.md); RFC-0003 (durable demo), RFC-0004 (enterprise platform)

## Reviewer brief

- **Decision:** adopt the **retrieval-composition seam** (`RetrievalIndex` per source → `RetrievalFusion` → `ContextComposer`) as *the* integration contract, with the first durable `sqlite-vec` backend and graph retrieval moved behind the port.
- **Recommended outcome:** accept.
- **Change if accepted:**
  1. `ReciprocalRankFusion` in `core/retrieval` (already shipped as the de-risk spike).
  2. Durable `SqliteVectorIndex::open(path)` — embeddings persist across restarts (spike shipped). `content_hash`-keyed upsert + dead-vector GC are follow-on (see O2), not part of the spike.
  3. Graph retrieval becomes a `RetrievalIndex` so the graph store is swappable (SQLite → Neo4j) and its results RRF-fuse with vectors.
  4. A composition orchestrator + backend-selection config in `core/orchestration`.
  5. Demo Q&A rewired to true RRF-fused hybrid (KG + semantic), retiring the bespoke `buildEvidence` chunk tiering.
- **Affected surface:** `core/retrieval`, `core/orchestration`, `adapters/{knowledge/sqlite, retrieval/sqlite-vec}`, `bindings/node`, `demo/backend`, `docs/`.
- **Stakes:** costly-to-reverse (this seam is what every future backend conforms to) but not one-way — adapters are additive, and the read-path-only scope bounds blast radius.
- **Review focus:** (1) the **target-type/mechanism-agnostic principle** is not violated anywhere; (2) the write-path boundary (this RFC is read-path only) is held.
- **Not in scope:** Postgres / Neo4j / pgvector adapter implementations; distributed cross-store write consistency; learned reranker; entity-embedding semantic graph.

## The ask

**Recommendation (BLUF):** adopt the retrieval-composition seam as the contract every store plugs into, so scaling to Postgres / Neo4j / pgvector is *additive adapter work*, not rework. Deliver it now via RRF fusion, a durable sqlite-vec backend, graph-retrieval-behind-the-port, an orchestrator + backend config, and a demo rewired to true RRF-fused hybrid.

**Why now (SCQA):** The demo already proves two things — the KG-only path answers code questions at 87.5% (`docs/perf/PERFORMANCE.md`), and lazy query-time embeddings warm up correctly (`docs/perf/lazy-embeddings.md`). **But both bypass the storage-neutral ports**: Q&A grounding lives in a bespoke TypeScript `buildEvidence` that ranks entities by exact/prefix/substring and merges chunks in tiers — it is not a `RetrievalIndex`, it does not fuse with scores, and it is wired to one SQLite graph store. The complication: the stated scale target is Postgres (chunks ± graph ± pgvector) and Neo4j (graph), including "pgvector for graph." If the bespoke path stays, every backend swap reworks the demo, "hybrid" is never genuinely fused, and embeddings don't persist. The question: **what is the one seam that makes a backend swap an adapter addition instead of a refactor?**

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Adopt `RetrievalIndex → RetrievalFusion → ContextComposer` as the integration contract | Accept | The ports already exist and are storage-neutral; this makes them *the* seam | Acceptance | Confirm this is the contract |
| D2 | Default fusion = RRF, k=60 | Accept | Score-free ⇒ robust across incomparable backends (traversal vs cosine vs BM25); domain enum already names it. *(k=60 verification pending — see Evidence.)* | Acceptance | Confirm k=60 default |
| D3 | Ports are **target-type-oriented, mechanism-agnostic** (stated rule) | Accept | Guarantees "pgvector for graph" is an adapter choice, not an architecture change | Acceptance | Confirm the rule; flag any violation |
| D4 | Durable `sqlite-vec` (file-backed) as first vector backend | Accept | Persistence asked for; the spike proves file-backed reopen-survival. `content_hash`-keyed upsert (re-index dedup) + dead-vector GC are follow-on (O2), not claimed by the spike | Acceptance | Confirm separate-file; upsert/GC deferred (O2) |
| D5 | Orchestrator + per-plane backend config in `core/orchestration`; `core/retrieval` keeps fusion+ports | Accept | Matches the boundary rule (engram-core = orchestration facade) | Acceptance | Confirm home + config shape |
| D6 | Postgres/Neo4j/pgvector/external-embedder = follow-on adapter ADRs; this RFC = seam + SQLite delivery | Accept | One coherent RFC, not a monolith | Acceptance | Confirm scope boundary |

## Problem & goals

**Goals.**
- One storage-neutral seam such that graph / chunk / vector backends are independently swappable and co-locatable (one DB) or splittable (Neo4j + Postgres).
- True hybrid retrieval: KG results and semantic results RRF-fused, not appended.
- Persistent embeddings (survive restarts; reused on re-index via `content_hash`).
- Graph retrieval behind `RetrievalIndex` so Neo4j is an adapter, not a rewrite.
- Backend selection by config, not code.

**Non-goals.**
- Build Postgres / Neo4j / pgvector adapters now (follow-on ADRs).
- Solve distributed cross-store **write** consistency (sagas/outbox) — this RFC is read-path only.
- A learned/cross-encoder reranker (`RerankStrategy` variants stay available but unused).
- Move *all* graph ranking to Rust — relationships remain complementary evidence; only the chunk/entity retrieval that competes with semantic goes behind the port.
- Entity-embedding "semantic graph" (the port allows it; building it now is scope creep — deferred future work, see D6).

## Proposal

The pipeline every backend plugs into:

```
 GraphRetrievalIndex   VectorRetrievalIndex   (future: lexical, external vector, …)
   (SQLite → Neo4j)     (sqlite-vec → pgvector → Pinecone)
        \                    /
   each yields ranked, policy-checked RetrievalResult[]   ← backend-agnostic
                |
        RetrievalFusion (RRF, core/retrieval)             ← knows nothing of stores
                |
        ContextComposer (budget, explanations)
                |
          ContextPayload  →  Q&A grounding / agentic tools
```

**Layer ownership.**
- `core/retrieval`: the ports (`RetrievalIndex`, `RetrievalFusion`, `ContextComposer`) + fusion algorithms (`WeightedRetrievalFusion`, `ReciprocalRankFusion`) + `compose_context`. No store/provider/policy imports (unchanged).
- `core/orchestration` (engram-core): the orchestrator that runs the wired indexes → fuses → composes, plus the **backend-selection config** (per plane: graph/chunk/vector → adapter + dsn). The facade owns wiring; the algorithm stays in `core/retrieval`.
- Adapters: each backend implements `RetrievalIndex`. sqlite-vec gains `open(path)` (durable). A new graph `RetrievalIndex` in the knowledge adapter yields ranked entity/chunk `RetrievalResult`s. pgvector/Neo4j/Postgres are future adapters behind the same port.
- Binding + demo: the demo wires indexes per config, calls the orchestrator, and feeds the RRF-fused context to Q&A.

**The principle (D3), stated for posterity:** a `RetrievalIndex` is defined by **what it returns** (`RetrievalTargetType`: Entity/Relationship/Chunk/…), never by **how** it retrieves (traversal vs vector vs lexical). Therefore "pgvector for graph" is an adapter selection: a graph index may be backed by Neo4j traversal *or* pgvector semantics *or* both (two `Entity`-target indexes, RRF-fused). The orchestrator and ports must never assume a mechanism.

## Options considered

*Axis: where the seam lives + how much moves behind the port.*

- **A — Seam in Rust, RRF in `core/retrieval`, KG-behind-port, orchestrator+config in `core/orchestration`** *(recommended).* Maximally aligned with the boundary rules; fusion is reusable/testable; backends are additive. Cost: a real (if bounded) refactor of the demo's Q&A + a new graph index + orchestrator.
- **B — RRF in the TypeScript demo only.** Lighter; but couples fusion to the demo, is not reusable, does not make the graph store swappable, and leaves "hybrid" demo-local. Rejected: doesn't satisfy the scale goal.
- **C — Do-nothing (keep bespoke `buildEvidence`).** Rejected explicitly: scaling to Postgres/Neo4j then requires reworking the bespoke path per backend, which is the exact cost this RFC exists to avoid.

Prior art grounding: the domain already names `FusionStrategy::ReciprocalRankFusion` (unimplemented — this RFC implements it) and ships `WeightedRetrievalFusion` as the structural template; `compose_context` already does fuse+budget on pre-collected candidates.

## Risks & what would make this wrong

- **Pre-mortem (assume it shipped and failed):**
  1. The orchestrator/config becomes a god-module that knows about every backend (violates the boundary rules).
  2. Moving KG retrieval behind the port *regresses* demo Q&A quality vs the bespoke exact/prefix/substring ranking.
  3. RRF's k=60 underweights a genuinely strong single-source signal on this data.
  4. Durable vec0 hits concurrency trouble under concurrent read/write (the demo already uses WAL for the knowledge DB; the vector file needs the same discipline).
  5. Scope/policy is not enforced uniformly across backends → a backend leaks out-of-scope results.
- **Key assumptions (falsifiable):**
  - RRF over [graph, vector] ≥ bespoke tiered merge on the 8Q/50Q evals (tested in Experiment).
  - File-backed vec0 is concurrent-safe under demo load with WAL + `busy_timeout`.
  - A SQLite-backed graph `RetrievalIndex` can rank entities/chunks well enough to preserve the 87.5% baseline.
- **Drawbacks:** more moving parts; demo rewire carries short-term regression risk; RRF discards raw scores (could underweight a strong signal — mitigated by per-source weights as a later knob, not by abandoning RRF).

## Evidence & prior art

- **Spike / de-risk result (both green, shipped with this RFC):**
  - *RRF:* `ReciprocalRankFusion` in `core/retrieval/src/reciprocal.rs` + 7 integration tests (`consensus_outranks_single_source`, `per_source_rank_not_global_index`, `ignores_raw_scores_across_sources`, `stamps_reciprocal_strategy_trace`, `respects_limit`, `empty_returns_empty`, `default_k_is_60`) — all pass. Confirms per-source ranking, consensus boosting, and score-agnosticism.
  - *Durable sqlite-vec:* `SqliteVectorIndex::open(path, dimensions)` + `index::tests::file_backed_index_survives_reopen` — pass. A vector written, handle dropped, file reopened, searched ⇒ the vector persists.
- **Repo precedent:** `FusionStrategy::ReciprocalRankFusion` (`core/domain/src/retrieval.rs:155`, unimplemented); `WeightedRetrievalFusion` (`core/retrieval/src/weighted.rs`, structural template); `RetrievalFusion`/`RetrievalIndex` ports (`core/retrieval/src/ports.rs`); `compose_context` (`core/retrieval/src/composer.rs`); `content_hash` field on `VectorEntry`; ADR-0005/0006/0007; the existing `dbPath` option pattern for native engines.
- **External prior art:** RRF (Cormack, Clarke, Buettcher, *SIGIR 2009*); k=60 is the conventional constant. *(Stated from established IR practice; the URL was not fetched this session — to be verified before status → Open.)* pgvector and Neo4j are the canonical target backends for the two planes.
- **Reverses:** the just-shipped `benchmark-lazy-embeddings` spec's "in-memory cold-start" decision (→ durable) and "tiered evidence-merge" fusion (→ RRF). Both were listed under that spec's *Ask first*, so this is the contemplated follow-on, not a silent reversal.

## Experiment / validation

- **Hypothesis:** RRF-fused hybrid (KG + semantic) matches or beats the KG-only baseline on the eval suites, and the warm-up curve holds with a persistent cache (cold only on the first-ever run or after a model/schema change).
- **What we measure:** 8Q/50Q accuracy (correct+partial) for KG-only vs RRF-hybrid; per-query latency; cache hit-rate across passes; and a one-time vs repeated-run comparison to confirm persistence.
- **Success / failure:** hybrid ≥ baseline accuracy; warm-up hit-rate climbs across passes; a second run (persistent cache warm) shows pass-1 hit-rate ≈ prior run's final hit-rate. Failure mode: hybrid < baseline by >1 question ⇒ investigate whether the graph index ranking or RRF k needs tuning before adoption.

## Open questions

- **O1 — Backend-config format.** Env-only (demo) vs a declarative TOML manifest (production)? *Recommend env-overridable now, manifest shape reserved* — don't over-build before a second backend exists. Owner: Approver. Decide-by: when the Postgres adapter lands.
- **O2 — `content_hash` upsert + dead-vector GC strategy.** When does re-index dedup happen (`ON CONFLICT (id) ... WHERE content_hash` upsert), and when are vectors whose chunk was deleted reclaimed? *Recommend upsert-on-write (content_hash-keyed) + lazy GC (skip search hits that map to no live chunk) initially; a real sweep later.* Not claimed by the spike. Owner: Approver. Decide-by: the durable-engine implementation slice.

*Decided, not open:* orchestrator home = `core/orchestration` (D5). *Future work, not open:* entity-embedding "semantic graph" is enabled by D3 but explicitly deferred (D6 / Non-goals).

## Follow-on artifacts

- **ADR-0009 — Retrieval-composition seam** (the architecture decision this RFC asks to adopt).
- **ADR-0010 — Durable vector store** (sqlite-vec file-backed now, pgvector later) — or folded into ADR-0009.
- **Spec `backend-agnostic-retrieval`** — the multi-slice implementation plan (graph index, orchestrator, config, durable engine wiring, demo rewire, benchmark re-run).
- **Future adapter ADRs** (non-blocking): postgres-chunks, pgvector, neo4j-graph, external-embedder.

## Errata

_None yet._
