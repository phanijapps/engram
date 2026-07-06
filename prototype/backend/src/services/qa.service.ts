// Q&A orchestration over knowledge graph + memory + beliefs.
//
// Grounds answers in deterministic retrieval (memory + beliefs + the knowledge
// graph), then synthesizes via the pi SDK with agentic tool calls over the
// graph. Sources come only from retrieval, never from LLM-invented text. Never
// throws — a missing/failed LLM returns an evidence-only result.

import os from "node:os";
import { extractJsonObject, ensureModelsJson, getLLMConfig } from "../adapters/llm.client.js";
import { getBeliefTransport, getKnowledgeTransport, getTransport } from "../adapters/engram.client.js";
import { lazyEmbeddingsEnabled, semanticChunksFor } from "../adapters/embeddings.client.js";
import { AGENTIC_SYSTEM_PROMPT } from "../prompts/qa.prompts.js";
import { buildEvidence, fetchGraph } from "./evidence.service.js";
import type { MemoryItem, QaBelief, QaChunk, QaEntity, QaRelationship, QaResult } from "./qa.types.js";
import type { Scope } from "@engram/contracts";

const QA_REQUESTER = {
  actor: { id: "actor-demo", kind: "agent" as const, displayName: "Demo QA" },
  roles: ["maintainer"],
  permissions: ["memory.retrieve"],
};

type ToolResult = { tool: string; query?: string; entity?: string };

function executeTool(
  call: ToolResult,
  entities: QaEntity[],
  relationships: QaRelationship[],
  chunks: QaChunk[],
): string {
  switch (call.tool) {
    case "search_entities": {
      const q = (call.query ?? "").toLowerCase().trim();
      if (!q) return "Missing 'query' parameter.";
      const matched = entities
        .filter((e) => e.name?.toLowerCase().includes(q))
        .slice(0, 15)
        .map(
          (e) =>
            `${e.name} (${e.kind ?? "entity"}) — ${e.sourceRefs?.[0]?.location?.path ?? "unknown file"}`,
        );
      return matched.length ? matched.join("\n") : `No entities matching "${q}".`;
    }
    case "get_neighbors": {
      const name = (call.entity ?? "").toLowerCase().trim();
      if (!name) return "Missing 'entity' parameter.";
      const edges = relationships
        .filter(
          (r) =>
            r.subject?.name?.toLowerCase() === name ||
            r.object?.name?.toLowerCase() === name,
        )
        .slice(0, 20)
        .map((r) => `${r.subject?.name ?? "?"} ${r.predicate} ${r.object?.name ?? "?"}`);
      return edges.length ? edges.join("\n") : `No relationships for "${name}".`;
    }
    case "get_code": {
      const name = (call.entity ?? "").toLowerCase().trim();
      if (!name) return "Missing 'entity' parameter.";
      const entity = entities.find((e) => e.name?.toLowerCase() === name);
      if (!entity) return `Entity "${name}" not found.`;
      const chunk = chunks.find((c) => c.entities?.some((e) => e.id === entity.id));
      return chunk ? chunk.text.slice(0, 1500) : `No code chunk for "${name}".`;
    }
    default:
      return `Unknown tool: ${call.tool}`;
  }
}

/**
 * Runs an agentic Q&A loop: the LLM calls graph-search tools to explore the
 * knowledge graph step-by-step, then synthesizes a final answer. Uses the pi
 * SDK session in multi-turn mode — the session persists across tool calls.
 */
