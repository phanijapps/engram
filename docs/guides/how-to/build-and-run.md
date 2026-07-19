# Build and run engram

> Prerequisites, build/test commands, the demo, and MCP server startup — all
> verified against this repo. For connecting an agent over MCP, see
> [Connect via MCP](./connect-via-mcp.md); for what you are building, see the
> [architecture overview](../../architecture/overview.md).

## Prerequisites

- **Rust** toolchain (stable; `rustup show` in the repo pins the channel).
- **Node.js** + **pnpm** (the TypeScript workspace + N-API build + demo).
- **SQLite** system libraries (the SQLite backend links libsqlite3; on Debian/Ubuntu
  `apt install libsqlite3-dev`).
- **A C/C++ build toolchain** (the N-API native module + sqlite-vec compile native code).
- *(optional)* **FastEmbed** model runtime — only for the FastEmbed-backed vector
  provider (`--features fastembed`). The first run downloads the BGE-small model.

## Build

The workspace's `core/integration` crate has **`default = []`** — a bare
`cargo build` builds the engine-neutral core with **no storage backend**. Select
the backend with a Cargo feature:

```bash
# Engine-neutral core only (no storage-backed provider)
cargo build

# SQLite backend (the default storage engine)
cargo build --features sqlite

# SurrealDB backend (embedded SurrealKV)
cargo build --features surreal

# FastEmbed-backed SQLite vector provider (implies sqlite)
cargo build --features fastembed
```

Build the TypeScript side (generates contracts, builds the N-API native module,
then every package):

```bash
pnpm install
pnpm run build           # = build:native (N-API) + every package build
pnpm run contracts:generate   # regenerate TS types from JSON schemas
```

`pnpm run build:native` compiles `@engram/node`'s native module (the Rust
N-API binding). Run it again after any change to `bindings/node/` or the Rust
core it exposes.

## Test

```bash
# Rust workspace (all crates, default features)
cargo test --workspace

# SurrealDB backend tests (feature-gated; #[tokio::test], lazy connection open)
cargo test -p engram-integration --features surreal

# TypeScript: typecheck + tests across all packages
pnpm run typecheck
pnpm run test

# Contract regeneration must be a no-op (generated artifacts are committed)
pnpm run contracts:generate
```

## Validation gates (run before handoff)

These mirror `AGENTS.md` § Validation:

```bash
cargo fmt --all
cargo check --workspace
pnpm run contracts:generate
pnpm run typecheck
pnpm run test
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/check-engine-neutrality.sh   # ADR-0022: no engine types in neutral layers
```

`check-engine-neutrality.sh` denies `Sql*` / `Surreal*` / `surrealdb` imports
and raw SQL in the neutral layers (`core/*` port crates, the `engram-integration`
facade files, `bindings/node`). The engine crates + their bootstrap submodules
are exempt.

## Run the demo

The demo is the `prototype/` workspace (RFC-0004): a Hono backend, a React +
shadcn/ui frontend, and an MCP server.

```bash
pnpm install

# Backend (Hono: ingest, graph, RRF-hybrid Q&A, benchmark)
pnpm --filter prototype-backend dev

# Frontend (Vite + React: dashboard, WebGL graph, chat)
pnpm --filter prototype-frontend dev

# Demo MCP server
pnpm --filter engram-mcp dev
```

Open the frontend URL Vite prints, index a repository via the dashboard, then
run a hybrid Q&A query to see graph + vector retrieval fused.

## Start an MCP server

Both MCP servers are workspace binaries (stdio JSON-RPC 2.0). Build + run:

```bash
# Memory MCP — agent memory operations against a storage path
cargo run -p engram-memory-mcp -- <storage-path>

# Codegraph MCP — index a repo, then query its call graph
cargo run -p engram-codegraph-mcp -- /path/to/store.db
```

For client configs (Claude Desktop, Copilot, Cursor) and the full tool lists,
see [Connect via MCP](./connect-via-mcp.md).

## See also

- [Connect via MCP](./connect-via-mcp.md) — agent connection + client configs.
- [Architecture overview](../../architecture/overview.md) — what you are building.
- [Extend the storage layer](./extend-storage.md) — building a new backend.
- [`README`](../../../README.md) — project overview, use cases, and the doc map.
- `AGENTS.md` § Validation — the canonical gate list.
