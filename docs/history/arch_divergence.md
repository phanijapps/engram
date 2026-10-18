# Architecture Divergence Tracker

This document tracks where the implementation diverges from the v2 target
architecture recorded under `docs/research/` — principally
`architecture-design-v2.md`, `memory-knowledge-architecture.md`, and
`synthesis.md`. It is a working engineering ledger, not a replacement for
ADRs, RFCs, or specs. Every row is grounded in a cited research target and a
cited implementation fact so the ledger is greppable and self-auditing.

**Calibration (read first).** Two things that *look* like divergences are
not, and must not be re-added as such:

- **Production storage substrate is not a divergence.** The research treats
  storage technology as illustrative and pluggable — it mandates separate
  *interfaces* per tier, not specific engines (`synthesis.md:240-243`,
  `synthesis.md:246-247`; `architecture-design-v2.md:794`), and leaves local
  storage choice as an open decision (`synthesis.md:296`). The SQLite
  adapters satisfy the interface requirement. A Postgres / Neo4j / pgvector
  adapter is a deployment item in `docs/backlog.md` (`deployment-adapters`),
  not an architecture divergence from research.
- **Belief / contradiction as a subsystem is beyond the research's explicit
  scope.** The research is silent on belief networks; it mandates only the
  cross-cutting *provenance + confidence* invariants (`synthesis.md:248-249`).
  The belief/contradiction implementation is a value-add. Its only
  research-relevant divergence is **port placement** (Area 2), not existence.

The cross-cutting invariants the research *does* mandate are honored, not
divergent: provenance everywhere (`synthesis.md:248-249`), policy as a
retrieval concern (`synthesis.md:250-251`), and scope isolation. Idempotency
(`docs/adr/0005-storage-adapter-semantics.md`) is enforced beyond what the
research specifies.

## Scale

- `100%`: the architecture boundary is enforced by code, docs, and tests.
- `75%`: the main contract exists and at least one adapter proves the path, but
  some callers still rely on compatibility shims or test-only composition.
- `50%`: the concept exists in contracts and partial implementation, but the
  boundary is not yet enforced.
- `<50%`: mostly research, draft model, or isolated prototype behavior.

## Selected Divergence Areas

