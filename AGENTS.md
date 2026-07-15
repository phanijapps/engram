# Repository Instructions

Engram is a contract-first Agentic Memory layer with a Rust core and
TypeScript bindings/SDK. The repository should stay modular: domain contracts
come first, Rust owns deterministic behavior, TypeScript owns integration
ergonomics, and infrastructure lives behind adapters.

## Current Rule

The implementation stack is accepted (`docs/adr/0003-implementation-stack.md`,
Status: Accepted). Before implementation work, run:

```bash
.codex/hooks/pre-implementation-check.sh
```

## Target Repository Shape

```text
contracts/                 Portable JSON schemas and generated contract outputs.
docs/                      Architecture, ADRs, RFCs, research, and domain model.
examples/                  Scenario fixtures and usage sketches.

core/                      Storage-neutral Rust crates.
  domain/                  Domain types, invariants, serde, version markers.
  runtime/                 Shared errors, result type, clocks, ids, policy gates.
  memory/                  Memory service and repository ports.
  knowledge/               Knowledge, graph, ontology, source, ingestion ports.
  belief/                  Belief synthesis, contradiction, and bi-temporal ports.
  hierarchy/               Hierarchy build, navigation, and aggregate ports.
  consolidation/           Consolidation planning, gated mutation, decay, audit.
  reflection/              Reflection synthesizer + consolidation executor (derived beliefs).
  retrieval/               Retrieval composition and fusion ports.
  orchestration/           Orchestration facade and compatibility re-exports.
  integration/             SDK facade: EngramProvider, EngramConfig, CapabilityReport.
  eval/                    Deterministic fixtures and regression harness.
  graph-analytics/         Pure graph algorithms (PageRank, betweenness, communities, reachability).

adapters/                  Replaceable infrastructure crates.
  ingest/                  Filesystem/Git ingestion adapter until split.
  memory/sqlite/           SQLite memory persistence adapter.
  knowledge/sqlite/        SQLite knowledge, graph, taxonomy, and ontology adapter.
  hierarchy/sqlite/        SQLite hierarchy persistence adapter.
  orchestration/belief-sqlite/  SQLite belief and contradiction persistence adapter.
  retrieval/sqlite-vec/    sqlite-vec retrieval index adapter.
  retrieval/tantivy-lexical/       BM25 lexical retrieval index adapter (keyword mode).
  retrieval/cross-encoder-rerank/  Cross-encoder reranker adapter.
  retrieval/associative-graph/     Associative (Personalized PageRank) retrieval index adapter.
  integration/             Backend recipe / conformance composition (SQLite wiring until backends/ split).

backends/                  Backend recipe crates (ADR-0022). A *backend* is one
  sqlite/                  recipe that composes adapter cells + owns connection
                           lifecycle, config validation, and per-engine
                           conformance. SQLite is the only implemented backend
                           today; `backends/` is created when a second engine
                           arrives (YAGNI). The current SQLite wiring lives in
                           `adapters/integration` until then.

bindings/                  Native language bridges.
  node/                    N-API bridge for TypeScript.

codegraph/                 On-top codegraph layer (RFC-0012): code-specific
  queries/                 crates built on engram (dead-code / blast-radius /
                           dependency-path over call edges).
  temporal/                Temporal scoring (recent / impact / compound) over versioned symbols.
  mcp-server/              MCP server exposing codegraph queries to AI agents.

packages/                  TypeScript workspace.
  contracts/               Generated TypeScript types and schemas.
  client/                  Ergonomic application SDK.
  node/                    Native binding package.
  adapters/                JS-side framework and gateway integrations.
  eval/                    Fixture authoring helpers and CLI wrappers.
```

The `packages/` workspace already matches this shape (adapters, client,
contracts, eval, node) now that the stack ADR (ADR-0003) is accepted.

## Boundary Rules

- `docs/domain-data-model.md` is the source of truth until Rust domain types are
  accepted as the generated-contract source.
- `engram-domain` must not depend on SQL, vector stores, embedding providers,
  async runtimes, Node, N-API, or TypeScript tooling.
