# Plan: TS runtime HTTP-MCP module

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting <!-- Drafting | Executing | Done -->

## Approach

Add the HTTP-MCP module (Module 2) to `@engram/runtime`: an `McpServer` exposing
the facade's query/mutation as MCP tools, mounted on `node:http` via the official
MCP SDK's `NodeStreamableHTTPServerTransport` behind host/origin guards. Five
moves:

1. **(T1)** Add the MCP SDK deps (`@modelcontextprotocol/server` +
   `@modelcontextprotocol/node`) + scaffold `src/mcp/` (`createMcpServer(transport)`
   + a tool-registration helper).
2. **(T2)** Thin tool handlers for `recall`, `write_memory`, `put_entity` backed by
   the facade (input schema → facade call). TDD over a mock transport.
3. **(T3)** `src/mcp/server.ts` (`node:http` + `NodeStreamableHTTPServerTransport`
   + `localhostHostValidation`/`localhostOriginValidation`, loopback) + `bin.ts`
   (`--config/--port`); package.json bin + tsup entry + external.
4. **(T4)** Tests: tool dispatch (mock transport); the guard rejects a bad
   Host/Origin; an in-process MCP client lists tools + calls `recall`.
5. **(T5)** Smoke: real bin + an MCP client (or JSON-RPC POST) against the addon;
   recursive gates.

Riskiest part: the MCP SDK v2 tool-registration API (verify via contract-acquisition
at execute — `server.tool(name, schema, handler)` shape) + the transport's
`handleRequest` wiring. The host/origin guards are mandatory (no unguarded mount).

## Constraints
- RFC-0017 Phase D / Module 2; ADR-0022; Phase A facade + Phase C runtime.
- AGENTS.md — no god-packages; reuse shared helpers; package `index.ts` a facade.
- Framework decision (research 2026-08-01): MCP SDK on `node:http`, no web framework.

## Design (LLD)

### Design decisions
- **Official MCP SDK on `node:http`** (`@modelcontextprotocol/server` +
  `@modelcontextprotocol/node`, v2) — purpose-built, interoperable, zero web-framework
  dep. Traces to: AC1, AC6.
- **Tool surface = facade's current dispatch** (`recall`, `write_memory`, `put_entity`)
  — keeps the slice self-contained (no `@engram/node` widening); `belief_put`/
  `put_relationship` deferred (backlog). Traces to: AC2.
- **Loopback + host/origin guards** — the SDK's `localhostHostValidation` +
  `localhostOriginValidation`; bind `127.0.0.1`. Traces to: AC3.
- **Stateless transport** (`sessionIdGenerator: undefined`) for v1; sessions later
  (ask-first).
- **Thin handlers** — tools delegate to the facade; the server owns no business logic.

### Component / module decomposition
- `src/mcp/tools.ts` — `registerTools(server, transport)` (the 3 tools, thin handlers).
- `src/mcp/server.ts` — `createHttpMcpServer({transport, port})` (`node:http` +
  transport + guards) + `runMcpHttpFromArgs`.
- `src/mcp/bin.ts` — `#!/usr/bin/env node` (`--config/--port`).
- `package.json` — bin `engram-mcp-http`; deps add the MCP packages.
- `tsup.config.ts` — add `src/mcp/bin.ts` entry + `external` the MCP packages.
- `src/index.ts` — export `createHttpMcpServer`.

### Failure, edge cases & resilience
- Bad Host/Origin → the guard refuses the request (non-2xx; exact status pinned
  from the SDK at execute).
- Missing addon → facade load error, bin exits non-zero.
- Tool error → surfaced as an MCP error result (the SDK wraps handler throws).

## Tasks

### T1: MCP SDK deps + scaffold src/mcp/
**Depends on:** none
**Tests:** goal-based — `pnpm install` resolves both `@modelcontextprotocol/server`
+ `@modelcontextprotocol/node`; `src/mcp/tools.ts` exists; the SDK's
tool-registration API is confirmed (contract-acquisition).
**Approach:** `pnpm --filter @engram/runtime add @modelcontextprotocol/server @modelcontextprotocol/node`;
`src/mcp/tools.ts` stub `registerTools`; verify the SDK's tool-registration API
(contract-acquisition — exact `server.tool(...)` signature).
**Done when:** deps install; `src/mcp/` scaffolds; the SDK API is confirmed.

### T2: tool handlers (recall, write_memory, put_entity) — TDD
**Depends on:** T1
**Tests:** mock transport — `recall` tool calls `transport.recall({query, scope})`;
`write_memory` → `transport.write({...})`; `put_entity` → `transport.putEntity({...})`.
**Approach:** `registerTools(server, transport)` with thin handlers mapping the MCP
input schema to the facade call.
**Done when:** tool-dispatch tests green.

### T3: HTTP server (node:http + transport + guards) + bin
**Depends on:** T2
**Tests:** no stub (T4 covers).
**Approach:** `createHttpMcpServer({transport, port=3000})` — `createServer` with
the host/origin guards + `NodeStreamableHTTPServerTransport({sessionIdGenerator:
undefined})` + `server.connect` + `handleRequest`; `bin.ts` (`--config/--port`);
wire package.json bin + tsup entry + external; export from `src/index.ts`.
**Done when:** bin builds; server starts on loopback.

### T4: guard + in-process client tests
**Depends on:** T3
**Tests:** a request with a bad Host (or Origin) is refused (non-2xx); an
in-process MCP client (`@modelcontextprotocol/client`, added as a devDep) calls
`tools/list` → the 3 tools + `tools/call recall` → facade dispatch.
**Done when:** guard + client tests green.

### T5: Smoke + gates
**Depends on:** T4
**Tests:** real bin against the addon → an MCP client lists tools + `recall` returns;
recursive typecheck + `@engram/runtime` test.
**Done when:** smoke recorded; gates green; `git status` clean.

## Rollout
Additive (new module + new deps); nothing removed; reversible. No Rust change.

## Risks
- **MCP SDK v2 API drift** — verify tool-registration + transport signatures at execute (contract-acquisition).
- **`@engram/runtime` shared-file conflict with #88** — execute after #88 merges (rebase onto main).

## Changelog
- 2026-08-01: initial plan (full mode; MCP SDK on node:http per the framework research).
