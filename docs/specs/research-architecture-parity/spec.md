# Spec: Research Architecture Parity

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004, ADR-0005, ADR-0007, ADR-0008, ADR-0009, ADR-0010, ADR-0011, RFC-0001, RFC-0002, `docs/architecture/reference.md`, `docs/research/synthesis.md`, `docs/research/architecture-design-v2.md`
- **Brief:** none
- **Contract:** draft extension contracts for memory subsystems, retrieval modes, hierarchy construction, taxonomy evolution, consolidation, evaluation fixtures, and adapter-facing compatibility probes; accepted v1 wire contracts remain compatible unless a later contract review promotes a breaking version
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram reaches research-architecture parity for the `docs/research/` target
when the local Rust library, Node transport, TypeScript packages, demo smoke
paths, documentation, and validation hooks all agree on the same behavior. The
memory layer exposes explicit working, episodic, semantic, and procedural memory
roles; composes temporal, cue-based, hierarchical, semantic, graph, and vector
retrieval; supports governed taxonomy evolution, hierarchy construction,
consolidation, belief valid-time behavior, provenance, policy, and evaluation as
first-class capabilities; passes the repository gate set in this spec; fails
mechanically on retired-adapter, stale-doc, god-module, and TypeScript
domain-duplication regressions; and provides AgentZero adapter-readiness probes
without switching AgentZero to Engram.

## Boundaries

### Always do

- Keep `docs/domain-data-model.md` as the domain source of truth until an ADR
  promotes Rust domain types as the generated-contract source.
- Preserve the memory, knowledge, belief, hierarchy, consolidation, retrieval,
  policy, provenance, and evaluation boundaries named in `AGENTS.md` and
  `docs/architecture/reference.md`.
- Implement behavior in focused Rust crates and modules; crate roots and package
  entry points remain facades.
- Keep durable local conformance SQLite-backed, with vector behavior behind
  `engram-store-vector` and model-backed behavior behind traits.
- Keep TypeScript packages as ergonomic transports and clients over Rust
  behavior, not a second implementation.
- Treat AgentZero integration readiness as an adapter-facing compatibility
  contract: stable scopes, valid-time behavior, fixtures, mappings, and
  diagnostics.
- Keep every new behavior covered by deterministic eval fixtures or mechanical
  gates before calling it architecture-complete.
- Update stale research and divergence docs in the same implementation slices
  that close or reclassify those gaps.

### Ask first

- Promote draft extension contracts into accepted versioned public contracts.
- Add full record-time bitemporal audit reads or a two-axis temporal query API.
- Add a background scheduler, autonomous taxonomy merge, autonomous hierarchy
  rebuild, or automatic consolidation trigger beyond ADR-0011's explicit-command
  baseline.
- Add a production storage substrate such as Postgres, pgvector, Neo4j, object
  storage, or a hosted control plane.
- Change accepted v1 JSON schemas, generated TypeScript contracts, public Rust
  trait signatures, or public Node/SDK payloads.
- Add an AgentZero-specific route, settings file reader, scheduler, UI DTO, or
  product vocabulary to Engram core.

### Never do

- Create a god service, god adapter, or god package that owns construction,
  validation, state, orchestration, scoring, persistence, scheduling, and error
  translation at once.
- Reintroduce the retired broad in-memory memory or knowledge adapters as active
  conformance surfaces.
- Collapse memory, knowledge, belief, hierarchy, policy, provenance, taxonomy,
  consolidation, retrieval, or evaluation into one table, type, package, or
  API concept.
- Treat vector search as the whole memory layer.
- Claim full bitemporality unless record-time storage and record-time query
  semantics are specified and tested.
- Let AgentZero compatibility change Engram's portable ontology, route shape, or
  scheduler ownership.

## Testing Strategy

- **TDD:** domain invariants, temporal predicates, cue matching, lifecycle
  transitions, taxonomy proposal validation, hierarchy construction rules,
  consolidation planning, and ranking helpers use focused unit tests.
- **Goal-based integration:** SQLite-backed vertical fixtures prove write,
  retrieve, explain, consolidate, evaluate, forget, hierarchy, taxonomy, belief,
  graph, and vector behavior through public ports.
- **Goal-based contract checks:** generated contracts, schema examples, Rust
  public APIs, Node bindings, and TypeScript package exports stay reproducible
  and compatible.
- **Goal-based architecture checks:** hooks prove retired in-memory adapters,
  forbidden cross-crate imports, stale research claims, and god-module shapes do
  not re-enter active code.
- **Manual QA / smoke:** the demo backend and frontend exercise the integrated
  local library path for indexing, graph exploration, hybrid retrieval, belief
  review, hierarchy navigation, taxonomy review, and evaluation reporting.
- **Adapter-readiness fixtures:** AgentZero-compatible probes verify scope
  mapping, valid-time behavior, source provenance, belief lifecycle, recall
  result projection, capability reporting, and migration diagnostics without
  switching AgentZero's provider.

