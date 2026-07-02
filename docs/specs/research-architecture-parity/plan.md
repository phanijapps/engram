# Plan: Research Architecture Parity

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Close the gap between the research architecture and implementation as a sequence
of contract-backed vertical slices, not a single sweeping rewrite. The first
slices make the docs and contracts honest, then add focused Rust behavior behind
existing boundaries: retrieval routing, hierarchy construction, governed
taxonomy evolution, consolidation, belief lifecycle completeness, evaluation,
and TypeScript/Node projections. AgentZero cutover remains out of scope, but
each slice preserves adapter-readiness through compatibility probes and mapping
fixtures.

Expected touch areas: `docs/research/`, `docs/arch_divergence.md`,
`docs/architecture/`, `docs/domain-data-model.md`, `contracts/v1/` only for
compatible additions or examples, `core/{domain,memory,knowledge,retrieval,belief,hierarchy,consolidation,eval}`,
`adapters/{memory/sqlite,knowledge/sqlite,hierarchy/sqlite,orchestration/belief-sqlite,retrieval/sqlite-vec}`,
`bindings/node`, `packages/*`, `demo/*`, `.codex/hooks/`, and `docs/specs/`.

Tempted to implement a single `MemoryEngine` facade; declining because it would
erase the research's memory/knowledge/retrieval/consolidation boundaries.
Tempted to add AgentZero route-compatible APIs in Engram; declining because
AgentZero compatibility belongs in the AgentZero adapter. Tempted to introduce
production substrates while closing architecture gaps; declining because the
research requires separate interfaces, not specific database brands.

## Constraints

- `docs/domain-data-model.md` remains the source of truth until an ADR changes
  the generation source.
- `docs/architecture/reference.md` is normative for crate/package shape,
  dependency direction, and clean-design rules.
- ADR-0009 keeps retrieval composition store-free and read-path focused.
- ADR-0010 keeps behavior ports split out of `engram-core`.
- ADR-0011 keeps automatic consolidation scheduling deferred behind an explicit
  future decision.
- Retired in-memory adapters must not return as active conformance surfaces.
- AgentZero adapter integration stays governed by
  `docs/specs/agentzero-engram-adapter-integration` and
  `docs/specs/zbot-engram-belief-bitemporal-cutover`.

## Loop Progress

- 2026-07-02: T0/T1 complete. The pre-implementation gate passes, stale
  belief/in-memory research claims are refreshed, PHASE64 tracks the remaining
  parity work, and `check-docs.sh` now runs the registry-backed research parity
  drift check with focused regression tests.
- 2026-07-02: T2 complete as a compatible draft-extension contract update.
  `docs/domain-data-model.md` now names memory roles, governed taxonomy
  proposals, hierarchy build records, capability reports, adapter-readiness
  probes, migration diagnostics, and vector retrieval's v1 compatibility
  classification without changing accepted v1 schemas.
- 2026-07-02: T3 complete. `engram-domain` exposes derived `MemoryRole`
  classification without adding a v1 `MemoryRecord.role` field, and the SQLite
  memory service now has role fixtures covering write, retrieve, archive forget,
  and eval-report recall for working, episodic, semantic, and procedural
  records.
- 2026-07-02: T4 complete at the storage-neutral retrieval layer.
  `engram-retrieval` now has a mode-aware `RetrievalRouter` for temporal, cue,
  hierarchical, semantic, graph, keyword, and vector-as-semantic routes, with
  unsupported-mode/source-error reporting and composition tests for budget
  omissions and degraded source failures.
- 2026-07-02: T5 complete. `engram-domain` now has typed
  `HierarchyBuildRecord` / `HierarchyBuildStatus`, `engram-hierarchy` validates
  parentage before build outputs are accepted, and the SQLite hierarchy fixture
  persists explainable raw-event, episode, schema, and domain layers with build
  metadata and bounded path navigation.
- 2026-07-02: T6 complete. `engram-domain` now has taxonomy proposal,
  validation report/finding, and semantic-drift types; `engram-knowledge`
  validates proposals for duplicate labels, missing relation endpoints,
  broader/narrower cycles, and reviewer-gated merge; SQLite taxonomy tests prove
  validated proposals merge into the durable store without autonomous merge.

## Construction Tests

**Integration tests:**

- End-to-end local conformance fixture: write memory, ingest knowledge, build
  hierarchy, propose taxonomy changes, retrieve fused context, synthesize belief
  candidates, detect contradictions, run consolidation dry-run, apply gated
  mutations, evaluate recall/leakage/ranking, and forget scoped records.
