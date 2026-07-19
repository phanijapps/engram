# Spec: mcp-server (expose backend as MCP HTTP server)

- **Status:** Draft
- **Shape:** service
- **Constrained by:** MCP protocol (Model Context Protocol over HTTP/SSE)
- **Contract:** none

## Objective

The engram backend exposes itself as an **MCP server** (HTTP transport), so any
MCP-compatible client (Claude Desktop, VS Code Copilot, etc.) can: index code
repositories (partial/full/force), search the knowledge graph (functional +
technical), and run agentic_search (LLM-explored multi-hop answers). This makes
engram a **tool** any LLM agent can use, not just a web UI.

## Decision

Add an MCP HTTP endpoint (`/mcp`) to the demo backend using the MCP TypeScript
SDK (`@modelcontextprotocol/sdk`). The server exposes 4 tools:
- `index_repo(path, {force?})` — start a background scan job, return jobId + status.
- `search(query, {top_k?, kind?})` — keyword/entity search over the graph.
- `agentic_search(query)` — run the agentic Q&A loop.
- `get_job(jobId)` — poll a scan job's status.

## Acceptance Criteria

- [ ] `POST /mcp` accepts MCP `tools/list` + returns the 4 tools.
- [ ] `tools/call` with `index_repo` starts a scan → returns jobId.
- [ ] `tools/call` with `search` returns entities + relationships matching the query.
- [ ] `tools/call` with `agentic_search` returns a synthesized answer with citations.
- [ ] Works with any MCP client (tested via curl with raw MCP JSON-RPC).
