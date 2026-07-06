// Registers the engram tool executors on an MCP SDK server.
//
// Used by the /mcp Streamable HTTP route (app.ts) and by the protocol test.
// Keeps all MCP-protocol tool wiring in one place, on top of the shared executors.

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  agenticSearch,
  getJob,
  indexRepo,
  search,
  type ToolDeps,
} from "./executors.js";

function textResult(result: unknown) {
  return { content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }] };
}

export function registerEngramTools(server: McpServer, deps: ToolDeps): void {
  server.registerTool(
    "index_repo",
    {
      description: "Index a code repository into the knowledge graph.",
      inputSchema: {
        path: z.string().describe("Absolute path to the repository"),
        force: z.boolean().optional().describe("Force a full re-index"),
      },
    },
    async (args) => textResult(await indexRepo(deps, args)),
  );

  server.registerTool(
    "get_job",
    {
      description: "Check the status of an indexing job.",
      inputSchema: { jobId: z.string() },
    },
    async (args) => textResult(await getJob(deps, args)),
  );

  server.registerTool(
    "search",
    {
      description: "Search the knowledge graph for entities by name.",
      inputSchema: {
        query: z.string(),
        limit: z.number().optional(),
      },
    },
    async (args) => textResult(await search(deps, args)),
  );

  server.registerTool(
    "agentic_search",
    {
      description: "Ask a question — the knowledge graph is explored to answer it.",
      inputSchema: { question: z.string() },
    },
    async (args) => textResult(await agenticSearch(deps, args)),
  );
}
