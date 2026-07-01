// Q&A over knowledge + memory (RFC 0004 Slice 6).
//
// Grounds answers in deterministic retrieval — now INCLUDING the knowledge graph
// (entities + relationships, the call graph) — then synthesizes via the pi SDK.
// When creds are present, the LLM runs AGENTICALLY with tools to search + traverse
// the graph (search_entities, get_neighbors, get_call_graph). Sources come from
// deterministic retrieval, never from LLM-invented text. Never throws.

import os from "node:os";
import { extractJsonObject, ensureModelsJson, getLLMConfig, runLLM } from "./llm.js";
import { getBeliefTransport, getKnowledgeTransport, getTransport } from "./engram.js";
import type { Scope } from "@engram/contracts";

export type QaSource = {
  kind: "memory" | "belief" | "entity" | "relationship" | "chunk";
  id: string;
  text: string;
  source: string;
};

export type QaResult = {
  answer: string;
  sources: QaSource[];
  llm: "ok" | "unavailable" | "error";
};

export type MemoryItem = {
  targetId?: string;
  content?: { text?: string };
  provenance?: { source?: string };
};
export type QaBelief = {
  id: string;
  subject: { key: string };
  content: string;
  provenance?: { source?: string };
};
export type QaEntity = {
  id: string;
  graphId?: string;
  kind?: string;
  name: string;
  provenance?: { source?: string };
  sourceRefs?: { location?: { path?: string } }[];
};
export type QaRelationship = {
  id: string;
  graphId?: string;
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
  provenance?: { source?: string };
};
export type QaChunk = {
  id: string;
  text: string;
  documentId?: string;
  entities?: { id?: string }[];
};

const QA_REQUESTER = {
  actor: { id: "actor-demo", kind: "agent" as const, displayName: "Demo QA" },
  roles: ["maintainer"],
  permissions: ["memory.retrieve"],
};

const STOP = new Set([
  "the", "a", "an", "is", "are", "was", "were", "of", "to", "in", "on", "for",
  "and", "or", "how", "what", "why", "who", "when", "do", "does", "did", "with",
  "this", "that", "it", "find", "show", "get", "give", "tell",
]);

function queryTerms(question: string): string[] {
  return question
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((term) => term.length > 2 && !STOP.has(term));
}

/**
 * Pure: build a grounded context + sources from memories, beliefs, knowledge-graph
 * entities + relationships. Entities are filtered by query-term match on name/kind;
 * relationships whose endpoints are matched entities are included (the local call
 * graph around the question's subject).
 */
export function buildEvidence(
  question: string,
  memories: MemoryItem[],
  beliefs: QaBelief[],
  entities: QaEntity[],
  relationships: QaRelationship[],
  chunks: QaChunk[],
): { context: string; sources: QaSource[] } {
  const terms = queryTerms(question);
  const sources: QaSource[] = [];
  const blocks: string[] = [];

  // Memories.
  for (const memory of memories) {
    const text = memory.content?.text ?? "";
    if (!text) continue;
    const id = memory.targetId ?? "memory";
    sources.push({ kind: "memory", id, text, source: memory.provenance?.source ?? "memory" });
    blocks.push(`[memory ${id}] ${text}`);
  }

  // Beliefs (filtered by query terms).
  for (const belief of beliefs) {
    const hay = `${belief.subject.key} ${belief.content}`.toLowerCase();
    if (!terms.some((t) => hay.includes(t))) continue;
    sources.push({ kind: "belief", id: belief.id, text: belief.content, source: belief.subject.key });
    blocks.push(`[belief ${belief.id}] ${belief.subject.key}: ${belief.content}`);
  }

  // Knowledge-graph entities: rank by match quality (exact > prefix > substring)
  // and cap at 20 so the context stays focused on large repos.
  const ranked = entities
    .map((e) => {
      const name = e.name.toLowerCase();
      const score = terms.reduce((best, t) => {
        if (name === t) return Math.max(best, 3);
        if (name.startsWith(t) || name.endsWith(t)) return Math.max(best, 2);
        if (name.includes(t)) return Math.max(best, 1);
        return best;
      }, 0);
      return { e, score };
    })
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 20);
  const matchedEntityIds = new Set(ranked.map((x) => x.e.id));
  for (const { e } of ranked) {
    const src = e.provenance?.source ?? e.graphId ?? "graph";
    const filePath = e.sourceRefs?.[0]?.location?.path;
    const fileLabel = filePath ? ` ${filePath}` : "";
    sources.push({
      kind: "entity",
      id: e.id,
      text: `${e.name} (${e.kind ?? "entity"})`,
      source: filePath ? `${src}:${filePath}` : src,
    });
    blocks.push(`[entity] ${e.name} (${e.kind ?? "entity"})${fileLabel} (${src})`);
  }

  // Relationships whose endpoints are matched entities (the local call graph).
  // Cap at 30 to keep the context manageable.
  let relCount = 0;
  for (const r of relationships) {
    if (relCount >= 30) break;
    const sId = r.subject.id;
    const oId = r.object.id;
    if (!sId || !oId) continue;
    if (!matchedEntityIds.has(sId) && !matchedEntityIds.has(oId)) continue;
    const sName = r.subject.name ?? sId;
    const oName = r.object.name ?? oId;
    const src = r.provenance?.source ?? r.graphId ?? "graph";
    sources.push({
      kind: "relationship",
      id: r.id,
      text: `${sName} ${r.predicate} ${oName}`,
      source: src,
    });
    blocks.push(`[${r.predicate}] ${sName} -> ${oName} (${src})`);
    relCount++;
  }

  // Knowledge chunks: the actual code text. Prioritize entity-ref-matched chunks
  // (the code that DEFINES the matched entities) over text-term matches (which
  // could be docs/markdown that merely mention the terms). Fall back to text-term
  // matches only if no entity-ref chunks are found.
  const byEntityRef = chunks.filter((c) =>
    c.entities?.some((e) => e.id && matchedEntityIds.has(e.id)),
  );
  const byTextTerm = chunks.filter((c) =>
    terms.some((t) => c.text.toLowerCase().includes(t)),
  );
  const matchedChunks = (byEntityRef.length > 0 ? byEntityRef : byTextTerm).slice(0, 8);
  for (const chunk of matchedChunks) {
    const text = chunk.text.slice(0, 1200);
    const csrc = chunk.documentId ?? "code";
    blocks.push(`[chunk] ${text}`);
    sources.push({
      kind: "chunk",
      id: chunk.id,
      text: chunk.text.slice(0, 120),
      source: "code",
    });
  }

  return {
    context: blocks.length ? blocks.join("\n") : "(no relevant records found)",
    sources,
  };
}

