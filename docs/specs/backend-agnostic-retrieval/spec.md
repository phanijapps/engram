# Spec: backend-agnostic-retrieval (RRF-fused hybrid over the composition seam)

- **Status:** Shipped
- **Shape:** mixed (service + integration)
- **Constrained by:** [RFC-0005](../../rfcs/0005-backend-agnostic-retrieval-composition.md) / [ADR-0009](../../adr/0009-retrieval-composition-seam.md); AGENTS.md boundary rules; `engram-eval`
- **Contract:** none new (reuses the existing `RetrievalIndex` / `RetrievalFusion` / `ContextComposer` ports in `core/retrieval`)

## Objective

Route Q&A retrieval through the storage-neutral composition seam so that
**knowledge-graph results and semantic (vector) results are RRF-fused**, not
appended. Implementation is **SQLite-only** and delivers: a configurable RRF
(k + per-source weights, defaults when config absent), a graph `RetrievalIndex`
behind the port, a composition orchestrator + backend-selection config, and a
durable sqlite-vec backend — so that reaching the two documented deployments
(**pgvector(graph+vector)**; **pgvector(vector)+neo4j(graph)**) is later an
additive adapter change, not a refactor.

User-visible outcomes (each testable):
- Q&A answers are grounded in RRF-fused KG + semantic evidence; the bespoke
  `buildEvidence` chunk-tiering is retired.
- RRF strength is tunable without code changes; absent config still works.
- Embeddings persist across restarts and are reused on re-index.
- The 8Q/50Q benchmark shows hybrid accuracy ≥ the KG-only baseline and the
  warm-up curve holds across runs.

## Decision

Per ADR-0009: graph/chunk/vector backends are `RetrievalIndex` impls producing
ranked `RetrievalResult`s; `ReciprocalRankFusion` fuses them; `ContextComposer`
budgets; `core/orchestration` wires indexes per a backend-selection config and
drives the pipeline. Ports stay target-type-oriented and mechanism-agnostic
(RFC-0005 D3). Read path only.

## Boundaries

### Always do
- Route Q&A retrieval through `RetrievalIndex → RetrievalFusion → ContextComposer`.
- Keep `core/retrieval` free of store/provider/policy imports; fusion + config live there.
- Provide config defaults so absent config works out-of-the-box (RRF k=60, equal weights).
- Make the durable engine + orchestrator fail closed to the KG-only path on any backend error.

### Ask first
- Adding any non-SQLite adapter (the two Postgres/Neo4j deployments are documented targets, not built here).
- Changing the default fusion away from RRF.
- A persistent on-disk dead-vector GC sweep (lazy skip-of-dead is the default).

### Never do
- Put vector/graph/embedding code in `core/domain` or make a `RetrievalIndex` couple to a retrieval mechanism (D3 violation).
- Embed the whole corpus eagerly at index time.
- Cross the read-path boundary into distributed cross-store write consistency.
- Make Q&A throw on a retrieval/backend failure.

## Testing Strategy

- **TDD (Rust, hermetic):** extend RRF for the configurable strength (k + per-source weights, defaults); graph `RetrievalIndex` ranking → `RetrievalResult`s; orchestrator fusion over two stub indexes; durable reopen-survival (already green).
- **Integration (Rust):** orchestrator wires [graph index, vector index] → RRF → `compose_context` → a `ContextPayload` whose top items reflect cross-source consensus.
- **Goal-based (demo + benchmark):** `POST /bench` (KG-only) vs `POST /bench/lazy` (RRF-hybrid) on the indexed terminal repo — hybrid correct+partial ≥ baseline; warm-up hit-rate climbs across passes; a second run reuses persisted embeddings (pass-1 hit-rate ≈ prior final).

## Acceptance Criteria

- [x] Configurable RRF: `ReciprocalFusionConfig` (k + per-source weights) with `Default`; tests for weights + defaults. (T1)
- [x] Graph `RetrievalIndex` in the knowledge adapter yields ranked entity/chunk `RetrievalResult`s (SQLite-backed). (T2)
- [x] Composition seam exposed via the binding (`graphCandidates` + `fuseRrf`/`fuseRrfIds`); the TS demo orchestrates. A full Rust orchestrator in `core/orchestration` is deferred until a 2nd backend needs it (option b′, RFC-0005). (T3)
- [x] Durable sqlite-vec wired into the engine (path option, `${ENGRAM_DB}.embeddings.db`). `content_hash`-keyed upsert + lazy GC are follow-on (RFC-0005 O2). (T4)
- [x] Demo Q&A retrieves through the seam (RRF-fused graph+vector hybrid); bespoke `buildEvidence` chunk-tiering replaced by the fused order when the seam is on (KG-only fallback retained). (T5)
- [x] Benchmark re-run: RRF-hybrid 22/24 (91.7%) ≥ KG-only 6/8 (75%); warm-up hit-rate 18%→73% + latency falls; results in `docs/perf/lazy-embeddings.md`. (T6)

## Assumptions

- Design + decisions are settled by the accepted RFC-0005 (D1–D7); this spec implements them, SQLite-only.
- The implementation stack gate (ADR-0003) is satisfied; the FastEmbed model is cached for the demo; the terminal repo is indexed and `.env` creds are present.
- The two Postgres/Neo4j deployments are **documented only** (RFC-0005 D6) — no non-SQLite adapter is built in this spec.
- Cross-store write consistency is out of scope (read-path only) — `content_hash` upsert handles single-store cache staleness, not distributed writes.
