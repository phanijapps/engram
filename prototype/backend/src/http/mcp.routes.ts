// MCP (Model Context Protocol) route — spec-compliant Streamable HTTP endpoint.
//
// Stateless: each request gets a fresh McpServer + transport (no session store).
// Any HTTP MCP client connects at POST/GET /mcp and gets the full
// initialize/tools handshake.

import type { Hono } from "hono";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import { registerEngramTools } from "../mcp/tools.js";
import { buildToolDeps } from "../mcp/deps.js";
import { SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerMcpRoute(app: Hono): void {
  app.all("/mcp", async (c) => {
    const server = new McpServer({ name: "engram", version: "0.1.0" });
    registerEngramTools(server, buildToolDeps(SCAN_SCOPE));
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: undefined,
    });
    await server.connect(transport);
    return transport.handleRequest(c.req.raw);
  });
}