- `engram-runtime` owns shared runtime primitives only: portable errors, result
  type, clocks, id generation, scope matching, and policy authorizer traits.
- `engram-memory` owns memory service and repository ports. It must not own
  knowledge graph, ontology, source ingestion, vector, or document parsing
  contracts.
- `engram-knowledge` owns source-grounded knowledge, graph, ontology, source
  reader, chunker, and ingestion ports. It must not own memory write, lifecycle
  event, or forget service contracts.
- `engram-core` is an orchestration facade and compatibility re-export layer
  above split behavior crates. It must not become the canonical owner of memory
  or knowledge ports again.
- `engram-store-sql` is the active local memory adapter. It owns memory records,
  lifecycle events, idempotency, write/retrieve/forget behavior, and local
  in-memory/file-backed SQLite construction only.
- `engram-store-knowledge-sqlite` is the active local knowledge, graph,
  taxonomy, and ontology adapter. It must not own memory writes, memory
  lifecycle events, memory forget semantics, vector indexes, or sibling store
  internals.
- `engram-store-lexical` is the BM25 lexical retrieval adapter (Tantivy). It
  implements the contracted `RetrievalMode::keyword`; Tantivy must stay in this
  adapter and must not enter `engram-domain` or `engram-retrieval` core.
- `engram-rerank-cross-encoder` is the cross-encoder reranker adapter. It
  implements the contracted `RerankStrategy::cross_encoder`; model integrations
  stay behind the injected `RerankScorer` or a feature gate, never in core.
- **Engine neutrality (ADR-0022).** `engram-domain`, the other `core/*` port
  crates, `engram-integration` (the SDK facade: `EngramProvider`,
  `EngramConfig`, `CapabilityReport`), and the N-API `bindings/node` crate must
  never name an engine type (`Sql*`, `pgvector`, …) or hold SQL. The only place
  an engine name may appear in those layers is as a config string. This is what
  makes backend swap-by-config true; a neutrality lint enforces it.
- **Engines live on a capability × engine grid (ADR-0022).** Each storage engine
  is one adapter cell at `adapters/<capability>/<engine>` behind a core port
  (memory, knowledge, belief, hierarchy, retrieval/vector, retrieval/lexical).
  Engines are interchangeable within a capability slot; a new engine is an
  additive cell, never a rewrite of neutral layers.
- **A backend is a recipe crate (ADR-0022).** `backends/<name>` owns connection
  lifecycle, config validation, adapter composition, and per-engine conformance
  only — never ports, domain types, or capability logic. It composes adapter
  cells into an `EngramProvider` and is the only place a "backend" identity
  exists. SQLite wiring currently lives in `adapters/integration`; it moves to
  `backends/sqlite` when a second engine is adopted.
- **Surface parity — integration + N-API (and every transport).** Every runtime
  capability, operation, and `RetrievalIndex` / `RetrievalMode` must be reachable
  through BOTH `engram-integration` (the Rust SDK `EngramProvider` facade,
  including its unified-recall lanes) AND the N-API binding (`bindings/node`, the
  TS transport) — and through any other supported transport — and be reflected in
  `CapabilityReport`. A capability is not "shipped" until a Rust embedder and a
  TS/N-API agent can both invoke it. Wiring one surface and leaving the other
  unwired creates transport asymmetry (e.g., a retriever reachable from the
  binding but not the facade) and is not allowed. The `lexical-keyword-retrieval`
  + `lexical-wiring` split is the precedent for *sequencing* an adapter unit
  ahead of its wiring — never for permanently stranding a capability on one
  surface. If a surface genuinely cannot carry a capability, record why in an
  ADR. A parity lint (mirroring the engine-neutrality lint) is the intended
  enforcement.
- `engram-graph-analytics` owns pure, dependency-free graph algorithms only
  (PageRank, betweenness, communities, reachability). It must not depend on
  `engram-domain`, storage, or any infrastructure; callers map domain edges to a
  generic edge list at the call site.
- The `codegraph/` area is the on-top codegraph layer (RFC-0012), not engram.
  Its crates depend only on `engram-domain`, `engram-graph-analytics`, or other
  engram ports; they must not own storage/infra, duplicate domain truth, or live
  under `core/`/`adapters/`/`bindings/`.
