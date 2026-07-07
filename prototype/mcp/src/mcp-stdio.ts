#!/usr/bin/env node
// engram-mcp — a stdio MCP server that exposes Engram as a local tool backend.
//
// Speak the Model Context Protocol over stdin/stdout so an MCP client (Claude
// Code, Cursor, etc.) can spawn this process as an Engram backend for its
// repository. It reuses the prototype backend's tool registration; the only
// difference from the HTTP /mcp route is the transport.
//
// Run:
//   pnpm --filter engram-mcp start          # built
//   pnpm --filter engram-mcp dev            # via tsx (build prototype-backend first)
//   node dist/mcp-stdio.js
//   engram-mcp                               # after a global/link install
//
// Register with an MCP client by pointing its server command at this binary.
//
// IMPORTANT: stdout is the protocol channel — never write logs to stdout. All
// diagnostics go to stderr.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  applyEnvDefaults,
  buildToolDeps,
  registerEngramTools,
  SCAN_SCOPE,
} from "prototype-backend";

applyEnvDefaults();

const server = new McpServer({ name: "engram", version: "0.1.0" });
registerEngramTools(server, buildToolDeps(SCAN_SCOPE));

const transport = new StdioServerTransport();
await server.connect(transport);

console.error("[engram-mcp] stdio MCP server ready");
