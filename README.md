# Engram

This repository is the starting workspace for an agentic memory layer. The
implementation stack is Rust 2024 for the deterministic core and TypeScript for
bindings, SDKs, and application integrations.

## Goals

- Compose memory capabilities behind stable interfaces.
- Keep storage, retrieval, ranking, consolidation, and policy enforcement
  independently replaceable.
- Preserve provenance and permissions on every memory operation.
- Make memory quality measurable through repeatable evaluations.
- Support local-first development with a clear path to service deployment.

## Status

Engram is pre-1.0 and contract-first. The accepted v1 core contract lives under
`contracts/v1/`, with Rust crates and TypeScript packages being implemented
through spec-driven slices. Current behavior includes memory write/retrieve/
forget, evaluation fixtures, SQLite persistence, native TypeScript bindings,
source-grounded ingestion, sqlite-vec retrieval tests, hierarchy and belief
repositories, dry-run consolidation reporting, and framework-neutral adapter
observability.

## Workspace

```text
contracts/          External contracts and portable schemas.
crates/             Rust workspace for domain models, core traits, and adapters.
docs/               Architecture notes, ADRs, RFCs, and research.
examples/           Scenario fixtures and usage sketches.
packages/           TypeScript contracts, native bindings, client, and adapters.
specs/              Spec-driven acceptance contracts for implementation slices.
```

## Current Phase

1. Keep `docs/domain-data-model.md` aligned with accepted contracts.
2. Use `contracts/v1/` as the machine-readable v1 contract package.
3. Add or update a spec before implementation slices.
4. Add deterministic evaluation fixtures alongside behavior slices.
5. Keep Rust behavior, TypeScript bindings, and generated contracts in sync.
6. Avoid production, security, or performance claims until backed by release
   gates and evidence.

## Architectural Bias

The memory layer should behave like a small platform, not a single monolithic
agent feature. The default shape is ports and adapters:

- `contracts/v1` owns accepted v1 wire contracts.
- `engram-domain` owns Rust projections of portable memory concepts.
- `engram-core` owns behavior traits and ports.
- Store, vector, embedding, and gateway adapters implement those ports.
- TypeScript packages expose generated contracts, native bindings, SDKs, and
  integration helpers.

See `docs/architecture.md` for the initial module map.

## Release Readiness

Engram is not production-ready. Before publishing crates, npm packages, or
release tags, follow `docs/release-checklist.md`.

## Validation

```bash
python3 -m pip install -r requirements-dev.txt
python3 scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
pnpm install
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
pnpm run build
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
```

## Contributing

See `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, and
`GOVERNANCE.md`.

## License

MIT. See `LICENSE`.