const QA_SYSTEM_PROMPT =
  "You are a code intelligence assistant. You answer questions about a knowledge graph " +
  "(entities, relationships, call graphs) and source code chunks.\n\n" +
  "When asked to EXPLAIN or UNDERSTAND something:\n" +
  "1. Read the code from [chunk] sources to explain what it does and how it works.\n" +
  "2. Trace the call graph from [entity] + [relationship] sources (who calls whom, data flow).\n" +
  "3. Describe inputs, transformations, and outputs.\n\n" +
  "When asked for a CALL GRAPH:\n" +
  "1. List the root entity and trace outward via relationships (calls, depends_on, contains).\n" +
  "2. Show each hop as a tree or list.\n\n" +
  "Format rules:\n" +
  "- Use plain text arrows (->) NOT LaTeX or math notation.\n" +
  "- Do NOT use $...$, \\rightarrow, \\uparrow, or any LaTeX.\n" +
  "- Cite sources by their source/repo name in parentheses, e.g. (scan:agentzero).\n" +
  "- Use markdown headings, bullets, and code references.\n" +
  "If the context is insufficient, say so — do not invent records.";

/** Fetch knowledge-graph entities + relationships + chunks visible to `scope`. */
async function fetchGraph(scope: unknown): Promise<{ entities: QaEntity[]; relationships: QaRelationship[]; chunks: QaChunk[] }> {
  try {
    const transport = getKnowledgeTransport();
    const [entities, relationships, chunks] = await Promise.all([
      transport.listEntities(scope),
      transport.listRelationships(scope),
      transport.listChunks(scope),
    ]);
    return { entities: entities as QaEntity[], relationships: relationships as QaRelationship[], chunks: chunks as QaChunk[] };
  } catch {
    return { entities: [], relationships: [], chunks: [] };
  }
}

// --- Agentic Q&A: LLM explores the graph step-by-step via tools ---

const AGENTIC_SYSTEM_PROMPT = [
  "You are a code intelligence assistant with tools to explore a knowledge graph.",
  "The graph contains entities (functions, classes, concepts, requirements, value streams)",
  "and relationships (calls, mentions, defines, contains, satisfies, implements).",
  "",
  "Available tools — respond with ONLY a JSON object to call one:",
  '  {"tool":"search_entities","query":"<keyword>"}',
  "    Find up to 15 entities whose name contains the keyword. Returns name, kind, source file.",
  '  {"tool":"get_neighbors","entity":"<exact entity name>"}',
  "    Get up to 20 relationships involving that entity. Returns subject predicate object.",
  '  {"tool":"get_code","entity":"<exact entity name>"}',
  "    Get the source code text of the chunk that defines that entity.",
  "",
  "Workflow: search for relevant entities → trace their neighbors → read code → answer.",
  "When you have enough information, respond with your answer in markdown prose (no JSON).",
  "Format rules: plain text arrows (->), no LaTeX. Cite source file in parentheses.",
  "Max 8 tool calls. If the graph is insufficient, say so.",
].join("\n");

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
    // Turn 0: send the question.
    const timer = setTimeout(stop, 30_000);
    collected = "";
    await session.prompt(
      `${AGENTIC_SYSTEM_PROMPT}\n\nQuestion: ${question}\n\nUse the tools to explore the knowledge graph and answer. Start by searching for entities related to the question.`,
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

/** Answers a question over knowledge graph + memory + beliefs. Never throws. */
export async function answerQuestion(question: string, scope: unknown): Promise<QaResult> {
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
  const { context, sources } = buildEvidence(
    question,
    memories,
    beliefs as QaBelief[],
    graph.entities,
    graph.relationships,
    graph.chunks,
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
    const answer = await runAgenticQA(question, graph, config);
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