- Adapter-readiness fixture: exercises Engram library ports with AgentZero-like
  scopes, valid-time intervals, source references, capability reports, and
  migration diagnostics without importing AgentZero product code.

**Manual verification:**

- Demo backend/frontend smoke for indexing, graph exploration, hybrid retrieval,
  belief review, hierarchy navigation, taxonomy review, consolidation dry-run,
  and evaluation report display.

## Design (LLD)

### Design Decisions

- Treat research parity as a set of durable contracts and vertical behavior
  slices. A slice counts only when docs, ports, adapter behavior, eval fixtures,
  and bindings all agree.
- Keep construction and navigation separate. Hierarchy and taxonomy builders
  produce explainable artifacts; retrieval navigates those artifacts.
- Keep taxonomy evolution governed. Discovery and proposal can be automatic, but
  merge requires validation and an explicit actor/action.
- Keep consolidation operation-owned, not scheduler-owned. Engram provides
  plans and gated steps; host applications decide when to invoke them.
- Keep full record-time bitemporality out until storage and public query
  contracts can answer it honestly.

### Data & Schema

- Add or clarify draft extension models for memory subsystem roles, cue queries,
  hierarchy construction results, taxonomy proposals, drift reports,
  consolidation plans/runs, evaluation assertions, and compatibility probes.
- Compatible v1 schema additions must be optional. Breaking renames or required
  fields require a new contract version.
- SQL schema changes stay adapter-local and must preserve scope, provenance,
  policy, and idempotency columns needed by conformance fixtures.

### Interfaces & Contracts

- `engram-memory`: owns memory records, events, lifecycle, write/retrieve/forget
  service ports, and role-specific memory contracts.
- `engram-knowledge`: owns sources, chunks, graph, ontology, taxonomy, source
  reading, chunking, ingestion, taxonomy evolution ports, and graph retrieval
  candidate sources.
- `engram-retrieval`: owns query routing, retrieval indexes, fusion, ranking
  traces, context composition, and predictive hints while remaining store-free.
- `engram-hierarchy`: owns hierarchy construction and navigation ports plus
  shared traversal.
- `engram-belief`: owns valid-time belief lifecycle, contradiction idempotency,
  and embedding-scoring helpers.
- `engram-consolidation`: owns dry-run plans, gated apply, evaluation gates, and
  operation reports.
- `engram-eval`: owns deterministic recall, leakage, ranking, hierarchy,
  taxonomy, belief, and compatibility fixture assertions.
- `engram-node` and TypeScript packages expose these capabilities as narrow
  transports and ergonomic wrappers over Rust behavior.

### Component / Module Decomposition

- Retrieval router modules: temporal, cue, hierarchical, semantic, graph,
  vector, fusion, explanation.
- Hierarchy modules: build inputs, construction algorithms, summary policy,
  path navigation, evaluation.
- Taxonomy modules: discovery, proposal, validation, merge, drift, repository
  persistence.
- Consolidation modules: planners, task executors, dry-run renderers, policy
  gates, mutation appliers, reports.
- Eval modules: fixture loading, expected recall, forbidden recall, policy
  leakage, ranking, hierarchy, taxonomy drift, belief lifecycle,
  compatibility probes.
- Binding/package modules: transport, validation, generated type re-exports,
  client facades, fixtures.

### Behavior & Rules

- Retrieval first filters by scope and policy, then routes by requested mode and
  available indexes, then fuses and composes results with explanations.
- Cue matching is deterministic and supports exact, partial, and best-match
  modes without requiring vector search.
- Hierarchy construction records provenance, source seeds, algorithm, version,
  and validation findings for every node and relation.
- Taxonomy proposals preserve source evidence and remain separate from accepted
  taxonomy state until validation and merge.
- Consolidation mutations are dry-run by default and require explicit policy and
  evaluation gates before apply.
- Belief valid-time intervals are start-inclusive and end-exclusive. Record-time
  reads fail explicitly unless a repository stores historical versions.
- Adapter-readiness probes use Engram vocabulary and metadata; they do not add
  AgentZero terms to Engram domain truth.

### Failure, Edge Cases & Resilience

- Missing retrieval sources degrade with `RetrievalSourceFailure` and preserve
  partial safe results when policy allows.
- Scope translation failures, policy-denied reads, invalid taxonomy merges,
  unsupported record-time queries, and destructive forget conflicts fail closed.
- Consolidation apply is idempotent by operation/run identifiers and reports
  partial failures without hiding skipped mutations.
- Taxonomy merge validation rejects cycles, duplicate preferred labels in a
  scheme, invalid cross-scheme mappings, and provenance-free changes.
- Hierarchy construction rejects multiple parents within a single tree version
  unless explicitly modeled as a DAG relation.

