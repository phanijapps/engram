# RFC 0012: Code-Structural Graph Layer on top of Engram

## Status

Draft — proposal/plan, pending acceptance. Research basis:
`docs/research/memtrace-survey.md`. Extends `docs/rfcs/0008-cross-repo-linkage.md`.
No implementation code is written under this RFC until it is accepted and the
first spec is approved (per the repo's artifact sequence: RFC → spec → plan →
work-loop).

## Problem

We want memtrace-equivalent capability — a bi-temporal structural knowledge
graph of source code, served to coding agents over MCP, with hybrid retrieval,
blast-radius analysis, cross-service API topology, a dashboard, and agent skills
— **built on top of engram as the substrate, not baked into engram's core.**

## Key finding (this is why the plan is small, not large)

The accepted engram domain model (`docs/domain-data-model.md`) and the existing
ingest adapter **already cover the bulk of the structural substrate**:

| Memtrace concept | Already in engram? | Where |
|---|---|---|
| Symbol nodes (function/method/class/…) | ✅ accepted | `KnowledgeEntity.EntityKind` |
| CALLS / IMPORTS / EXPORTS / IMPLEMENTS / CONTAINS edges | ✅ accepted (predicates listed) | `KnowledgeRelationship` |
| Code symbol as a retrievable unit | ✅ accepted | `KnowledgeChunkKind::code_symbol` |
| Code file / git source | ✅ accepted | `SourceDocumentKind::code`, `SourceKind::git_repository` |
| Lexical + graph retrieval legs | ✅ in contract (modes `keyword`, `graph`) | `RetrievalMode` |
| Reciprocal Rank Fusion | ✅ accepted + **implemented** | `FusionStrategy`, `core/retrieval/reciprocal.rs` |
| Cross-encoder rerank | ✅ in contract (`RerankStrategy::cross_encoder`) | not yet implemented |
| Community detection shape | ✅ drafted | `HierarchyNodeKind::cluster` + `HierarchyBuildConfig` (`algorithm`, `targetClusterSize`, `interClusterThreshold`) |
| Bi-temporal timestamps | ✅ standard semantics | `validFrom` / `validUntil` |
| Scope-bound workspace for cross-repo | ✅ accepted | `Scope.workspace` + RFC 0008 |
| Policy + provenance on every record | ✅ invariant | (memtrace lacks this — our edge) |

**Spike result (verified):** `adapters/ingest/src/extractor.rs` already emits
`KnowledgeEntity` + `KnowledgeRelationship` (with `graph_id` and CALLS edges);
`scanner.rs` already persists them via `put_entity` / `put_relationship`;
`code_symbol.rs` already produces `KnowledgeChunkKind::CodeSymbol`. **The symbol
→ entity/relationship graph already exists.** The gap is therefore *filling
ports and adding intelligence layers*, not redesigning the core.

## Decision: a new layer, not core changes

Per `AGENTS.md` ("Do not create god classes/modules/packages"; crate roots are
facades) and the user's directive ("build on top, not bake in"), the code-graph
intelligence lives in a **new top-of-tree layer** that depends on engram's
stable crates and does **not** modify their behavior:

```text
                ┌──────────────────────────────────────────────┐
   AI agents ──▶│  NEW LAYER: code-structural graph             │
   (MCP / UI)   │  codegraph-algos · temporal · topology        │
                │  mcp-server · dashboard · skills · fleet      │
                └───────────────┬──────────────────────────────┘
                                │ depends on (read + compose)
                                ▼
   ENGRAM (substrate, unchanged behavior):
     engram-domain   engram-knowledge   engram-retrieval
     engram-hierarchy   engram-memory   engram-eval
     engram-ingest (tree-sitter)   engram-node (N-API)
```

- **Only compatible additions** touch engram-domain: new additive `EntityKind`
  values (`struct`, `interface`, `trait`, `type_alias`, `enum`, `endpoint`, …)
  and a few additive edge predicates — all permitted under the contract-freeze
  policy ("Add new enum values only when consumers tolerate unknown values").
- Everything else (algorithms, temporal engine, topology, MCP, UI, skills,
  fleet) is **new crates / packages** in a new workspace area, composing engram
  ports.
- Repo strategy (open question Q1): recommended = **new area in this monorepo**
  (e.g. `codegraph/` crates + `packages/codegraph`), depending on engram crates
  by path. Alternative = separate repo consuming published engram crates.

## Contract surfaces involved

- **Compatible additions to `engram-domain`** (ADR-gated, additive only):
  - `EntityKind`: add `struct`, `interface`, `trait`, `type_alias`, `enum`,
    `endpoint`, `ci_job`, `terraform_resource`, `sql_policy`, …
  - `KnowledgeRelationship` predicates: `calls`/`imports`/`exports`/`implements`/
    `contains` already exist; add `overrides`, `annotated_with` if needed.
  - Optional `validFrom` / `validUntil` on `KnowledgeEntity` (compatible; today
    they live on `MemoryAssertion` / `Belief`). **Contract risk R1.**