## Acceptance Criteria

- [x] `docs/research/*.md`, `docs/arch_divergence.md`, and
  `docs/architecture/*` accurately describe current implementation status; no
  active research note says a shipped capability is absent or that a retired
  adapter is required.
- [x] `.codex/hooks/pre-implementation-check.sh` passes before any task that
  changes runtime manifests, Rust code, TypeScript package code, adapters,
  bindings, generated contracts, or demo implementation begins.
- [x] Domain and contract docs define explicit working, episodic, semantic, and
  procedural memory roles, with storage-neutral Rust ports or existing domain
  mappings for each role.
- [x] Retrieval supports temporal, cue-based, hierarchical, semantic, graph, and
  vector modes through `engram-retrieval` without making retrieval fusion call
  storage adapters, embedding providers, policy engines, or model rerankers
  directly.
- [x] SQLite-backed memory, knowledge, hierarchy, belief, taxonomy, graph, and
  vector fixtures prove the local conformance path without any active broad
  in-memory adapter.
- [x] Hierarchy has durable construction and navigation behavior with provenance,
  algorithm metadata, and eval coverage for raw event, episode, schema, and
  domain-level navigation.
- [x] Taxonomy supports governed `Discovery -> Proposal -> Validation -> Merge`
  with provenance, validation findings, cross-scheme mappings, advisory semantic
  drift reporting, and no autonomous merge by default.
- [x] Consolidation is a separately owned pipeline with explicit inputs,
  dry-run output, gated apply, conflict handling, provenance, policy checks, and
  evaluation reports for memory-to-fact, memory-to-belief, hierarchy, taxonomy,
  and graph candidate operations.
- [x] Belief valid-time behavior is complete for live reads, stale state,
  supersession, retraction, source references, contradiction idempotency,
  contradiction resolution, and embedding/ranking helpers; record-time history is
  either implemented behind an accepted contract or explicitly rejected at the
  repository boundary.
- [x] Policy and provenance are visible on write, retrieve, ingest, consolidate,
  taxonomy merge, hierarchy build, belief mutation, and forget paths.
- [x] Evaluation fixtures cover accepted recall, forbidden recall, leakage,
  policy filtering, ranking, hierarchy granularity, taxonomy drift, belief
  lifecycle, contradiction review, and adapter-readiness scenarios.
- [x] Node bindings and TypeScript packages expose narrow, generated-contract
  aligned surfaces over Rust behavior for memory, knowledge, retrieval, belief,
  hierarchy, taxonomy, consolidation, and evaluation without duplicating domain
  logic; an architecture check and fixtures fail if representative behavior is
  reimplemented in TypeScript instead of delegated to Rust transports.
- [x] AgentZero can build an adapter against Engram using documented library
  ports, compatibility fixtures, scope mapping, valid-time probes, capability
  reports, and migration diagnostics; this spec does not switch AgentZero to
  Engram or require AgentZero route/UI changes.
- [x] Architecture and validation hooks fail if a crate/package grows into a god
  module, if retired in-memory adapters return, or if active docs drift from the
  shipped implementation.
- [x] Required repository gates pass:
  `cargo fmt --all --check`, `cargo check --workspace`, `cargo test --workspace`,
  `pnpm run contracts:generate`, `pnpm run typecheck`, `pnpm run test`,
  `pnpm run build`, `.codex/hooks/check-contracts.sh`, and
  `.codex/hooks/check-docs.sh`.

## Assumptions

- Technical: Engram is a contract-first Rust core with TypeScript ergonomics,
  SQLite-backed local conformance, sqlite-vec retrieval, N-API JSON transport,
  and demo-scale local deployment (source: `docs/architecture/reference.md`).
- Technical: current architecture divergence is concentrated in taxonomy
  evolution, durable temporal/cue/hierarchy retrieval, predictive retrieval
  wiring, hierarchy construction, and additional consolidation algorithms
  (source: `docs/arch_divergence.md`).
- Technical: the broad in-memory memory and knowledge adapters are retired and
  must not re-enter active workspace conformance (source:
  `docs/specs/retire-memory-inmem/spec.md`,
  `docs/specs/retire-knowledge-inmem/spec.md`, and workspace `Cargo.toml`).
- Technical: AgentZero provider cutover has its own integration specs and is
  not part of this spec's implementation scope (source:
  `docs/specs/agentzero-engram-adapter-integration/spec.md` and user
  confirmation 2026-07-02).
- Product: "10/10" means research-architecture parity for Engram as a pristine
  local library and SDK surface, excluding hosted control plane and actual
  AgentZero provider switch (source: user confirmation 2026-07-02).
- Process: this work runs in full work-loop mode because it is multi-feature,
  structural, and public-interface-affecting (source: work-loop risk triggers).
