import { z } from "zod";

import type { MemoryKind } from "@engram/contracts";
import type { NativeProviderTransport } from "@engram/node";
import type { McpServer } from "@modelcontextprotocol/server";

import { buildScope } from "../shared/config.js";
import { buildRetrievalRequest, buildWriteMemoryRequest } from "./requests.js";

const scopeSchema = z.object({
  tenant: z.string(),
  workspace: z.string().optional()
});

const textResult = (payload: unknown) => ({
  content: [{ type: "text" as const, text: JSON.stringify(payload) }]
});

/** Registers the Module-2 tools (query + light mutation) on the MCP server,
 *  each backed by the held-provider facade. Handlers are thin translators:
 *  simple MCP input → full domain request (default requester/policy) → facade. */
export function registerTools(
  server: McpServer,
  transport: NativeProviderTransport
): void {
  server.registerTool(
    "recall",
    {
      description:
        "Unified recall (fused across lanes) over the engram knowledge + memory layer.",
      inputSchema: z.object({ query: z.string(), scope: scopeSchema })
    },
    async ({ query, scope }) => {
      const result = await transport.recall(
        buildRetrievalRequest(query, buildScope(scope))
      );
      return textResult(result);
    }
  );

  server.registerTool(
    "write_memory",
    {
      description: "Write a memory (observation by default) into the project scope.",
      inputSchema: z.object({
        text: z.string(),
        scope: scopeSchema,
        kind: z.string().optional()
      })
    },
    async ({ text, scope, kind }) => {
      const result = await transport.write(
        buildWriteMemoryRequest({
          text,
          scope: buildScope(scope),
          ...(kind !== undefined ? { kind: kind as MemoryKind } : {})
        })
      );
      return textResult(result);
    }
  );

  server.registerTool(
    "put_entity",
    {
      description: "Upsert a knowledge entity (full KnowledgeEntity JSON object).",
      inputSchema: z.object({ entity: z.record(z.string(), z.unknown()) })
    },
    async ({ entity }) => {
      const result = await transport.putEntity(entity);
      return textResult(result);
    }
  );
}
