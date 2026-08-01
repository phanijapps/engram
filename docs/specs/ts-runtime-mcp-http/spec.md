# Spec: TS runtime HTTP-MCP module

- **Status:** Draft <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0017 (Phase D / Module 2, Accepted), ADR-0022, the Phase A facade + Phase C `@engram/runtime` (shipped); the framework decision (research, 2026-08-01): the official `@modelcontextprotocol/sdk` on `node:http`
- **Brief:** none
- **Contract:** none — exposes the Phase A facade's query/mutation as MCP tools; no new Rust surface
- **Shape:** service

> **Spec contract:** this document defines what "done" means.

## Objective

The second deployable TS module and RFC-0017's Module 2 — the **agent-facing
remote query/mutation surface**: an `engram-mcp-http` server in `@engram/runtime`
that exposes the Phase A facade's query + light mutation as **MCP tools** over
the Streamable HTTP transport, using the official `@modelcontextprotocol/sdk` (v2:
`@modelcontextprotocol/server` + `@modelcontextprotocol/node`) mounted on
`node:http`. This is the multi-agent/remote counterpart to the stdio `engram-mcp`
Rust binary (which stays the single-agent default). A real, interoperable MCP
server — Claude/Cursor/etc. connect directly over HTTP.

## Framework decision (research 2026-08-01)

The official MCP TS SDK ships a `node:http` transport
(`NodeStreamableHTTPServerTransport`), so **no web framework is needed** (Hono /
Fastify / Express would be a redundant dep for one `/mcp` route). The SDK owns the
MCP protocol (JSON-RPC, tools/list, tools/call, streaming); we register tools
backed by the facade and mount the transport on `node:http` behind the SDK's
`localhostHostValidation` + `localhostOriginValidation` guards.

## Boundaries

### Always do
- Route every tool through `createNativeProviderTransport` (the held provider); reuse `@engram/runtime` shared helpers.
- Mount on `node:http` (no web framework); bind loopback (`127.0.0.1`) + apply the SDK's host/origin guards (never an unguarded mount).
- Keep tool handlers thin — delegate to the facade; no business logic in the server.
- The tool surface is **Module 2 scope** — query + light mutation backed by the
  facade's *current* dispatch (`recall`, `write_memory`, `put_entity`).
  `belief_put`/`put_relationship` are deferred (the Phase A facade doesn't
  dispatch them yet — a small widening, tracked in backlog). Scan/consolidate are
  Module 1/3 (excluded).

### Ask first
- Adding another runtime dependency beyond the MCP SDK; widening the tool surface to scan/consolidate; enabling stateful sessions.

### Never do
- Bypass the facade; put the HTTP server or MCP protocol in Rust; re-implement the MCP protocol (use the SDK); expose an unguarded/remote-by-default mount; make `@engram/runtime` a god-package (module isolated in `src/mcp/`).

## Testing Strategy
- **TDD** — the tool handlers (argv/config → facade dispatch; input-schema → facade call shape) over a mock transport.
- **Goal-based** — recursive typecheck + `@engram/runtime` build; the `engram-mcp-http` bin resolves.
- **Manual / integration QA** — an in-process test (or real bin + an MCP client / JSON-RPC POST) lists tools + calls `recall`/`write_memory` against the real addon.

## Acceptance Criteria
- [ ] An `engram-mcp-http` bin exists in `@engram/runtime`, serving MCP over HTTP (loopback) via `@modelcontextprotocol/node`'s transport on `node:http`.
- [ ] An MCP client can `tools/list` and `tools/call` the Module-2 tools (`recall`, `write_memory`, `put_entity`), backed by the facade (integration test / smoke). (`belief_put`/`put_relationship` deferred — facade widening.)
- [ ] The server applies `localhostHostValidation` + `localhostOriginValidation` and rejects non-local requests (TDD: a request with a bad Host/Origin is refused).
- [ ] Tool handlers are thin (delegate to the facade; verified by the mock-transport TDD).
- [ ] `pnpm run typecheck` (recursive) + `pnpm --filter @engram/runtime test` green.
- [ ] `@engram/runtime` `package.json` declares the new dep (`@modelcontextprotocol/server` + `@modelcontextprotocol/node`) and the `engram-mcp-http` bin; `tsup.config.ts` external includes the MCP packages.

## Assumptions
- Technical: the official MCP SDK v2 exposes `McpServer` (`@modelcontextprotocol/server`) + `NodeStreamableHTTPServerTransport` + `localhostHostValidation`/`localhostOriginValidation` (`@modelcontextprotocol/node`), mountable on `node:http` (verified via the SDK docs / context7, 2026-08-01). The exact tool-registration API is verified at execute time (contract-acquisition).
- Technical: the facade exposes `recall`, `write`, `putEntity`, `consolidate` (Phase A, on `main`); `putRelationship`/`beliefPut` proxies exist on the binding (verify which the facade currently dispatches + widen if needed).
- Process: full-mode work-loop (new dependency + new module = structural change); RFC-0017 Phase D governs. Execution rebase after PR #88 (maintenance) merges to avoid `@engram/runtime` shared-file (`index.ts`/`package.json`/`tsup.config.ts`) conflicts.
