import { z } from "zod";

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
        kind: z
          .enum([
            "observation",
            "fact",
            "preference",
            "episode",
            "artifact",
            "relationship",
            "procedure"
          ])
          .optional()
      })
    },
    async ({ text, scope, kind }) => {
      const result = await transport.write(
        buildWriteMemoryRequest({
          text,
          scope: buildScope(scope),
          ...(kind !== undefined ? { kind } : {})
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

  server.registerTool(
    "put_relationship",
    {
      description: "Upsert a knowledge relationship (full KnowledgeRelationship JSON object).",
      inputSchema: z.object({ relationship: z.record(z.string(), z.unknown()) })
    },
    async ({ relationship }) => {
      const result = await transport.putRelationship(relationship);
      return textResult(result);
    }
  );

  server.registerTool(
    "belief_put",
    {
      description: "Upsert a belief (full Belief JSON object).",
      inputSchema: z.object({ belief: z.record(z.string(), z.unknown()) })
    },
    async ({ belief }) => {
      const result = await transport.beliefPut(belief);
      return textResult(result);
    }
  );

  server.registerTool(
    "forget",
    {
      description: "Forget (delete/redact/tombstone/archive) a memory — full ForgetRequest JSON.",
      inputSchema: z.object({ request: z.record(z.string(), z.unknown()) })
    },
    async ({ request }) => {
      const result = await transport.forget(request);
      return textResult(result);
    }
  );

  // ---- Friendly READ tools (P3.6) — the consumer surface for extracting info.

  server.registerTool(
    "graph_overview",
    {
      description:
        "Community-level graph overview: the top-N Louvain communities + inter-community meta-edges. Good first look at the graph shape. Returns { communities: [{label, memberCount}], edges: [{sourceLabel, targetLabel, weight}], totalCommunities }.",
      inputSchema: z.object({
        scope: scopeSchema,
        limit: z.number().int().min(1).max(2000).optional(),
      }),
    },
    async ({ scope, limit }) => {
      const result = await transport.communityOverview(buildScope(scope), limit);
      return textResult(result);
    },
  );

  server.registerTool(
    "list_memories",
    {
      description:
        "List memory facts (observations) in scope, keyset-paged. `after` is the opaque nextCursor from a prior page. Returns { items: [MemoryRecord], nextCursor }.",
      inputSchema: z.object({
        scope: scopeSchema,
        after: z.string().nullable().optional(),
        limit: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, after, limit }) => {
      const result = await transport.listMemoriesPaged(
        buildScope(scope),
        after ?? null,
        limit ?? 50,
      );
      return textResult(result);
    },
  );

  // ---- Maintenance tools (pi-mono LLM) — the agent surface for belief synthesis
  // + contradiction detection, plus the belief/contradiction read tools. The LLM
  // modules are imported LAZILY inside the handlers so listTools / recall never
  // load pi-mono (the server stays light; the LLM only loads on a maintenance
  // call). The LLM runs TS-side (Rust stays LLM-free).

  server.registerTool(
    "belief_list",
    {
      description:
        "List beliefs in scope (Rust-backed; all statuses). Returns [Belief, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.listBeliefs(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "contradiction_list",
    {
      description:
        "List contradiction review records in scope (Rust-backed; all statuses). Returns [Contradiction, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.listContradictions(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "maintenance_run",
    {
      description:
        "Run a maintenance op over scope. PRIVACY: reflect-llm / contradict-llm route the scope's memories/beliefs to a third-party LLM (Anthropic by default; set PI_PROVIDER=ollama for local) and incur token cost — only call with consent. Ops: reflect-llm (synthesize beliefs), contradict-llm (detect contradictions), consolidate (deterministic, no LLM). Default reflect-llm.",
      inputSchema: z.object({
        scope: scopeSchema,
        op: z.enum(["reflect-llm", "contradict-llm", "consolidate"]).optional(),
      }),
    },
    async ({ scope, op }) => {
      const theScope = buildScope(scope);
      const theOp = op ?? "reflect-llm";
      if (theOp === "consolidate") {
        return textResult(await transport.consolidate({ scope: theScope }));
      }
      const { createLlmProvider } = await import("../maintenance/llm.js");
      const llm = createLlmProvider();
      if (theOp === "contradict-llm") {
        const { contradictLlm } = await import("../maintenance/contradict.js");
        return textResult(await contradictLlm({ transport, scope: theScope, llm }));
      }
      const { reflectLlm } = await import("../maintenance/reflect.js");
      return textResult(await reflectLlm({ transport, scope: theScope, llm }));
    },
  );

  server.registerTool(
    "contradiction_detect",
    {
      description:
        "Detect semantic contradictions across a scope's beliefs via pi-mono. LLM-only in this slice (the rule-based same-subject pre-filter is deferred — see backlog). Routes belief text to a third-party LLM (Anthropic default; PI_PROVIDER=ollama for local). Returns { beliefsRead, contradictionsWritten, skipped } (emitted records; the store dedupes by canonical pair key).",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const { createLlmProvider } = await import("../maintenance/llm.js");
      const { contradictLlm } = await import("../maintenance/contradict.js");
      return textResult(
        await contradictLlm({
          transport,
          scope: buildScope(scope),
          llm: createLlmProvider(),
        }),
      );
    },
  );
}
