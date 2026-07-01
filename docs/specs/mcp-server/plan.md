# Plan: mcp-server

### T1 — MCP SDK integration + tools/list
- **Tests:** goal-based — curl MCP JSON-RPC tools/list returns 4 tools.
- **Approach:** `pnpm add @modelcontextprotocol/sdk`. New `src/mcp.ts`: create an MCP server, register 4 tools (index_repo, search, agentic_search, get_job) with input schemas. Add `POST /mcp` route in app.ts.

### T2 — Implement index_repo + get_job tools
- **Depends on:** T1
- **Approach:** Wraps `/ingest/jobs` + `/ingest/jobs/:id`. Returns jobId on start, status on poll.

### T3 — Implement search + agentic_search tools
- **Depends on:** T1
- **Approach:** `search` calls `listEntities`/`listRelationships` filtered by query terms. `agentic_search` calls `answerQuestion` from `qa.ts`. Both return structured JSON.

### T4 — Validate with raw MCP JSON-RPC
- **Depends on:** T2, T3