### Quality Attributes

- **Correctness:** every acceptance criterion maps to deterministic fixtures or
  mechanical gates.
- **Modularity:** every behavior lands in a focused crate/module matching the
  reference architecture; hooks guard against boundary backslide.
- **Inspectability:** every result, proposal, consolidation mutation, and
  retrieval item carries provenance and explanation.
- **Compatibility:** AgentZero-facing behavior is exercised through adapter
  probes, not by importing AgentZero route/API shapes into Engram.
- **Operability:** local SQLite-backed smoke paths and demo flows prove the
  library can run without hosted infrastructure.

### Dependencies & Integration

- Rust crates depend only in the direction allowed by `docs/architecture/reference.md`.
- SQLite adapters implement ports and translate adapter errors into
  `CoreError`; they do not define domain truth.
- FastEmbed and model-backed features stay feature-gated or trait-backed.
- TypeScript packages consume generated contracts and native transports.
- AgentZero consumes Engram through its separate adapter spec; Engram never
  depends on AgentZero crates.

## Tasks

### T0: Pre-implementation gate passes

**Depends on:** none

**Touches:** none

**Tests:**
- Goal-based: `.codex/hooks/pre-implementation-check.sh`.

**Approach:**
- Run the repository's mandatory pre-implementation check before any task edits
  runtime manifests, Rust code, TypeScript package code, adapters, bindings,
  generated contracts, or demo implementation.
- Treat failure as a blocker for implementation tasks, not as a warning to work
  around.

**Done when:** the gate passes in the current workspace and downstream
  implementation tasks record this prerequisite in their work-loop notes.

### T1: Architecture parity ledger is current

**Depends on:** T0

**Touches:** `docs/research/*.md`, `docs/arch_divergence.md`,
`docs/architecture/*.md`, `docs/specs/research-architecture-parity/*`,
`.codex/hooks/**`, `tools/**`

**Tests:**
- Goal-based: a bounded doc-drift lint reads an explicit registry of stale
  research claims, retired adapter names, required supersession markers, and
  active/inactive capability statuses, then scans `docs/research/`,
  `docs/arch_divergence.md`, and `docs/architecture/`.
- Goal-based: `.codex/hooks/check-docs.sh`.

**Approach:**
- Refresh stale research claims around belief valid-time behavior and retired
  in-memory adapters.
- Convert the current audit into a parity matrix with each gap linked to a spec
  task or explicit deferral.
- Add a small registry-backed doc lint rather than relying on a hardcoded grep;
  the registry is the place future stale-claim checks are added.
- Keep historical claims readable as history, but mark stale claims as superseded
  by current specs/implementation.

**Done when:** a contributor can read `docs/research/` and
`docs/arch_divergence.md` without learning an obsolete implementation fact.

### T2: Extension contracts describe the research architecture roles

**Depends on:** T1

**Touches:** `docs/domain-data-model.md`, `contracts/v1/**`,
`core/domain/src/**`, `packages/contracts/**`

**Tests:**
- TDD: domain invariant tests for memory roles, cue queries, taxonomy proposals,
  hierarchy build records, consolidation runs, and eval assertions.
- Goal-based: `pnpm run contracts:generate` produces no uncommitted generated
  drift after checked-in source changes.
- Goal-based: `.codex/hooks/check-contracts.sh`.

**Approach:**
- Clarify draft extension models before code changes.
- Add only compatible optional v1 schema fields/examples unless a separate
  contract review accepts a new version.
- Generate or update Rust/TypeScript shapes from the accepted source of truth.

**Done when:** every research architecture concept needed by later tasks has a
documented, storage-neutral contract shape and compatibility classification.

### T3: Memory subsystem roles are explicit and storage-neutral

**Depends on:** T2

**Touches:** `core/memory/**`, `core/domain/src/memory.rs`,
`adapters/memory/sqlite/**`, `core/eval/**`

**Tests:**
- TDD: role classification and lifecycle tests cover working, episodic,
  semantic, and procedural memory records.
- Integration: SQLite memory conformance fixture writes and retrieves each role
  with provenance, policy, valid intervals where applicable, and forget behavior.

**Approach:**
- Add role-specific request/filter helpers without creating separate god
  services.
- Keep working memory volatile at the contract level; persistent roles go
  through repository ports.
- Add eval fixtures that prove role-specific retrieval and forgetting.

**Done when:** memory subsystem roles are visible in contracts, ports, SQL-backed
fixtures, and eval reports.

### T4: Retrieval router supports all research modes on durable paths

**Depends on:** T2, T3

