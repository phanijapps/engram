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
    sources.push({
      kind: "entity",
      id: e.id,
      text: `${e.name} (${e.kind ?? "entity"})`,
      source: e.graphId ?? "graph",
    });
    blocks.push(`[entity ${e.id}] ${e.name} (${e.kind ?? "entity"})`);
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
    sources.push({
      kind: "relationship",
      id: r.id,
      text: `${sName} ${r.predicate} ${oName}`,
      source: r.graphId ?? "graph",
    });
    blocks.push(`[relationship ${r.id}] ${sName} ${r.predicate} ${oName}`);
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
  "You are a code intelligence assistant. You answer questions about a knowledge graph " +
  "(entities, relationships, call graphs) and source code chunks.\n\n" +
  "When asked to EXPLAIN or UNDERSTAND something:\n" +
  "1. Read the code from [chunk] sources to explain what it does and how it works.\n" +
  "2. Trace the call graph from [entity] + [relationship] sources (who calls whom, data flow).\n" +
  "3. Describe inputs, transformations, and outputs.\n\n" +
  "When asked for a CALL GRAPH:\n" +
  "1. List the root entity and trace outward via relationships (calls, depends_on, contains).\n" +
  "2. Show each hop as a tree or list.\n\n" +
  "Cite sources by their [id]. Use markdown headings, bullets, and code references. " +
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
