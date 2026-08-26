//! Ask route — AGENTIC RAG. The LLM orchestrates its own tool calls: it decides
//! what to search (recall / list_memories / graph_overview), reasons over the
//! results, and iterates until it has enough to answer. Not a dumb
//! stuff-context-then-generate pipeline.
//!
//! Tools exposed to the agent: recall, list_memories, graph_overview, write_memory.
//! The BFF dispatches tool calls to the engram facade + returns results to the LLM.

import { Hono } from "hono";
import { createLlmProvider, type LlmAgentMessage, type Tool } from "@engram/runtime";

import type { VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
import { getProvider } from "../engram/provider.ts";

function msg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

interface ToolCallEntry {
  tool: string;
  args: Record<string, unknown>;
  result: unknown;
  error?: string;
}

export function askRoute(cfg: VizConfig): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);

  // Tools the agent can call (TypeBox schemas for pi-mono tool-use).
  const AGENT_TOOLS: Tool[] = [
    {
      name: "recall",
      description: "Hybrid search over the memory + knowledge store (vector + graph + associative). Use for semantic queries.",
      parameters: {
        type: "object",
        properties: { query: { type: "string", description: "The search query" } },
        required: ["query"],
      } as Tool["parameters"],
    },
    {
      name: "list_memories",
      description: "List all memory facts in the scope. Use when you need to browse everything.",
      parameters: { type: "object", properties: {} } as Tool["parameters"],
    },
    {
      name: "graph_overview",
      description: "Get the knowledge-graph community structure (top communities + edges).",
      parameters: {
        type: "object",
        properties: { limit: { type: "number", description: "Max communities (default 50)" } },
      } as Tool["parameters"],
    },
    {
      name: "write_memory",
      description: "Write a new memory observation or fact to the store.",
      parameters: {
        type: "object",
        properties: {
          text: { type: "string", description: "The memory text" },
          kind: { type: "string", description: "observation | fact | procedure | preference | episode" },
        },
        required: ["text"],
      } as Tool["parameters"],
    },
  ];

  // Dispatch a tool call to the engram facade.
  async function executeTool(
    name: string,
    args: Record<string, unknown>,
  ): Promise<unknown> {
    const transport = getProvider(cfg);
    switch (name) {
      case "recall": {
        const result = await transport.recall({
          query: String(args.query ?? ""),
          scope,
          requester: { actor: { id: "engram-cc-agent", kind: "agent", displayName: "engram-cc" } },
        } as never);
        const items = (result as { items?: unknown[] }).items ?? [];
        return { count: items.length, items: items.slice(0, 10) };
      }
      case "list_memories": {
        const page = await transport.listMemoriesPaged(scope, null, 50);
        return {
          count: page.items.length,
          items: page.items.map((m) => ({
            text: (m as { content?: { text?: string } }).content?.text ?? "",
            kind: (m as { kind?: string }).kind ?? "memory",
          })),
        };
      }
      case "graph_overview": {
        const limit = typeof args.limit === "number" ? args.limit : 50;
        const result = await transport.communityOverview(scope, limit);
        return result;
      }
      case "write_memory": {
        // Full WriteMemoryRequest shape (mirrors engram-mcp's write_memory tool):
        // requester + provenance + policy are required by the domain contract.
        const now = new Date().toISOString();
        const result = await transport.write({
          content: { text: String(args.text ?? "") },
          scope,
          kind: String(args.kind ?? "observation"),
          requester: { actor: { id: "engram-cc-agent", kind: "agent", displayName: "engram-cc" } },
          provenance: {
            source: "engram-cc",
            actor: { id: "engram-cc-agent", kind: "agent" },
            observedAt: now,
            evidence: [],
            derivations: [],
            confidence: 1.0,
            method: "ask-write",
          },
          policy: {
            visibility: "workspace",
            retention: "durable",
            sensitivity: "low",
            allowedUses: ["retrieval"],
            deleteMode: "tombstone",
          },
          links: [],
        } as never);
        return { ok: true, id: (result as { record?: { id?: string } }).record?.id };
      }
      default:
        return { error: `unknown tool: ${name}` };
    }
  }

  app.get("/ask", async (c) => {
    const q = c.req.query("q");
    if (!q) return c.json({ error: "query required (?q=question)" }, 422);

    const llm = createLlmProvider();
    const trace: ToolCallEntry[] = [];
    const messages: LlmAgentMessage[] = [
      { role: "user", content: q, timestamp: Date.now() },
    ];

    const MAX_ROUNDS = 5;
    for (let round = 0; round < MAX_ROUNDS; round++) {
      const resp = await llm.completeAgent({
        systemPrompt:
          "You are an AI agent with access to an engram memory store via tools. " +
          "When asked a question, USE THE TOOLS to find the answer — call recall or " +
          "list_memories to retrieve relevant context, then answer based on what you found. " +
          "Be proactive: always call at least one tool before answering. " +
          "Synthesize a clear, detailed answer from the retrieved context.",
        messages,
        tools: AGENT_TOOLS,
      });

      // Push the raw assistant response (content blocks) into the message history.
      // pi-mono needs the original toolCall blocks (with ids) to match toolResults.
      messages.push({ role: "assistant", content: resp.content, timestamp: Date.now() });

      // No tool calls → the agent is done (returned a text answer).
      if (resp.toolCalls.length === 0) {
        return c.json({ answer: resp.text, trace, rounds: round + 1 });
      }

      // Execute each tool call + push results back into the conversation.
      // pi-mono requires toolCallId + toolName on toolResult messages.
      for (const call of resp.toolCalls) {
        const toolCallBlock = resp.content.find(
          (b) => b.type === "toolCall" && b.name === call.name,
        );
        const callId = toolCallBlock?.id ?? call.name;
        try {
          const result = await executeTool(call.name, call.arguments);
          trace.push({ tool: call.name, args: call.arguments, result });
          messages.push({
            role: "toolResult",
            toolCallId: callId,
            toolName: call.name,
            content: [{ type: "text", text: JSON.stringify(result).slice(0, 4000) }],
            isError: false,
            timestamp: Date.now(),
          });
        } catch (err) {
          const error = msg(err);
          trace.push({ tool: call.name, args: call.arguments, result: null, error });
          messages.push({
            role: "toolResult",
            toolCallId: callId,
            toolName: call.name,
            content: [{ type: "text", text: `Error: ${error}` }],
            isError: true,
            timestamp: Date.now(),
          });
        }
      }
    }

    // Max rounds reached — force a final answer without tools.
    const final = await llm.completeAgent({
      systemPrompt: "Answer the user's question based on the tool results you have. Be concise.",
      messages,
    });
    return c.json({ answer: final.text, trace, rounds: MAX_ROUNDS, maxedOut: true });
  });

  return app;
}