| Area | Target (research) | Current (impl) | Divergence | Closing condition | Status |
|------|-------------------|----------------|------------|-------------------|--------|
| Memory/knowledge separation + per-tier storage interfaces | Memory and knowledge are separate concerns that compose through retrieval; storage is behind one interface per tier, engines illustrative (`synthesis.md:240-243`, `synthesis.md:246-247`). | Separate `engram-memory` and `engram-knowledge` port crates; durable SQLite adapters for memory (`engram-store-sql`), knowledge/graph/taxonomy/ontology (`engram-store-knowledge-sqlite`), vectors (`engram-store-vector`), and belief (`engram-store-belief-sqlite`); retrieval composition in `engram-retrieval`. The former broad process-local memory fixture is retired. | The main remaining divergence is compatibility re-export consumption through `engram-core`; the all-in-one fixture no longer hides storage-tier boundaries. | Downstream no longer depends on `engram-core` compatibility re-exports. | `98%` |
| Rust crate modularity — behavior ports split into focused crates | Small crates own one reason to change (`AGENTS.md`); consolidation is a separate pipeline, not an incidental write side-effect (`synthesis.md:224-226`). | RFC-0006 (accepted 2026-07-01) landed: `BeliefRepository`/`BeliefSynthesizer`/`ContradictionDetector` → `engram-belief`; `HierarchyRepository`/`HierarchyBuilder` → `engram-hierarchy`; `ConsolidationService` + executor/outcome + dry-run/gated + planner/evaluation_gate/validation → `engram-consolidation`; eval ports → `engram-eval`. `engram-core` is now a pure re-export facade (ADR-0010). Adapter package names are still pre-move (`engram-store-sql`, `engram-store-vector`). | Only the package-name rename and the deferred facade-fate decision (RFC-0006 D2) remain; the god-module is gone. | Rename adapter packages after compatibility planning; decide the facade fate against Area 1. | `95%` |
| Hierarchy construction/navigation separation | Distinct Construction Module and Navigation Module ("Organize then Retrieve") (`architecture-design-v2.md:159-164`); durable chunk hierarchy in the structured tier (`architecture-design-v2.md:535`). | Durable SQLite backend behind `HierarchyRepository` plus distinct construction/navigation ports in `engram-hierarchy`; the `path_for` traversal is shared via `engram_hierarchy::navigation` (extracted 2026-07-01). | Semantic clustering, model-assisted summaries, and a durable construction executor are deferred (research open decisions); construction and navigation share one crate (the trait-level split is sufficient). | Add durable construction algorithms, clustering, and model-assisted summaries when proactive hierarchy building is prioritized. | `75%` |
| Governed SKOS taxonomy evolution pipeline | Governed four-phase evolution (Discovery → Proposal → Validation → Merge) with a mandatory validation gate, provenance, and drift detection (`architecture-design-v2.md:332-379,800`; `synthesis.md:165-184`). | Durable `TaxonomyRepository` (SQLite), draft proposal/validation/drift domain types, `engram-knowledge` proposal validation, reviewer-gated merge checks, and SQLite merge fixtures exist. | Autonomous discovery workers and UI review surfaces are still deferred, but the governed lifecycle is now represented in code and tests. | Add richer discovery heuristics and demo review workflows when UI surfaces are prioritized. | `80%` |
| Consolidation as a formal, separately-owned pipeline | A separate pipeline with explicit inputs, outputs, conflict handling, and evaluation (`synthesis.md:224-226`). | `engram-consolidation` owns `ConsolidationService`, dry-run + gated mutating services, deterministic `ConsolidationPlan` operations for memory-to-fact, memory-to-belief, hierarchy, taxonomy, graph, compaction, and evaluation gates, plus an injectable apply/policy gate with auditable `ConsolidationRun` reports. | Consolidation is no longer anchored in `engram-core`; the trigger policy is decided (**ADR-0011** — explicit-command baseline; automatic triggers deferred). Concrete repository mutation executors remain adapter work, but the core pipeline/gates are real. | Implement adapter-specific mutation executors and introduce an auto-trigger scheduler only via a new ADR if needed. | `92%` |
| Retrieval mode completeness | Four retrieval modes — temporal, hierarchical, semantic, and cue-based — plus provenance and explainability (`architecture-design-v2.md:802`; `synthesis.md:190-198`). | Active durable paths include SQL keyword memory retrieval, sqlite-vec semantic candidates, SQLite knowledge graph candidates, shared fusion, and `FusionTrace` explainability (`engram-retrieval`). Temporal, cue, and hierarchy-expansion dispatch previously existed only in the retired process-local fixture. | Temporal, cue, and hierarchy expansion need durable adapter-backed implementations before they can count as active current behavior. | Re-land temporal, cue, and hierarchy retrieval through the storage-neutral retrieval boundary and focused durable adapters. | `55%` |
| Predictive retrieval (proactive loading) | Proactive, prediction-error-driven context loading via `predict_context(state) → RetrievalHints`, consumed by the query router (`architecture-design-v2.md:511-524`). | `AgentState` / `RetrievalHints` / `PredictiveRetriever` contract plus a deterministic `RecentActivityPredictor` baseline exist in `engram-retrieval` (2026-07-01). | Not yet wired into the `retrieve()` path; model-assisted prediction (expectation models, prediction-error signals) is deferred. | Wire `RetrievalHints` into the retrieval path; add a model-assisted predictor when proactive loading is prioritized. | `30%` |

## Current Alignment Snapshot

