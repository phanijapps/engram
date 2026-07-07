// Evidence building for grounded Q&A.
//
// Pure ranking + selection over memories, beliefs, knowledge-graph entities,
// relationships, and code chunks into a grounded context + cited sources. Also
// owns the graph fetch that loads entities/relationships/chunks visible to a
// scope. No LLM, no HTTP — those live in qa.service and the route layer.

import { getKnowledgeTransport } from "../adapters/engram.client.js";
import type {
  MemoryItem,
  QaBelief,
  QaChunk,
  QaEntity,
  QaRelationship,
  QaSource,
} from "./qa.types.js";

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
  semanticChunks: QaChunk[] = [],
  fusedChunkIds: string[] = [],
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

  // Knowledge chunks: the actual code text. Two selection paths:
  //  • Hybrid (RFC-0005 seam): when `fusedChunkIds` is present, chunks are
  //    ordered by RRF-fused graph + vector rank (the true hybrid), with any
  //    semantic chunks the fuser missed appended.
  //  • KG-only fallback (embeddings off/unavailable, or cache cold): entity-ref
  //    chunks first, else text-term matches — the original behavior.
  const byId = new Map(chunks.map((c) => [c.id, c]));
  let matchedChunks: QaChunk[];
  if (fusedChunkIds.length > 0) {
    matchedChunks = fusedChunkIds.map((id) => byId.get(id)).filter((c): c is QaChunk => !!c);
    for (const sc of semanticChunks) {
      if (!matchedChunks.some((c) => c.id === sc.id)) matchedChunks.push(sc);
    }
    matchedChunks = matchedChunks.slice(0, 8);
  } else {
    const byEntityRef = chunks.filter((c) =>
      c.entities?.some((e) => e.id && matchedEntityIds.has(e.id)),
    );
    const priority: QaChunk[] = [...byEntityRef];
    for (const sc of semanticChunks) {
      if (!priority.some((c) => c.id === sc.id)) priority.push(sc);
    }
    const byTextTerm = chunks.filter((c) =>
      terms.some((t) => c.text.toLowerCase().includes(t)),
    );
    matchedChunks = (priority.length > 0 ? priority : byTextTerm).slice(0, 8);
  }
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

/** Fetch knowledge-graph entities + relationships + chunks visible to `scope`. */
export async function fetchGraph(scope: unknown): Promise<{ entities: QaEntity[]; relationships: QaRelationship[]; chunks: QaChunk[] }> {
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
