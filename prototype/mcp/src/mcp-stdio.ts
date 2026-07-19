#!/usr/bin/env node
// engram-mcp — stdio MCP server that exposes Engram as a local tool backend.
//
// Speaks the Model Context Protocol over stdin/stdout so a coding agent
// (Claude Code, Cursor, etc.) can index repos and retrieve grounded evidence
// from the knowledge graph. agentic_search returns the raw evidence bundle;
// the calling agent synthesizes the answer with its own LLM.
//
// IMPORTANT: stdout is the protocol channel — never write logs to stdout.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { buildToolDeps } from "./deps.js";
import { registerEngramTools } from "./tools.js";
import { SCAN_SCOPE } from "./data/scan-defaults.js";
import { seedOntologies } from "./seed.js";

// Load .env if present; set durable DB path default.
try { process.loadEnvFile(); } catch { /* no .env — fine */ }
process.env.ENGRAM_DB ??= "engram.db";

// Seed Code Repo + IT SDLC ontology/taxonomy on startup (idempotent upserts).
await seedOntologies();

const server = new McpServer({ name: "engram", version: "0.1.0" });
registerEngramTools(server, buildToolDeps(SCAN_SCOPE));

const transport = new StdioServerTransport();
await server.connect(transport);

console.error("[engram-mcp] stdio MCP server ready");
