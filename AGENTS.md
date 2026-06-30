# Repository Instructions

Engram is a contract-first Agentic Memory layer with a Rust core and
TypeScript bindings/SDK. The repository should stay modular: domain contracts
come first, Rust owns deterministic behavior, TypeScript owns integration
ergonomics, and infrastructure lives behind adapters.

## Current Rule

Do not add runtime manifests or implementation code until
`docs/adr/0003-implementation-stack.md` exists. Before implementation work, run:

```bash
.codex/hooks/pre-implementation-check.sh
```

## Target Repository Shape

```text
contracts/                 Portable JSON schemas and generated contract outputs.
docs/                      Architecture, ADRs, RFCs, research, and domain model.
examples/                  Scenario fixtures and usage sketches.

crates/                    Rust workspace.
  engram-domain/        Domain types, invariants, serde, version markers.
  engram-runtime/       Shared errors, result type, clocks, ids, policy gates.
  engram-memory/        Memory service and repository ports.
  engram-knowledge/     Knowledge, graph, ontology, source, ingestion ports.
  engram-core/          Orchestration facade, retrieval, consolidation.
  engram-eval/          Deterministic fixtures and regression harness.
  engram-ingest/        Document/code source ingestion and chunking ports.
  engram-hierarchy/     Hierarchy construction, paths, expansion logic.
  engram-belief/        Belief derivation, contradiction, consolidation.
  engram-store-memory/  In-memory memory adapter for quick tests only.
  engram-store-knowledge-memory/ In-memory knowledge/graph/ontology test adapter.
  engram-store-sql/     SQL persistence adapter.
  engram-store-vector/  Vector index adapter.
  engram-node/          N-API bridge for TypeScript.

packages/                  TypeScript workspace.
  contracts/               Generated TypeScript types and schemas.
  client/                  Ergonomic application SDK.
  node/                    Native binding package.
  adapters/                JS-side framework and gateway integrations.
  eval/                    Fixture authoring helpers and CLI wrappers.
```

Existing placeholder folders under `packages/` may be renamed to this shape when
the stack ADR is accepted.

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
- `engram-store-memory` is a quick memory fixture. Do not add graph, ontology,
  embedding provider, durable document, or production cache behavior to it.
- `engram-store-knowledge-memory` is a quick knowledge, graph, and ontology
  fixture. It must not own memory writes, memory lifecycle events, or memory
  forget semantics.
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
- Put spec-driven acceptance criteria under `specs/v1/`.
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
