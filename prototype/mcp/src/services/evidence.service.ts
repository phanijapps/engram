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
    .filter((term) => term.length >= 2 && !STOP.has(term));
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

  // Memories — cap source text to avoid bloating the response.
  for (const memory of memories) {
    const text = memory.content?.text ?? "";
    if (!text) continue;
    const id = memory.targetId ?? "memory";
    const snippet = text.slice(0, 300);
    sources.push({ kind: "memory", id, text: snippet, source: memory.provenance?.source ?? "memory" });
    blocks.push(`[memory ${id}] ${snippet}`);
  }

  // Beliefs (filtered by query terms).
  for (const belief of beliefs) {
    const hay = `${belief.subject.key} ${belief.content}`.toLowerCase();
    if (!terms.some((t) => hay.includes(t))) continue;
    sources.push({ kind: "belief", id: belief.id, text: belief.content, source: belief.subject.key });
    blocks.push(`[belief ${belief.id}] ${belief.subject.key}: ${belief.content}`);
  }

  // Knowledge-graph entities: rank by match quality across name + file path +
  // source repo. Deduplicate by (name, repo) so same-named entities from
  // different repos are both kept. Boost executable kinds over concepts.
  const KIND_BOOST = new Set(["function", "class", "interface", "struct", "trait", "method", "endpoint"]);
  const seen = new Map<string, { e: QaEntity; score: number }>();
  for (const e of entities) {
    const name = e.name.toLowerCase();
    const filePath = (e.sourceRefs?.[0]?.location?.path ?? "").toLowerCase();
    // First word of provenance.source identifies the repo (e.g. "scan:ocean-hospitality-ui").
    const repoKey = (e.provenance?.source ?? "").split(" ")[0].toLowerCase();
    const lexical = terms.reduce((best, t) => {
      if (name === t) return Math.max(best, 3);
      if (name.startsWith(t) || name.endsWith(t)) return Math.max(best, 2);
      if (name.includes(t)) return Math.max(best, 1);
      // Match against file path or repo name gives a lower-priority signal.
      if (filePath.includes(t) || repoKey.includes(t)) return Math.max(best, 0.5);
      return best;
    }, 0);
    if (lexical === 0) continue;
    const score = lexical + (KIND_BOOST.has((e.kind ?? "").toLowerCase()) ? 1 : 0);
    // Deduplicate within the same repo only — cross-repo duplicates are intentional.
    const dedupeKey = `${name}::${repoKey}`;
    const prev = seen.get(dedupeKey);
    if (!prev || score > prev.score) seen.set(dedupeKey, { e, score });
  }
  const ranked = [...seen.values()]
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
    // Entity-ref chunks (structural — linked to matched graph nodes).
    const byEntityRef = chunks.filter((c) =>
      c.entities?.some((e) => e.id && matchedEntityIds.has(e.id)),
    );
    // Text-term chunks (lexical — raw code/JSX/comment text contains query terms).
    // Always run this path, not only as a fallback, so string literals and JSX
    // text surface even when entity-ref chunks exist from other repos.
    const byTextTerm = chunks.filter((c) =>
      terms.some((t) => c.text.toLowerCase().includes(t)),
    );
    // Merge: entity-ref first (higher signal), then text-term not already included,
    // then semantic (embedding-reranked) not already included.
    const merged: QaChunk[] = [...byEntityRef];
    for (const c of byTextTerm) {
      if (!merged.some((m) => m.id === c.id)) merged.push(c);
    }
    for (const sc of semanticChunks) {
      if (!merged.some((m) => m.id === sc.id)) merged.push(sc);
    }
    matchedChunks = merged.slice(0, 12);
  }
  for (const chunk of matchedChunks) {
    const text = chunk.text.slice(0, 500);
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

/** Normalize a user-supplied source name to a stable_source_key prefix. */
function normalizeSourceKey(source: string): string {
  return source.startsWith("scan:") ? source : `scan:${source}`;
}

/**
 * Fetch knowledge-graph entities + relationships + chunks visible to `scope`.
 * When `source` is provided (e.g. "ocean-hospitality-ui"), only entities and
 * relationships from that source graph are fetched — reducing the search space
 * from the full multi-repo DB to a single repo.
 */
export async function fetchGraph(
  scope: unknown,
  source?: string,
): Promise<{ entities: QaEntity[]; relationships: QaRelationship[]; chunks: QaChunk[] }> {
  const transport = getKnowledgeTransport();
  if (source) {
    const key = normalizeSourceKey(source);
    const [entities, relationships, chunks] = await Promise.all([
      transport.listEntitiesBySource(key, scope),
      transport.listRelationshipsBySource(key, scope),
      transport.listChunks(scope),
    ]);
    return { entities: entities as QaEntity[], relationships: relationships as QaRelationship[], chunks: chunks as QaChunk[] };
  }
  const [entities, relationships, chunks] = await Promise.all([
    transport.listEntities(scope),
    transport.listRelationships(scope),
    transport.listChunks(scope),
  ]);
  return { entities: entities as QaEntity[], relationships: relationships as QaRelationship[], chunks: chunks as QaChunk[] };
}