- **New layer contracts** (versioned under `contracts/`, generated):
  - `Episode` (git_commit / working_tree), symbol version timeline, the 6
    evolution scoring modes, structural-significance budgeting.
  - Graph-metric results (centrality, betweenness, community), blast-radius,
    dependency-path, complexity, API-topology graph.
  - MCP tool catalogue (~38 tools, mirroring memtrace's surface, engram-typed).
- **Reused, unchanged**: `KnowledgeGraphRepository`, `KnowledgeRepository`,
  `RetrievalRequest`/`RetrievalResult`, `FusionTrace`, `HierarchyBuildConfig`,
  `Scope`, `Policy`, `Provenance`, `EvaluationFixture`.

## Milestones (dependency-ordered)

Each milestone lists: crates/packages affected · contract surface · first
testable slice · deferrals · verification. Evaluation is part of every
milestone, not a late stage (per the build skill and `engram-eval`).

### M0 — Spike ratification, contract additions, build-location ADR  *(GATE)*
- **Affected:** `engram-domain` (additive enums), new ADR 0019 (build location),
  `engram-ingest` (read-only confirmation).
- **Contract:** additive `EntityKind`/predicates; ADR ratifies "new layer, not
  core."
- **First slice:** ingest a tiny repo with the in-memory knowledge adapter;
  assert `KnowledgeEntity(function)` + `KnowledgeRelationship(calls)` exist and
  resolve caller→callee within a file. Extend the assertion to cross-file
  resolution and **document the resolution frontier** (this becomes the M2/M4
  work item if it doesn't already resolve cross-file).
- **Defer:** multi-language breadth (start Rust+TS+Python), the full 41-kind
  taxonomy.
- **Verify:** `cargo test -p engram-ingest`; `.codex/hooks/check-contracts.sh`.

### M1 — Retrieval parity: lexical leg + cross-encoder rerank + code embeddings
- **Affected:** new lexical adapter (Tantivy/BM25 behind `RetrievalMode::keyword`),
  `core/retrieval` composer (3-leg RRF: lexical+vector+graph), new cross-encoder
  reranker adapter (`RerankStrategy::cross_encoder`), `engram-provider-embed`
  (add a code-specialised model option, e.g. jina-code/bge-code).
- **Contract:** no breaks — `keyword`/`graph` modes and `cross_encoder` rerank
  are already enumerated; this *implements* them.
- **First slice:** `find_code(query)` over an indexed fixture returns
  hybrid-ranked symbols; an `EvaluationFixture` asserts expected recall +
  forbidden recall (forbidden = symbols from an unrelated module).
- **Defer:** learned ranker, LLM-judge rerank.
- **Verify:** `cargo test -p engram-retrieval`; `pnpm run contracts:generate`;
  eval harness with deterministic fixtures.

### M2 — Code-graph intelligence: algorithms + quality metrics
- **Affected:** new `codegraph-algos` crate (PageRank, betweenness, Louvain→
  `HierarchyNodeKind::cluster` build); quality ops (cyclomatic complexity,
  dead-code = zero in-degree on `calls`, complexity hotspots); blast-radius +
  dependency-path traversal over `KnowledgeRelationship`.
- **Contract:** new result types (centrality/community/impact) under
  `contracts/`; reuses `HierarchyBuildConfig`.
- **First slice:** index a repo → `find_central_symbols`, `list_communities`,
  `get_impact(depth)` return results matching a golden graph fixture.
- **Defer:** process/flow auto-detection from entry points (later sub-milestone).
- **Verify:** `cargo test` in new crate; `engram-eval` deterministic graph-metric
  fixtures.

### M3 — Bi-temporal symbol versioning + evolution engine
- **Affected:** new `codegraph-temporal` crate; episode model
  (`git_commit`/`working_tree`); symbol `valid_from`/`valid_to`; the 6 scoring
  modes (compound/impact/novel/recent/directional/overview) + structural-
  significance budgeting. Reuses `Provenance` + `MemoryEvent` lifecycle.
- **Contract:** R1 — optional `validFrom`/`validUntil` on `KnowledgeEntity`
  (ADR-gated). **Must not** claim valid-time `as_of` as full bitemporality
  (domain invariant already forbids the overclaim).
- **First slice:** index at commit A, mutate + re-index → `get_timeline` and
  `get_evolution(mode=recent)` return the correct delta with episode provenance.
- **Defer:** cross-commit replay over arbitrary windows (build on the slice).
- **Verify:** git-fixture temporal eval; conformance that disappeared symbols
  are `valid_to`-stamped, not deleted.

### M4 — Cross-repo workspace fusion + HTTP API topology
- **Affected:** extends RFC 0008; `engram-ingest` workspace marker + cross-repo
  edge resolution; new `codegraph-topology` framework scanners (endpoint +
  call-site detection for the priority frameworks).
- **Contract:** cross-repo edge predicates already exist; workspace resolution
  is configuration, not contract.
- **First slice:** index a frontend + backend under one workspace →
  `get_api_topology` returns cross-repo HTTP edges (e.g. `fetch("/api/users")` →
  `Router::route`).
- **Defer:** the long tail of framework scanners (Terraform/SQL-RLS/Helm → later).
- **Verify:** cross-repo recall eval fixtures; policy/scope isolation tests
  (records from unrelated workspaces never blend).

### M5 — MCP server (the agent surface)
- **Affected:** new `codegraph-mcp` crate (Rust, JSON-RPC over stdio +
  streamable-HTTP) exposing ~38 tools over the codegraph layer + engram
  retrieval; owner/attach model (one owner per workspace; agents attach).
- **Contract:** MCP tool catalogue under `contracts/`; engram-typed args.
- **First slice:** a test MCP client drives `find_symbol`, `find_code`,
  `get_impact`, `get_symbol_context` end-to-end against an indexed repo.
- **Defer:** `execute_cypher`/direct-graph passthrough (later); fleet tools.
- **Verify:** MCP integration tests; `CapabilityReport` advertises implemented
  tools and marks the rest `deferred`.

### M6 — Dashboard UI
- **Affected:** new/extended package (build on `demo/engram-ui`): graph canvas,
  node CODE/INFO/HISTORY panels, timeline, insights ("curated read"), cortex.
- **Contract:** none (presentation over MCP/HTTP).
- **First slice:** dashboard renders the graph + a node detail panel for an
  indexed repo; switching the workspace lens re-scopes the view.
- **Defer:** fleet/cortex panes (ride on M7).
- **Verify:** `pnpm run build`; smoke test; typecheck.

### M7 — Skills + fleet coordination
- **Affected:** author ~40 engram-flavoured `SKILL.md` (single-domain, workflow,
  decision-memory, docs); new `codegraph-fleet` crate (publish-intent /
  record-episode / resolve / intent-verification / preflight).
- **Contract:** fleet/intent records are new layer contracts.
- **First slice:** the `memtrace-first`-equivalent skill chains
  `find_symbol` → `get_symbol_context` correctly; fleet publish-intent +
  resolve prevents two agents clobbering the same symbol.
- **Defer:** fleet is the **weakest-grounded** capability (memtrace's protocol
  is not public beyond the skill descriptions) — ship a minimal,
  well-instrumented version and iterate.
- **Verify:** skill evals; fleet conformance tests.

## Dependency graph

```
M0 (gate) ──▶ M1 (retrieval) ──▶ M2 (algos) ──▶ M5 (MCP) ──▶ M6 (UI)
                  │                   │             │
                  └──▶ M3 (temporal) ──┘             └──▶ M7 (skills+fleet)
                  └──▶ M4 (cross-repo/topology) ──────────▶ M7
```

M1 unblocks M2/M3/M4 (they all query). M5 (MCP) needs M1+M2 minimum. M6 needs
M5. M7 needs M5 and benefits from M3/M4. M0 is a hard gate.

## Contract risks (call out before implementation risks)

- **R1 — `validFrom`/`validUntil` on `KnowledgeEntity`.** Compatible (optional
  fields) but must go through an ADR; must not be claimed as full bitemporality.
- **R2 — Additive enum values** must be flagged as "consumers must tolerate
  unknown values"; older clients reading a `struct` entity must degrade, not
  crash.
- **R3 — Build-location discipline.** The new layer must stay split
  (algos / temporal / topology / mcp / ui / fleet as separate crates), never a
  single god crate. Engram core behavior stays unchanged.
- **R4 — Policy/scope on every write path.** Symbol/version/edge writes inherit
  engram's policy gate; no bypass for "ingest performance."

## Explicit deferrals

- Proprietary-grade indexer perf targets (memtrace's 50k files/<90s). We
  establish **our own** benchmarks via `engram-eval`, not trust vendor numbers.
- ArcadeDB / server / multi-tenant mode. We target engram's **SQLite adapters**
  as the store (sidesteps memtrace's closed MemDB/ArcadeDB split entirely).
- Long-tail framework scanners (Terraform, SQL RLS, Helm, K8s, the 20+ HTTP
  frameworks) — priority frameworks first.
- `learned_ranker` / `llm_judge` rerank; direct `execute_cypher` passthrough.

## Verification commands (per `AGENTS.md`)

```bash
cargo fmt --all
cargo check --workspace
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
pnpm run build        # after TS package surface changes
```

Plus per-milestone: `engram-eval` deterministic fixtures (recall, forbidden
recall, ranking, temporal delta, cross-repo recall, graph-metric golden
values).

## Open questions

- **Q1 — Repo strategy:** new area in this monorepo (recommended) vs. separate
  repo consuming published engram crates?
- **Q2 — MCP host language:** Rust MCP crate (parity + perf, recommended) vs.
  TS package over `@engram/node`?
- **Q3 — Embedding provider for code:** jina-code vs. bge-code vs. keep
  bge-small and rely on the new lexical leg?
- **Q4 — v1 taxonomy breadth:** how many of the 41 node kinds / languages in
  the first cut (propose: Rust, TS, Python + their priority HTTP frameworks)?
- **Q5 — Fleet protocol shape:** the weakest-grounded area; do we ship a minimal
  intent/episode/resolve model in v1 or defer fleet entirely past M6?