- Store, vector, embedding, model, and gateway integrations belong in adapter
  crates or TypeScript packages.
- TypeScript must not redefine domain truth. It may wrap, validate, compose, and
  expose Rust-backed APIs.
- Generated contracts must be reproducible from source and should not be edited
  manually.
- Memory, knowledge, belief, hierarchy, policy, provenance, and evaluation
  concepts must remain distinct unless an ADR changes the model.
- Do not create god classes, god modules, or god packages. A file that owns
  construction, validation, state, orchestration, scoring, persistence, and
  error translation at the same time must be split before handoff.
- Crate roots and package entry points should be facades: module declarations,
  narrow public re-exports, and top-level documentation only. Put behavior in
  focused modules named for the responsibility they own.
- When a module grows to mix multiple reasons to change, split it by boundary:
  domain model, validation, state, repository adapter, operation orchestration,
  scoring/ranking, policy, serialization, or external integration.

## Rust Standards

- Prefer small crates with explicit responsibilities over a large shared crate.
- Prefer focused modules with explicit responsibilities over large `lib.rs`,
  `main.rs`, `mod.rs`, or catch-all service files.
- Keep public service structs thin. They may compose dependencies and implement
  traits, but operation-specific behavior should live in focused modules or
  collaborators.
- Do not hide unrelated behavior behind a generic `Manager`, `Service`,
  `Engine`, `Processor`, or `Handler` unless the surrounding modules make the
  real responsibilities explicit.
- Use typed errors; avoid stringly public error contracts.
- Keep policy checks visible on write, retrieve, ingest, consolidate, and forget
  paths.
- Keep provider-backed behavior behind traits so tests can use deterministic
  stubs.
- Add focused unit tests for invariants and integration tests for vertical
  memory flows.
- Do not optimize before correctness fixtures and basic benchmarks exist.

## TypeScript Standards

- Keep public APIs typed from generated contracts.
- Keep package entry points narrow and stable.
- Keep package `index.ts` files as public facades. Move validation, transport,
  native binding calls, adapters, fixtures, and formatting into focused modules.
- Avoid monolithic SDK clients that own transport, retries, validation,
  serialization, policy decisions, and fixture execution in one class.
- Treat the native binding package as a transport over Rust behavior, not a
  second implementation.
- Put framework-specific code in `packages/adapters/`, not in the client core.
- Add type tests or compile checks for exported SDK shapes once the TS workspace
  exists.

## Documentation Standards

- Put research notes, excerpts, and links under `docs/research/`.
- Record durable technical decisions under `docs/adr/`.
- Record design proposals under `docs/rfcs/`.
- Update `docs/domain-data-model.md` before changing contracts, schemas, or
  generated public types.
- Classify public contract changes as compatible or breaking.
- Put accepted machine-readable contract artifacts under `contracts/v1/`.
- Put spec-driven acceptance criteria under `docs/specs/`.
- Do not treat `contracts/schemas/` as the source of truth; those files are
  legacy pointers.

## Validation

Use these checks before handoff:

```bash
cargo fmt --all
cargo check --workspace
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/check-engine-neutrality.sh   # ADR-0022 rule-1 gate (engine neutrality)
```

Run `pnpm run build` after TypeScript package surface changes.

## Local Codex Assets

- Use `.codex/skills/engram-contract` for changes to
  `docs/domain-data-model.md`, JSON schemas, generated contracts, or
  compatibility policy.
- Use `.codex/skills/engram-plan` when sequencing crates, packages, adapters,
  bindings, or milestones.
- Use `.codex/skills/engram-eval` when designing recall, leakage, policy,
  ranking, belief, hierarchy, or ingestion evaluations.
- Use `.codex/skills/engram-code-docs` when adding or reviewing Rust,
  TypeScript, SDK, binding, adapter, example, or public API documentation.
- Use `.codex/agents/` as role briefs for contract, Rust-core, evaluation, and
  integration-boundary reviews.
- Install local Git hooks with `git config core.hooksPath .githooks` when this
  workspace should enforce checks on commit.
