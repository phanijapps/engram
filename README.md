# Engram

> Contract-first agentic memory for agents, tools, and applications that need
> durable recall without turning memory into a monolith.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)
[![TypeScript](https://img.shields.io/badge/typescript-sdk-blue.svg)](packages)
[![Status](https://img.shields.io/badge/status-pre--1.0-yellow.svg)](#status)

Engram is an open-source memory layer with a Rust core and TypeScript bindings.
It is built around explicit contracts, deterministic behavior, and replaceable
adapters so memory can evolve from local fixtures to durable stores, vector
indexes, hierarchy navigation, belief networks, and agent integrations.

## Why Engram

Agent memory gets messy when storage, ranking, policy, provenance, and runtime
integration collapse into one service. Engram keeps those concerns separate:

- Contract-first v1 memory, retrieval, forget, and evaluation payloads.
- Rust 2024 core traits and deterministic adapter behavior.
- TypeScript packages generated from the accepted contracts.
- SQLite persistence and sqlite-vec retrieval test surfaces.
- In-memory hierarchy, belief, contradiction, consolidation, and evaluation
  slices.
- Local-first examples, benchmark smoke paths, and release gates.

## Status

Engram is **pre-1.0**. It is suitable for contract and adapter development, not
production deployment.

Current validated surface includes:

- memory write, retrieve, forget, and lifecycle events
- accepted v1 JSON schemas and TypeScript contract generation
- reusable Rust evaluation fixtures and report summaries
- in-memory and SQLite-backed memory services
- file-backed SQLite local smoke support
- sqlite-vec candidate retrieval with opt-in FastEmbed BGE-small test wiring
- source-grounded document/code ingestion
- native TypeScript binding package surface
- framework-neutral observed transport adapters

Before publishing crates, npm packages, release tags, or benchmark claims, use
`docs/release-checklist.md`.

## Architecture

```text
                         +-------------------------------+
                         |        Applications / Agents   |
                         |  SDKs, gateways, tools, CLIs   |
                         +---------------+---------------+
                                         |
                                         v
        +----------------------+  +------+----------------+  +----------------------+
        | TypeScript packages  |  |      N-API bridge     |  | Runtime adapters     |
        | contracts/client/... |  |      engram-node      |  | packages/adapters    |
        +----------+-----------+  +----------+------------+  +----------+-----------+
                   |                         |                          |
                   +-------------------------+--------------------------+
                                             |
                                             v
        +----------------------------------------------------------------+
        |                         Rust core ports                         |
        | engram-core: MemoryService, repositories, retrieval, policy,   |
        | evaluation, consolidation, ingestion, hierarchy, belief ports   |
        +-------------------------------+--------------------------------+
                                        |
                                        v
        +-------------------------------+--------------------------------+
        |                       Domain contract layer                     |
        | engram-domain + contracts/v1: memory, knowledge, policy,       |
        | provenance, retrieval, forget, evaluation, hierarchy, belief    |
        +-------------------------------+--------------------------------+
                                        |
          +-----------------------------+-----------------------------+
          |                             |                             |
          v                             v                             v
+--------------------+        +--------------------+        +--------------------+
| Memory adapters    |        | Knowledge ingest   |        | Retrieval adapters |
| in-memory, SQLite  |        | docs, files, code  |        | sqlite-vec, fusion |
+---------+----------+        +---------+----------+        +---------+----------+
          |                             |                             |
          v                             v                             v
+--------------------+        +--------------------+        +--------------------+
| Local state / SQL  |        | Source documents   |        | Vector candidates  |
| events/idempotency |        | chunks/provenance  |        | rehydrated targets |
+--------------------+        +--------------------+        +--------------------+
```

The rule of thumb: `engram-core` owns ports and deterministic behavior,
`engram-domain` owns portable concepts, concrete infrastructure lives behind
adapters, and TypeScript wraps generated contracts instead of redefining them.

## Repository Layout

```text
contracts/        Accepted JSON schemas, examples, and contract notes.
crates/           Rust workspace: domain, core, memory, SQL, vector, ingest,
                  evaluation, retrieval, and node bridge crates.
docs/             Architecture docs, ADRs, RFCs, research, specs, and roadmap.
examples/         Scenario fixtures and usage sketches.
packages/         TypeScript contracts, client, node package, adapters, eval.
specs/            V1 acceptance specs and legacy implementation phase specs.
.codex/           Local agent skills, review agents, and validation hooks.
```

## Quick Start

Install dependencies:

```bash
pnpm install
python3 -m pip install -r requirements-dev.txt
```

Run the Rust workspace:

```bash
cargo test --workspace
```

Run TypeScript generation, typechecks, and tests:

```bash
pnpm run check
```

Run local adapter examples:

```bash
cargo run -p engram-store-memory --example local_memory
cargo run -p engram-store-sql --example sql_memory
pnpm --filter @engram/client test
```

Run the local benchmark smoke path:

```bash
cargo run -p engram-store-memory --example benchmark_local
```

Benchmark output is local observation only. See `docs/benchmarks.md` for claim
boundaries.

## Contracts

The accepted v1 contract package lives in `contracts/v1/`.

Useful commands:

```bash
python3 scripts/validate_contracts.py
pnpm run contracts:generate
pnpm run contracts:check-generated
.codex/hooks/check-contracts.sh
```

Generated TypeScript types are emitted under `packages/contracts/src/generated/`
and should not be edited by hand.

## Development Workflow

Engram uses spec-driven implementation:

1. Record durable architecture decisions in `docs/adr/`.
2. Add or update specs under `docs/specs/` before behavior changes.
3. Keep `docs/implementation/phases.json` in sync with roadmap slices.
4. Run Rust, TypeScript, contract, docs, and vector feature gates before handoff.

Core validation:

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm run check
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
```

Vector/FastEmbed feature gate:

```bash
cargo check -p engram-store-vector --features fastembed-tests --tests
cargo clippy -p engram-store-vector --features fastembed-tests --tests -- -D warnings
```

The FastEmbed BGE-small path stays opt-in; default validation does not download
models.

## Documentation

- `docs/architecture.md` - module map and system architecture.
- `docs/domain-data-model.md` - domain model source of truth.
- `docs/implementation-roadmap.md` - completed roadmap loop and next-slice
  policy.
- `docs/sql-adapter-design.md` - SQLite adapter boundary and deferred server DB
  work.
- `docs/benchmarks.md` - local benchmark smoke commands and limitations.
- `docs/release-checklist.md` - release and publication gates.

## Contributing

Contributions are welcome while the project is pre-1.0, but contract discipline
is strict:

- Start with an issue, ADR, RFC, or spec for behavior changes.
- Keep public contracts compatible unless a breaking change is explicitly
  accepted.
- Keep crate roots and package entry points as narrow facades.
- Do not add god modules, hidden infrastructure coupling, or provider-backed
  behavior in core/domain crates.

Read:

- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`
- `GOVERNANCE.md`
- `AGENTS.md`

## License

MIT. See `LICENSE`.
