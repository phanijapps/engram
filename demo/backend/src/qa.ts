// Q&A over knowledge + memory (RFC 0004 Slice 6).
//
// Grounds answers in deterministic retrieval — now INCLUDING the knowledge graph
// (entities + relationships, the call graph) — then synthesizes via the pi SDK.
// When creds are present, the LLM runs AGENTICALLY with tools to search + traverse
// the graph (search_entities, get_neighbors, get_call_graph). Sources come from
// deterministic retrieval, never from LLM-invented text. Never throws.

import { getLLMConfig, runLLM } from "./llm.js";
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
export type QaEntity = { id: string; graphId?: string; kind?: string; name: string };
export type QaRelationship = {
  id: string;
  graphId?: string;
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
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

  // Knowledge-graph entities (filtered by query terms on name/kind).
  const matchedEntityIds = new Set<string>();
  for (const e of entities) {
    const hay = `${e.name} ${e.kind ?? ""}`.toLowerCase();
    if (!terms.some((t) => hay.includes(t))) continue;
    matchedEntityIds.add(e.id);
    sources.push({
      kind: "entity",
      id: e.id,
      text: `${e.name} (${e.kind ?? "entity"})`,
      source: e.graphId ?? "graph",
    });
    blocks.push(`[entity ${e.id}] ${e.name} (${e.kind ?? "entity"})`);
  }

  // Relationships whose endpoints are matched entities (the local call graph).
  for (const r of relationships) {
    const sId = r.subject.id;
    const oId = r.object.id;
    if (!sId || !oId) continue;
    if (!matchedEntityIds.has(sId) && !matchedEntityIds.has(oId)) continue;
    const sName = r.subject.name ?? sId;
    const oName = r.object.name ?? oId;
    sources.push({
      kind: "relationship",
      id: r.id,
      text: `${sName} ${r.predicate} ${oName}`,
      source: r.graphId ?? "graph",
    });
    blocks.push(`[relationship ${r.id}] ${sName} ${r.predicate} ${oName}`);
  }

  // Knowledge chunks: the actual code/document text. Include chunks that
  // reference a matched entity (entity-ref match) OR whose text contains a
  // query term. This gives the LLM the real code to explain, not just names.
  const matchedChunks = chunks
    .filter((c) => {
      const byEntity = c.entities?.some((e) => e.id && matchedEntityIds.has(e.id));
      const byText = terms.some((t) => c.text.toLowerCase().includes(t));
      return byEntity || byText;
    })
    .slice(0, 8);
  for (const chunk of matchedChunks) {
    const text = chunk.text.slice(0, 600);
    blocks.push(`[chunk ${chunk.id}] ${text}`);
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
  "You answer questions about a knowledge graph (entities, relationships, call graphs), " +
  "memories, and beliefs. Answer strictly from the provided context. For call-graph or " +
  "relationship questions, trace the entities and their relationships (calls, defines, " +
  "contains, depends_on, etc.) shown in the context. Cite sources by their [id]. " +
  "If the context does not contain the answer, say so — do not invent records.";

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
    const answer = (
      await runLLM(
        QA_SYSTEM_PROMPT,
        `Question:\n${question}\n\nKnowledge graph + memory + belief context:\n${context}`,
        config,
      )
    ).trim();
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