**Touches:** `core/retrieval/**`, `core/memory/**`, `core/knowledge/**`,
`core/hierarchy/**`, `adapters/*/sqlite*/**`, `bindings/node/**`,
`packages/**`

**Tests:**
- TDD: temporal, cue, hierarchical, semantic, graph, and vector routing tests
  exercise deterministic candidate selection and failure reporting.
- Integration: fused retrieval fixture proves scope/policy filtering,
  source-specific explanations, RRF traces, omitted results, and context budget
  behavior.

**Approach:**
- Add a storage-neutral query router that composes `RetrievalIndex`
  implementations and mode-specific filters.
- Re-land temporal, cue, and hierarchy expansion through durable adapters rather
  than retired fixture behavior.
- Wire predictive hints as optional query inputs, not autonomous background
  behavior.

**Done when:** all research retrieval modes are exercised through durable local
  adapters and exposed through Node/TS without store coupling in
  `engram-retrieval`.

### T5: Hierarchy construction is durable, explainable, and evaluable

**Depends on:** T2, T3, T4

**Touches:** `core/hierarchy/**`, `core/domain/src/hierarchy.rs`,
`adapters/hierarchy/sqlite/**`, `core/eval/**`, `bindings/node/**`,
`packages/**`, `demo/**`

**Tests:**
- TDD: builder tests reject invalid parentage, preserve provenance, and produce
  stable node/relation versions.
- Integration: hierarchy fixture builds raw event, episode, schema, and domain
  nodes, then navigates paths at bounded granularity.
- Manual QA: demo hierarchy navigation shows build metadata and path
  explanations.

**Approach:**
- Implement baseline deterministic builders before model-assisted summaries.
- Store construction algorithm metadata and validation findings.
- Keep navigation separate from construction and reusable across adapters.

**Done when:** hierarchy is no longer only ports plus path persistence; it has a
tested construction path and retrieval/navigation integration.

### T6: Taxonomy evolution is governed and drift-aware

**Depends on:** T2, T4

**Touches:** `core/knowledge/**`, `core/domain/src/taxonomy.rs`,
`adapters/knowledge/sqlite/**`, `core/eval/**`, `bindings/node/**`,
`packages/**`, `demo/**`

**Tests:**
- TDD: proposal validation rejects cycles, duplicate preferred labels, invalid
  relation types, missing provenance, and invalid cross-scheme mappings.
- Integration: taxonomy fixture runs discovery, proposal, validation, merge, and
  drift report flows over SQLite.
- Manual QA: demo taxonomy review displays proposal evidence, validation
  findings, and accepted changes.

**Approach:**
- Add proposal and validation ports behind `engram-knowledge`.
- Implement SQLite persistence for pending proposals and applied changes.
- Keep merge explicit and actor-scoped; drift detection remains advisory.

**Done when:** SKOS taxonomy evolution matches the research lifecycle without
  allowing autonomous default merges.

### T7: Consolidation pipeline has real task algorithms and gated apply

**Depends on:** T2, T3, T5, T6

**Touches:** `core/consolidation/**`, `core/belief/**`, `core/hierarchy/**`,
`core/knowledge/**`, `adapters/*/sqlite*/**`, `core/eval/**`

**Tests:**
- TDD: planners emit deterministic dry-run operations for memory-to-fact,
  memory-to-belief, hierarchy candidate, taxonomy candidate, graph candidate,
  and compaction tasks.
- Integration: gated apply fixture proves policy checks, idempotency, partial
  failure reports, provenance, and evaluation-gate rejection.

**Approach:**
- Extend existing dry-run/gated services with focused task executors.
- Keep scheduling out of Engram and follow ADR-0011's explicit-command baseline.
- Add evaluation gates before any mutation apply path.

**Done when:** consolidation is a formal pipeline with useful algorithms, not
  only an abstract service shell.

### T8: Belief lifecycle and temporal behavior are contract-complete

**Depends on:** T2, T3, T7

**Touches:** `core/belief/**`, `adapters/orchestration/belief-sqlite/**`,
`core/eval/**`, `bindings/node/**`, `packages/**`

**Tests:**
- TDD: valid-time reads, stale/clear, supersede, retract, source references,
  contradiction idempotency, contradiction resolution, and embedding ranking
  helpers cover AgentZero-compatible cases.
- Integration: SQLite belief fixture rejects record-time queries unless a
  versioned historical store exists.
- Adapter-readiness: AgentZero-like valid-time probes pass without AgentZero
  product vocabulary in Engram domain types.

**Approach:**
- Promote the current valid-time helpers into documented extension behavior.
- Decide whether record-time history remains explicitly unsupported or moves to
  a new accepted contract.
