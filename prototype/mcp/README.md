# engram-mcp

A stdio [Model Context Protocol](https://modelcontextprotocol.io) server that exposes Engram as a local tool backend. An MCP client (Claude Code, Cursor, etc.) spawns this process and gets Engram's memory, knowledge graph, and Q&A capabilities as callable tools.

## What it does

Reuses the prototype backend's tool registration — the same tools exposed by the HTTP `/mcp` route — over stdin/stdout instead of HTTP. The backend stays the single source of truth; this package only changes the transport.

Tools exposed:

- **ingest** — start a background repository scan, poll job status
- **knowledge** — list graph entities visible to a scope
- **qa** — ask a grounded question over memory + knowledge + beliefs

## Prerequisites

- Node.js 22 or later
- The Engram workspace cloned and built:
  ```bash
  git clone https://github.com/phanijapps/engram
  cd engram
  pnpm install
  pnpm run build:native        # compiles the Rust core → N-API binding
  pnpm --filter prototype-backend build   # emits dist/ (engram-mcp imports from here)
  ```

## Build

```bash
pnpm --filter engram-mcp build
```

This emits `dist/mcp-stdio.js` with a `#!/usr/bin/env node` shebang, ready to run as a bin.

## Run

```bash
# built
pnpm --filter engram-mcp start
# or directly
node prototype/mcp/dist/mcp-stdio.js
# or globally linked
pnpm link --global engram-mcp && engram-mcp
```

The server logs `[engram-mcp] stdio MCP server ready` to **stderr** (stdout stays clean for the protocol).

## Register with an MCP client

Point your client's server command at the built binary. For Claude Code, add to your MCP config:

```json
{
  "command": "node",
  "args": ["/abs/path/to/prototype/mcp/dist/mcp-stdio.js"]
}
```

If you linked it globally (`pnpm link --global engram-mcp`):

```json
{
  "command": "engram-mcp"
}
```

## Environment

The server loads `.env` from the working directory on startup. Relevant variables:

| Variable | Default | Purpose |
| --- | --- | --- |
| `ENGRAM_DB` | `demo-engram.db` | SQLite database path (shared with the HTTP backend) |
| `ENGRAM_LLM_BASE_URL` | — | LLM API base URL (enables synthesized Q&A) |
| `ENGRAM_LLM_API_KEY` | — | LLM API key |
| `ENGRAM_LLM_MODEL` | — | LLM model name |

Without LLM credentials, Q&A returns evidence-only results (no synthesis).

## Architecture

```
engram-mcp (this package)
  └─ stdio transport (StdioServerTransport)
       └─ McpServer + registerEngramTools
            └─ prototype-backend (tool deps: transports, Q&A service, scan defaults)
                 └─ @engram/node (N-API binding → Rust core → SQLite)
```

The tool wiring (`registerEngramTools`, `buildToolDeps`) lives in `prototype-backend` and is shared between the HTTP `/mcp` route and this stdio entry. This package only owns the transport and the entry point.

## Publishing (not yet done)

`engram-mcp` is a private workspace package. For `npx -y engram-mcp` to work, it needs to be published to npm with the `@engram/node` native binding prebuilt per platform (the @napi-rs/cli triple system). See the [SurrealDB adapter how-to](../../docs/guides/how-to/build-a-surrealdb-store.md) for the extension contract; the native prebuild pipeline is the prerequisite for standalone distribution.