async function runAgenticQA(
  question: string,
  graph: { entities: QaEntity[]; relationships: QaRelationship[]; chunks: QaChunk[] },
  config: { baseUrl: string; apiKey: string; model: string },
  context: string,
): Promise<string> {
  const modelsJsonPath = await ensureModelsJson(config);
  const pi = await import("@earendil-works/pi-coding-agent");
  const authStorage = pi.AuthStorage.inMemory();
  const modelRegistry = pi.ModelRegistry.create(authStorage, modelsJsonPath);
  const model = modelRegistry.find("ollama-cloud", config.model);
  if (!model) throw new Error(`pi model not configured: ollama-cloud/${config.model}`);

  const { session } = await pi.createAgentSession({
    model,
    authStorage,
    modelRegistry,
    sessionManager: pi.SessionManager.inMemory(),
    noTools: "all",
    cwd: os.tmpdir(),
  });

  let collected = "";
  let capped = false;
  const stop = () => {
    if (capped) return;
    capped = true;
    void session.abort().catch(() => {});
  };
  const off = session.subscribe((event) => {
    if (capped) return;
    if (
      event.type === "message_update" &&
      event.assistantMessageEvent.type === "text_delta"
    ) {
      collected += event.assistantMessageEvent.delta;
      if (collected.length > 100_000) stop();
    }
  });

  const MAX_TURNS = 9;
  let turnResult = "";

  try {
    // Turn 0: send the question with retrieved evidence as grounding. The
    // grounding carries entity-ref + semantic chunks (when lazy embeddings are
    // on), so semantic retrieval shapes the answer even though the LLM may also
    // explore via tools. Empty grounding (no records) falls back to tools-only.
    const grounding =
      context && context.trim() && context !== "(no relevant records found)"
        ? `\n\nRetrieved evidence (entities, relationships, code chunks — cite these):\n${context}\n`
        : "";
    const timer = setTimeout(stop, 30_000);
    collected = "";
    await session.prompt(
      `${AGENTIC_SYSTEM_PROMPT}${grounding}\n\nQuestion: ${question}\n\nAnswer using the retrieved evidence above and/or the tools. Start by searching for entities related to the question if the evidence is insufficient.`,
    );
    clearTimeout(timer);
    turnResult = collected.trim();
    collected = "";

    // Turns 1-N: parse tool calls, execute, feed back.
    for (let turn = 1; turn < MAX_TURNS; turn++) {
      // Check if this is a final answer (prose, not a JSON tool call).
      let isToolCall = false;
      try {
        const json = JSON.parse(extractJsonObject(turnResult)) as ToolResult;
        if (json.tool) {
          isToolCall = true;
          const toolOutput = executeTool(json, graph.entities, graph.relationships, graph.chunks);
          const timer2 = setTimeout(stop, 30_000);
          collected = "";
          await session.prompt(`Tool result:\n${toolOutput}`);
          clearTimeout(timer2);
          turnResult = collected.trim();
          collected = "";
        }
      } catch {
        // Not JSON → this is the final prose answer.
      }
      if (!isToolCall) break;
    }
  } catch {
    // Abort/timeout/provider error: fall through with whatever was captured.
  } finally {
    off();
    session.dispose();
  }

  if (!turnResult.trim()) {
    throw new Error("LLM returned no content");
  }
  return turnResult;
}

/** Answers a question over knowledge graph + memory + beliefs. Never throws.
 *  `useLazyEmbeddings` overrides the env default so the benchmark can force a
 *  clean KG-only (false) vs hybrid (true) A/B. */
export async function answerQuestion(
  question: string,
  scope: unknown,
  opts?: { useLazyEmbeddings?: boolean },
): Promise<QaResult> {
  const useLazy = opts?.useLazyEmbeddings ?? lazyEmbeddingsEnabled();
  const [memoryResponse, beliefs, graph] = await Promise.all([
    getTransport().retrieve({
      query: question,
      scope: scope as Scope,
      requester: QA_REQUESTER,
      modes: ["keyword"],
      limit: 8,
      budget: { maxItems: 8, maxTokens: 2000 },
    }),
    getBeliefTransport().listBeliefs(scope),
    fetchGraph(scope),
  ]);
  const memories = ((memoryResponse as { items?: MemoryItem[] }).items ?? []);
  // Lazy embeddings: semantic-rerank graph candidates (no-op when off/unavailable).
  let semanticChunks: QaChunk[] = [];
  if (useLazy) {
    try {
      semanticChunks = await semanticChunksFor(question, graph.chunks, 8, true);
    } catch {
      semanticChunks = [];
    }
  }
  // Hybrid retrieval (RFC-0005 seam): RRF-fuse graph chunk order + vector chunk
  // order into one ranking. Fails closed to KG-only (empty list) on any error.
  let fusedChunkIds: string[] = [];
  if (useLazy) {
    try {
      const kt = getKnowledgeTransport();
      const req = { query: question, scope: scope as Scope, requester: QA_REQUESTER, modes: [] };
      const graphCands = (await kt.graphCandidates(req)) as Array<Record<string, unknown>>;
      const graphChunkIds = graphCands
        .filter((r) => String(r.target_type ?? r.targetType) === "chunk")
        .map((r) => String(r.target_id ?? r.targetId));
      const vectorChunkIds = semanticChunks.map((c) => c.id);
      fusedChunkIds = await kt.fuseRrfIds({ lists: [graphChunkIds, vectorChunkIds], limit: 8 });
    } catch {
      fusedChunkIds = [];
    }
  }
  const { context, sources } = buildEvidence(
    question,
    memories,
    beliefs as QaBelief[],
    graph.entities,
    graph.relationships,
    graph.chunks,
    semanticChunks,
    fusedChunkIds,
  );

  const config = getLLMConfig();
  if (!config) {
    return {
      answer: `Evidence-only (no LLM configured): ${sources.length} relevant record(s) from graph + memory + beliefs. Set .env creds (ENGRAM_LLM_*) for a synthesized answer.`,
      sources,
      llm: "unavailable",
    };
  }

  try {
    const answer = await runAgenticQA(question, graph, config, context);
    return { answer, sources, llm: "ok" };
  } catch (err) {
    return {
      answer: `LLM synthesis failed — showing evidence only. (${String(
        err instanceof Error ? err.message : err
      ).slice(0, 160)})`,
      sources,
      llm: "error",
    };
  }
}