- Expose Node/TS helpers only as Rust-backed transports.

**Done when:** belief and contradiction behavior is honest, testable, and ready
  for adapter consumption without claiming unsupported bitemporality.

### T9: Evaluation harness proves architecture-level behavior

**Depends on:** T3, T4, T5, T6, T7, T8

**Touches:** `core/eval/**`, `packages/eval/**`, `docs/perf/**`,
`docs/specs/**`

**Tests:**
- Integration: fixture runner covers expected recall, forbidden recall, leakage,
  policy, ranking, hierarchy, taxonomy drift, belief lifecycle, contradiction
  review, consolidation gates, and adapter-readiness.
- Goal-based: eval reports are deterministic under repeated runs.

**Approach:**
- Extend fixture schema with scenario categories and assertions per architecture
  capability.
- Add report summaries that identify which architecture parity bar each fixture
  proves.
- Keep model-backed tests ignored or stubbed by default.

**Done when:** architecture parity is measured by executable fixtures instead of
  prose confidence.

### T10: Node, TypeScript, and demo surfaces expose the integrated library path

**Depends on:** T4, T5, T6, T7, T8, T9

**Touches:** `bindings/node/**`, `packages/{contracts,client,node,eval}/**`,
`demo/backend/**`, `demo/frontend/**`, `README.md`, `docs/guides/**`

**Tests:**
- Goal-based: `pnpm run typecheck`, `pnpm run test`, and `pnpm run build`.
- Goal-based: an architecture check fails when TypeScript package code
  reimplements representative domain behavior that must be delegated to the
  native Rust transport.
- Integration: representative TypeScript client calls for memory retrieval,
  hierarchy navigation, taxonomy review, consolidation dry-run, and belief
  lifecycle are asserted to delegate to Rust-backed transports and return Rust
  contract JSON.
- Manual QA: backend/frontend smoke exercises memory, knowledge, retrieval,
  hierarchy, taxonomy, consolidation, belief, and eval flows.

**Approach:**
- Add narrow N-API JSON transports for missing Rust capabilities.
- Keep generated contracts and package facades aligned.
- Add demo routes/panels only where they exercise the real Rust behavior.
- Add the TypeScript delegation check as a guardrail, not as a replacement for
  fixtures. The check should target known duplication hazards rather than ban
  harmless validation, formatting, or transport composition.

**Done when:** TypeScript consumers can use the architecture-complete local
  library path without reimplementing domain behavior.

### T11: Architecture guards and final parity gates pass

**Depends on:** T0-T10

**Touches:** `.codex/hooks/**`, `tools/**`, `docs/architecture/**`,
`docs/specs/README.md`, `docs/backlog.md`

**Tests:**
- Goal-based: hooks fail on retired in-memory adapters, forbidden imports,
  stale research claims, unresolved spec status drift, and god-module patterns.
- Gates:
  - `cargo fmt --all --check`
  - `cargo check --workspace`
  - `cargo test --workspace`
  - `pnpm run contracts:generate`
  - `pnpm run typecheck`
  - `pnpm run test`
  - `pnpm run build`
  - `.codex/hooks/check-contracts.sh`
  - `.codex/hooks/check-docs.sh`

**Approach:**
- Add mechanical checks only where they prove project invariants without
  replacing architectural judgment.
- Update backlog for explicitly deferred production substrates, hosted control
  plane, AgentZero provider switch, and full record-time bitemporality if not
  implemented.

**Done when:** the spec can move to Shipped with all acceptance criteria checked
  or explicitly deferred to backlog anchors.

## Rollout

This is an internal library parity program. Ship task slices as separate PRs in
dependency order. Each slice must preserve existing v1 compatibility and pass
the local gates before the next slice depends on it. There is no production
data migration, cloud deployment, or AgentZero provider switch in this spec.

## Risks

- **Scope creep:** "10/10" could absorb production deployment, hosted control
  plane, or AgentZero cutover. Mitigation: keep those as explicit non-goals or
  backlog items unless a new spec accepts them.
- **God-service pressure:** integrated architecture work tempts a single
  orchestration facade. Mitigation: enforce crate/module boundaries and review
  every task for mixed reasons to change.
- **Fixture gaps:** prose may claim parity without executable proof. Mitigation:
  every task has eval or integration fixtures tied to acceptance criteria.
- **Contract churn:** extension models may need promotion. Mitigation: classify
  compatible vs breaking changes before schema/type generation.
- **Temporal overclaim:** valid-time support could be mistaken for full
  bitemporality. Mitigation: repository APIs reject record-time history unless a
  historical store exists.

## Changelog

- 2026-07-02: initial plan.
