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
- Focused SQLite-backed memory, knowledge, hierarchy, belief, and retrieval
  adapter slices.
- Local-first examples, benchmark smoke paths, and release gates.

## Status

Engram is **pre-1.0**. It is suitable for contract and adapter development, not
production deployment.

Current validated surface includes:

- memory write, retrieve, forget, and lifecycle events
- accepted v1 JSON schemas and TypeScript contract generation
- reusable Rust evaluation fixtures and report summaries
- SQLite-backed memory service and local in-memory SQLite conformance tests
- SQLite-backed knowledge graph, taxonomy, and ontology adapter
- storage-neutral retrieval composition and weighted fusion
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
        |                    Rust behavior boundaries                     |
        | runtime: errors/policy deps · memory: memory ports · knowledge: |
        | graph/ontology/source ports · retrieval: composition/fusion     |
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
| Memory adapters    |        | Knowledge adapters |        | Retrieval adapters |
| SQLite memory SQL  |        | SQLite graph/store |        | sqlite-vec, fusion |
+---------+----------+        +---------+----------+        +---------+----------+
          |                             |                             |
          v                             v                             v
+--------------------+        +--------------------+        +--------------------+
| Local state / SQL  |        | Sources/chunks     |        | Vector candidates  |
| events/idempotency |        | graph/ontology     |        | rehydrated targets |
+--------------------+        +--------------------+        +--------------------+
```

The rule of thumb: `engram-domain` owns portable concepts, `engram-memory`
owns memory ports, `engram-knowledge` owns knowledge graph, ontology, source,
and ingestion ports, `engram-retrieval` owns candidate composition and fusion,
`engram-core` keeps higher-level orchestration and compatibility re-exports,
concrete infrastructure lives behind adapters, and TypeScript wraps generated
contracts instead of redefining them.

## Repository Layout

```text
contracts/        Accepted JSON schemas, examples, and contract notes.
core/             Storage-neutral Rust crates: domain, runtime, memory,
                  knowledge, retrieval, orchestration, and evaluation.
adapters/         Replaceable Rust infrastructure: ingest, memory stores,
                  knowledge stores, and retrieval indexes.
bindings/         Native language bridges, including the Node N-API crate.
docs/             Architecture docs, ADRs, RFCs, research, specs, and roadmap.
examples/         Scenario fixtures and usage sketches.
packages/         TypeScript contracts, client, node package, adapters, eval.
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
cargo run -p engram-store-sql --example sql_memory
pnpm --filter @engram/client test
```

Run the local benchmark smoke path:

```bash
cargo run -p engram-store-sql --example benchmark_sql
```

Benchmark output is local observation only. See `docs/benchmarks.md` for claim
boundaries.

## Demo: build & run on a new machine

The demo (`demo/`) is an enterprise knowledge-platform UI over the Rust core:
index a polyglot repo or docs folder, build a knowledge graph, ask grounded +
agentic questions, and explore the graph. It needs the **FastEmbed** native
build (BGE-small embeddings) plus the TypeScript workspace.

### Prerequisites

- **Rust 1.85+** (edition 2024) — `cargo`.
- **Node 22+** and **pnpm 10** (`corepack enable && corepack prepare pnpm@10 --activate`).
- Optional, for LLM extraction + Q&A: an OpenAI-compatible endpoint reachable as
  `ollama-cloud` (e.g. ollama cloud `gemma4:31b-cloud`). Without it the demo
  runs deterministic-only (no LLM calls).

### Build (first time, in order)

```bash
# 1. Install JS dependencies (workspace root)
pnpm install

# 2. Build the native addon WITH the fastembed feature (Rust + BGE-small).
#    The BGE-small model downloads on the first embedding call — one-time.
pnpm --filter @engram/node build:native

# 3. Generate TypeScript contracts + build all packages (contracts, node, demo)
pnpm run contracts:generate
pnpm -r --if-present build
```

### Configure + run

```bash
# 4. LLM creds (optional). Copy the template and fill in real values.
cp demo/backend/.env.example demo/backend/.env
#   ENGRAM_LLM_BASE_URL=https://your-host/v1
#   ENGRAM_LLM_API_KEY=...
#   ENGRAM_LLM_MODEL=gemma4:31b-cloud
# Leave the placeholder to run deterministic-only.

# 5. Start the backend (Hono on :8787 — serves the API + /mcp)
pnpm --filter demo-backend dev

# 6. In another shell, start the frontend (Vite on :5173, proxies API routes
#    to :8787)
pnpm --filter demo-frontend dev
```

Open **http://localhost:5173**. From the dashboard, point **Index** at a local
repo (or docs folder) and let it scan; then open the graph or chat. Re-indexing
reuses durable embeddings (`${ENGRAM_DB}.embeddings.db`).

> The native addon must be rebuilt after any `bindings/node` change — re-run
> `pnpm --filter @engram/node build:native`. `tsx watch` reloads the backend on
> TS edits, but the `.node` is picked up only when the backend restarts.

## Connect via MCP

The backend exposes engram as **JSON-RPC 2.0 over HTTP** at `POST /mcp` — four
tools any MCP-compatible client can call:

| Tool | What it does |
| --- | --- |
| `index_repo` | Scan + ingest a repo (or docs folder) into the knowledge graph. |
| `get_job` | Poll an indexing job's status. |
| `search` | Keyword/entity search over the graph. |
| `agentic_search` | Grounded + agentic Q&A over the graph (LLM, if configured). |

With the backend running on `:8787`, call it directly with curl:

```bash
# List the tools
curl -s -X POST http://localhost:8787/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Index a repo, then ask a question
curl -s -X POST http://localhost:8787/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"index_repo","arguments":{"path":"/abs/path/to/repo"}}}'
```

To use it from a standard MCP client (Claude Desktop / Claude Code / VS Code
Copilot), point an **HTTP JSON-RPC transport** at `http://localhost:8787/mcp`.
For clients that speak stdio, bridge with [`mcp-remote`](https://www.npmjs.com/package/mcp-remote):

```jsonc
// Claude Desktop / Code mcpServers entry (stdio client bridged to the HTTP endpoint)
{
  "mcpServers": {
    "engram": { "command": "npx", "args": ["-y", "mcp-remote", "http://localhost:8787/mcp"] }
  }
}
```

> **Handshake gap (honest):** the demo's `/mcp` implements only `tools/list` and
> `tools/call` over plain request/response JSON-RPC — it does **not** implement
> the full MCP `initialize` / `notifications` / SSE handshake. Direct curl +
> custom JSON-RPC HTTP clients work today; a strict client (Claude Desktop via
> `mcp-remote`) will fail at `initialize` until that handshake is added. Driving
> it from scripts is the supported path for now.

## Contracts

The accepted v1 contract package lives in `contracts/v1/`.

Useful commands:

```bash
python3 tools/scripts/validate_contracts.py
pnpm run contracts:generate
pnpm run contracts:check-generated
.codex/hooks/check-contracts.sh
```

Generated TypeScript types are emitted under `packages/contracts/src/generated/`
and should not be edited by hand. Repository automation scripts live under
`tools/scripts/`.

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

- `docs/architecture/reference.md` - normative architecture and design rules.
- `docs/architecture/overview.md` - current module map.
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
