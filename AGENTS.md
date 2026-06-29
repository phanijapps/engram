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
  engram-core/          Engine orchestration, ports, policy, retrieval.
  engram-eval/          Deterministic fixtures and regression harness.
  engram-ingest/        Document/code source ingestion and chunking ports.
  engram-hierarchy/     Hierarchy construction, paths, expansion logic.
  engram-belief/        Belief derivation, contradiction, consolidation.
  engram-store-memory/  In-memory adapter for tests and first slices.
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
- `engram-core` owns deterministic memory behavior and depends on ports, not
  concrete infrastructure adapters.
- Store, vector, embedding, model, and gateway integrations belong in adapter
  crates or TypeScript packages.
- TypeScript must not redefine domain truth. It may wrap, validate, compose, and
  expose Rust-backed APIs.
- Generated contracts must be reproducible from source and should not be edited
  manually.
- Memory, knowledge, belief, hierarchy, policy, provenance, and evaluation
  concepts must remain distinct unless an ADR changes the model.

## Rust Standards

- Prefer small crates with explicit responsibilities over a large shared crate.
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