| v2 Architecture Item | Implementation State | Gap |
|----------------------|----------------------|-----|
| Memory and knowledge are separate but composable | Separate memory and knowledge port crates exist; graph/ontology/taxonomy storage is outside memory storage; shared retrieval composition lives in `engram-retrieval`. | `engram-core` compatibility re-exports still consumed. |
| Knowledge graph with ontology semantics | `KnowledgeGraphRepository` and `OntologyRepository` are durable via `engram-store-knowledge-sqlite` (`service.rs:392`; ADR-0008). | `validate_graph` is advisory only — enforced (write-rejecting) ontology validation is deferred (ADR-0008). |
| Storage layer with per-tier interfaces | Memory, knowledge, and vector SQLite adapters sit under their tier directories; the belief SQLite adapter sits under `adapters/orchestration/` (`engram-store-belief-sqlite`), reflecting the Area-2 port-placement issue rather than a clean per-tier adapter. | Only SQLite engines so far — sufficient, since substrates are illustrative; server/Postgres/Neo4j engines are a deployment item, not a divergence. |
| SKOS taxonomy evolution | Durable `TaxonomyRepository` and advisory drift detection exist. | The governed evolution pipeline is not implemented. |
| Hierarchical memory and HiRAG | Durable SQLite `HierarchyRepository` (`engram-store-hierarchy-sqlite`); `path_for` traversal shared via `engram_hierarchy::navigation`. | Durable hierarchy construction, retrieval expansion, semantic clustering, and model-assisted summaries deferred. |
| Belief network and consolidation cycle | Durable belief SQLite adapter (`engram-store-belief-sqlite`) and gated consolidation ship; ports now live in `engram-belief` and `engram-consolidation` (RFC-0006). | ADR-0011 decides the explicit-command consolidation trigger baseline; automatic triggers and additional task algorithms remain deferred. |
| Retrieval composition | `RetrievalFusion` and `ContextComposer` with `FusionTrace` ship (`engram-retrieval`); keyword memory, semantic vector, and graph candidates have durable paths. | Proactive `predict_context` loading is absent; temporal, cue, and hierarchy expansion need durable dispatch. |

## Immediate Closure Plan

Ordered by leverage. Each item is the seed of a future spec or RFC; open one
before adding behavior.

1. **✅ Split behavior ports out of `engram-core` (done — RFC-0006 / ADR-0010,
   2026-07-01).** `engram-belief`, `engram-hierarchy`, `engram-consolidation`
   created; eval ports moved into `engram-eval`; `engram-core` is now a pure
   facade. Remaining modularity work: rename adapter packages (item 7) and
   decide the facade fate (Area 1).
2. **✅ Add a durable hierarchy adapter + shared navigation** (done, 2026-07-01 — `engram-store-hierarchy-sqlite` + `engram_hierarchy::navigation` extracted so in-memory and SQLite share one `path_for` traversal). Remaining: clustering/model-assisted summaries.
3. **Implement the governed SKOS taxonomy evolution pipeline**
   (Discovery → Proposal → Validation → Merge) behind the taxonomy port, with a
   mandatory validation gate and provenance.
4. **✅ Decide the consolidation trigger policy** (done, 2026-07-01 — ADR-0011;
   explicit-command baseline, automatic triggers deferred). Remaining:
   additional task algorithms; an auto-trigger scheduler via a new ADR if needed.
5. **Re-land Cue, Temporal, and hierarchy expansion on durable paths.** The
   earlier process-local implementation was retired with the all-in-one fixture;
   the replacement should go through `engram-retrieval` and focused adapters.
6. **✅ Retire the broad process-local memory fixture** (done via the
   `retire-memory-inmem` slice; see `docs/product/engram.md`). Remaining: remove downstream dependence
   on `engram-core` compatibility re-exports.
7. **Rename adapter packages** to post-move names after compatibility
   planning (`engram-store-sql` → memory-sqlite; `engram-store-vector` →
   retrieval-sqlite-vec).

Deployment substrate work (Postgres / Neo4j / pgvector) is tracked in
`docs/backlog.md` (`deployment-adapters`); it is intentionally not a closure
item here — see the Calibration note above.
