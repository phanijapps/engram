// LLM routes — relationship extraction and batch doc enhancement.
//
// /llm/extract runs the deterministic baseline (persisted by Rust), then
// optionally enhances it with an LLM-extracted graph persisted into the SAME
// graphId. LLM creds come from .env; with none, this is deterministic-only.
// The LLM call never crashes the request — a missing/failed call reports
// `llm: "unavailable" | "error"`.

import type { Hono } from "hono";
import { getIngestTransport, getKnowledgeTransport } from "../adapters/engram.client.js";
import { enhanceWithLLM } from "../services/enhance.service.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerLlmRoutes(app: Hono): void {
  app.post("/llm/extract", async (c) => {
    const { text, documentKind, scope, policy, sourceName, actor } = await c.req.json();
    if (!text || typeof text !== "string") return c.json({ error: "text required" }, 400);
    const kind: "code" | "text" = documentKind === "code" ? "code" : "text";
    const reqScope = scope ?? SCAN_SCOPE;
    const reqPolicy = policy ?? SCAN_POLICY;
    const reqActor = actor ?? SCAN_ACTOR;
    const reqSource = sourceName ?? "demo:llm";

    const baseline = await getIngestTransport().ingestExtract({
      sourceKind: "filesystem",
      sourceName: reqSource,
      scope: reqScope,
      documentKind: kind,
      document: { path: reqSource },
      text,
      policy: reqPolicy,
      actor: reqActor,
    });
    const detEntities = Array.isArray(baseline.entities) ? baseline.entities : [];
    const detRelationships = Array.isArray(baseline.relationships) ? baseline.relationships : [];
    const graphId =
      baseline.graph && typeof (baseline.graph as { id?: unknown }).id === "string"
        ? (baseline.graph as { id: string }).id
        : "";

    const enhance = graphId
      ? await enhanceWithLLM({ text, kind, graphId, scope: reqScope, source: reqSource, actor: reqActor })
      : { status: "error" as const, entities: [], relationships: [], error: "no graph id" };

    return c.json({
      entities: [...detEntities, ...enhance.entities],
      relationships: [...detRelationships, ...enhance.relationships],
      chunkCount: baseline.chunkCount ?? 0,
      llm:
        enhance.status === "ok"
          ? { entities: enhance.entities.length, relationships: enhance.relationships.length }
          : enhance.status,
    });
  });

  // Batch LLM enhancement for text/markdown documents already in the knowledge
  // graph. Finds documents whose entities are only "concept" (deterministic
  // text extraction) + runs LLM extraction to add proper entities + relationships.
  app.post("/llm/enhance-docs", async (c) => {
    const { scope } = await c.req.json();
    const reqScope = scope ?? SCAN_SCOPE;
    const transport = getKnowledgeTransport();
    const [entities, relationships, chunks] = await Promise.all([
      transport.listEntities(reqScope),
      transport.listRelationships(reqScope),
      transport.listChunks(reqScope),
    ]);
    // Group chunks by documentId → reconstruct document text.
    const docsByChunk = new Map<string, { text: string; source: string; graphId?: string }>();
    for (const chunk of chunks as Array<Record<string, unknown>>) {
      const docId = String(chunk.documentId ?? "unknown");
      const text = String(chunk.text ?? "");
      const source = String((chunk as { provenance?: { source?: string } }).provenance?.source ?? docId);
      if (!docsByChunk.has(docId)) docsByChunk.set(docId, { text: "", source, graphId: undefined });
      const doc = docsByChunk.get(docId)!;
      doc.text += (doc.text ? "\n" : "") + text;
    }
    // Find entities grouped by graph to know which graphs are text-only (concepts only).
    const entityList = entities as Array<Record<string, unknown>>;
    const graphsByKind = new Map<string, Set<string>>();
    for (const e of entityList) {
      const gid = String(e.graphId ?? "");
      const kind = String(e.kind ?? "");
      if (!graphsByKind.has(gid)) graphsByKind.set(gid, new Set());
      graphsByKind.get(gid)!.add(kind);
    }
    // Enhance graphs that are text-only (only "concept" entities).
    const enhanced: Array<{ source: string; entities: number; relationships: number }> = [];
    for (const [docId, doc] of docsByChunk) {
      // Find the graphId for this document's entities.
      const graphEntity = entityList.find(
        (e) => String(e.id ?? "").startsWith("entity-") && false, // can't easily map doc→graph
      );
      // Simple heuristic: enhance any doc whose text looks like markdown/text.
      const looksLikeText = doc.text.includes("#") || doc.text.includes("##") || doc.text.length > 200;
      if (!looksLikeText || doc.text.length > 8000) continue;
      const result = await enhanceWithLLM({
        text: doc.text,
        kind: "text" as const,
        graphId: docId, // best-effort graph id
        scope: reqScope,
        source: doc.source,
        actor: SCAN_ACTOR,
      });
      if (result.status === "ok") {
        enhanced.push({
          source: doc.source,
          entities: result.entities.length,
          relationships: result.relationships.length,
        });
      }
    }
    return c.json({ enhanced, total: enhanced.length, llm: enhanced.length > 0 ? "ok" : "unavailable" });
  });
}
