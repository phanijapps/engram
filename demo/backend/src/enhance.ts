// LLM graph enhancement (RFC 0004 Slice 2).
//
// Adds LLM-extracted entities/relationships ON TOP of the deterministic
// baseline graph. Both layers are persisted as v1 KnowledgeEntity /
// KnowledgeRelationship objects (the exact shape Rust owns) into the SAME
// graphId the deterministic pass created, so they are queryable together. LLM
// objects carry lower confidence + an `llm_extraction` provenance method so the
// source of every claim stays visible. Output is validated before it reaches
// the graph (see `parseLLMGraph`); a missing/failed LLM call never breaks the
// deterministic result.

import { createHash } from "node:crypto";
import { extractGraph, getLLMConfig } from "./llm.js";
import { getKnowledgeTransport } from "./engram.js";

export type EnhanceStatus = "unavailable" | "ok" | "error";

export type EnhanceResult = {
  status: EnhanceStatus;
  /** LLM entities in v1 shape (already persisted when status === "ok"). */
  entities: unknown[];
  /** LLM relationships in v1 shape (already persisted when status === "ok"). */
  relationships: unknown[];
  error?: string;
};

const LLM_CONFIDENCE = 0.6;

function sha12(input: string): string {
  return createHash("sha256").update(input).digest("hex").slice(0, 12);
}

function nowIso(): string {
  return new Date().toISOString();
}

function llmProvenance(source: string, actor: unknown, confidence: number) {
  const at = nowIso();
  return {
    source,
    actor,
    observedAt: at,
    derivations: [{ kind: "ingestion", createdAt: at }],
    confidence,
    method: "llm_extraction",
  };
}

export type EnhanceOptions = {
  text: string;
  kind: "code" | "text";
  graphId: string;
  scope: unknown;
  source: string;
  actor: unknown;
};

/**
 * Extracts a graph from `text` via the LLM and persists the resulting entities
 * and relationships into graph `graphId`. Returns the LLM objects (for merging
 * into the response) plus a status: `unavailable` (no creds), `ok`, or `error`.
 * Never throws — a failed LLM call yields `error` with empty arrays.
 */
export async function enhanceWithLLM(opts: EnhanceOptions): Promise<EnhanceResult> {
  const config = getLLMConfig();
  if (!config) return { status: "unavailable", entities: [], relationships: [] };

  try {
    const parsed = await extractGraph(opts.text, opts.kind, config);
    const transport = getKnowledgeTransport();

    // Stable client ids so relationships can reference their endpoints. Names
    // are already deduped + trimmed by parseLLMGraph.
    const byName = new Map<string, { id: string; kind: string; name: string }>();
    const entities: unknown[] = [];
    for (const e of parsed.entities) {
      const id = `entity-llm-${sha12(opts.graphId + "|" + e.name + "|" + e.kind)}`;
      byName.set(e.name.toLowerCase(), { id, kind: e.kind, name: e.name });
      const entity = {
        id,
        graphId: opts.graphId,
        kind: e.kind,
        name: e.name,
        scope: opts.scope,
        provenance: llmProvenance(opts.source, opts.actor, LLM_CONFIDENCE),
        createdAt: nowIso(),
      };
      await transport.putEntity(entity);
      entities.push(entity);
    }

    const relationships: unknown[] = [];
    for (const r of parsed.relationships) {
      const subject = byName.get(r.subject.toLowerCase());
      const object = byName.get(r.object.toLowerCase());
      if (!subject || !object) continue;
      const rel = {
        id: `rel-llm-${sha12(opts.graphId + "|" + r.subject + "|" + r.predicate + "|" + r.object)}`,
        graphId: opts.graphId,
        subject: { id: subject.id, kind: subject.kind, name: subject.name },
        predicate: r.predicate,
        object: { id: object.id, kind: object.kind, name: object.name },
        scope: opts.scope,
        confidence: LLM_CONFIDENCE,
        provenance: llmProvenance(opts.source, opts.actor, LLM_CONFIDENCE),
        createdAt: nowIso(),
      };
      await transport.putRelationship(rel);
      relationships.push(rel);
    }

    return { status: "ok", entities, relationships };
  } catch (err) {
    return {
      status: "error",
      entities: [],
      relationships: [],
      error: String(err instanceof Error ? err.message : err),
    };
  }
}
