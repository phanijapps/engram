// Q&A over knowledge + memory (RFC 0004 Slice 6).
//
// Grounds answers in deterministic retrieval (memory keyword retrieve + beliefs),
// then — when `.env` LLM creds are present — synthesizes a prose answer via the
// pi SDK (`runLLM`, shared with extraction). Sources come ONLY from the
// deterministic retrieval, never from LLM-invented text: even if the model
// hallucinates a citation, the displayed sources are the real retrieved records.
// Missing creds → a deterministic evidence summary (never an error traceback).

import { getLLMConfig, runLLM } from "./llm.js";
import { getBeliefTransport, getTransport } from "./engram.js";
import type { Scope } from "@engram/contracts";

export type QaSource = {
  kind: "memory" | "belief";
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

const QA_REQUESTER = {
  actor: { id: "actor-demo", kind: "agent" as const, displayName: "Demo QA" },
  roles: ["maintainer"],
  permissions: ["memory.retrieve"],
};

const STOP = new Set([
  "the", "a", "an", "is", "are", "was", "were", "of", "to", "in", "on", "for",
  "and", "or", "how", "what", "why", "who", "when", "do", "does", "did", "with",
  "this", "that", "it",
]);

function queryTerms(question: string): string[] {
  return question
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((term) => term.length > 2 && !STOP.has(term));
}

/**
 * Pure: build a grounded context string + citation sources from retrieved
 * memories + beliefs. Beliefs are filtered by query-term overlap on subject +
 * content; memories are used as the retriever returned them.
 */
export function buildEvidence(
  question: string,
  memories: MemoryItem[],
  beliefs: QaBelief[]
): { context: string; sources: QaSource[] } {
  const terms = queryTerms(question);
  const matchedBeliefs = beliefs.filter((belief) => {
    const haystack = `${belief.subject.key} ${belief.content}`.toLowerCase();
    return terms.some((term) => haystack.includes(term));
  });

  const sources: QaSource[] = [];
  const blocks: string[] = [];

  for (const memory of memories) {
    const text = memory.content?.text ?? "";
    if (!text) continue;
    const id = memory.targetId ?? "memory";
    sources.push({
      kind: "memory",
      id,
      text,
      source: memory.provenance?.source ?? "memory",
    });
    blocks.push(`[memory ${id}] ${text}`);
  }
  for (const belief of matchedBeliefs) {
    sources.push({
      kind: "belief",
      id: belief.id,
      text: belief.content,
      source: belief.subject.key,
    });
    blocks.push(`[belief ${belief.id}] ${belief.subject.key}: ${belief.content}`);
  }

  return {
    context: blocks.length ? blocks.join("\n") : "(no relevant records found)",
    sources,
  };
}

const QA_SYSTEM_PROMPT =
  "You answer the user's question strictly from the provided context (memories + beliefs). " +
  "Cite a source by its [id] in square brackets. If the context does not contain the answer, " +
  "say you don't know. Do not invent sources or records.";

/** Answers a question over the demo's memory + beliefs. Never throws. */
export async function answerQuestion(question: string, scope: unknown): Promise<QaResult> {
  const [memoryResponse, beliefs] = await Promise.all([
    getTransport().retrieve({
      query: question,
      // `scope` is untyped HTTP input; Rust enforces the real Scope shape.
      scope: scope as Scope,
      requester: QA_REQUESTER,
      modes: ["keyword"],
      limit: 8,
      budget: { maxItems: 8, maxTokens: 2000 },
    }),
    getBeliefTransport().listBeliefs(scope),
  ]);
  const memories = ((memoryResponse as { items?: MemoryItem[] }).items ?? []);
  const { context, sources } = buildEvidence(question, memories, beliefs as QaBelief[]);

  const config = getLLMConfig();
  if (!config) {
    return {
      answer: `Evidence-only (no LLM configured): ${sources.length} relevant record(s). Set .env creds (ENGRAM_LLM_*) for a synthesized answer.`,
      sources,
      llm: "unavailable",
    };
  }

  try {
    const answer = (
      await runLLM(QA_SYSTEM_PROMPT, `Question:\n${question}\n\nContext:\n${context}`, config)
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
