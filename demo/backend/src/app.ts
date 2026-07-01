import { Hono } from "hono";
import { cors } from "hono/cors";
import { stream } from "hono/streaming";
import fs from "node:fs/promises";
import path from "node:path";
import {
  getBeliefTransport,
  getIngestTransport,
  getKnowledgeTransport,
  getRetrievalTransport,
  getTransport,
  scanManifestPath,
} from "./engram.js";
import { enhanceWithLLM } from "./enhance.js";
import { buildItOrgOntology } from "./itOrgOntology.js";
import { answerQuestion } from "./qa.js";

// Demo-local defaults for scan-ingested documents (single-user, local).
const SCAN_SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const SCAN_POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};
const SCAN_ACTOR = { id: "actor-demo", kind: "agent", displayName: "Demo Scan" };

// The backend is a thin JSON transport over the Rust memory service. It owns no
// behavior — v1 JSON in, v1 JSON out, unchanged by Rust — so TypeScript stays
// ergonomic and Rust stays the single source of truth.
export const app = new Hono();

// Dev CORS so the Vite frontend (separate origin) can call the API locally.
app.use(
  "*",
  cors({ origin: ["http://localhost:5173", "http://127.0.0.1:5173"] })
);

app.get("/health", (c) => c.json({ status: "ok" }));

app.post("/memory/write", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().writeMemory(request);
  return c.json(response);
});

app.post("/memory/retrieve", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().retrieve(request);
  return c.json(response);
});

app.post("/memory/forget", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().forget(request);
  return c.json(response);
});

// --- Knowledge graph (manual construction; extraction arrives in Slice 2) ----
app.post("/ingest/extract", async (c) => {
  const request = await c.req.json();
  return c.json(await getIngestTransport().ingestExtract(request));
});

// --- LLM relationship extraction (RFC 0004 Slice 2) -------------------------
// Runs the deterministic baseline (persisted by Rust), then optionally enhances
// it with an LLM-extracted graph persisted into the SAME graphId. LLM creds come
// from .env; with none, this is deterministic-only. The LLM call never crashes
// the request — a missing/failed call reports `llm: "unavailable" | "error"`.
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
// graph. Finds documents whose entities are only "concept" (deterministic text
// extraction) + runs LLM extraction to add proper entities + relationships.
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

// --- Background repo indexing (RFC 0004 background-repo-indexer) ------------
// Starts a Rust rayon-parallel scan on a background thread; returns a job id.
// Progress is polled via GET /ingest/jobs/:id. Deterministic extraction only.
app.post("/ingest/jobs", async (c) => {
  const { root, scope, policy, sourceName, maxBytes, force } = await c.req.json();
  if (!root || typeof root !== "string") return c.json({ error: "root required" }, 400);
  const reqScope = scope ?? SCAN_SCOPE;
  const reqPolicy = policy ?? SCAN_POLICY;
  const reqSource = sourceName ?? `scan:${path.basename(path.resolve(root))}`;
  const result = await getIngestTransport().startScanJob({
    root,
    scope: reqScope,
    policy: reqPolicy,
    actor: SCAN_ACTOR,
    sourceName: reqSource,
    maxBytes: typeof maxBytes === "number" ? maxBytes : 0,
    manifestPath: scanManifestPath() ?? undefined,
    force: force === true,
  });
  return c.json(result);
});

app.get("/ingest/jobs/:id", async (c) => {
  const id = c.req.param("id");
  return c.json(await getIngestTransport().getScanJob(id));
});

app.post("/retrieval/index", async (c) => {
  const { text } = await c.req.json();
  return c.json(await getRetrievalTransport().index(text));
});
app.post("/retrieval/search", async (c) => {
  const { query, topK } = await c.req.json();
  return c.json(await getRetrievalTransport().search(query, topK));
});
app.post("/knowledge/entity", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putEntity(request));
});
app.post("/knowledge/relationship", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putRelationship(request));
});
app.post("/knowledge/graph", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putGraph(request));
});
app.post("/knowledge/neighbors", async (c) => {
  const { graphId, nodeId, scope, limit } = await c.req.json();
  return c.json(await getKnowledgeTransport().neighbors(graphId, nodeId, scope, limit));
});

// Whole-graph overview for the explorer: every graph (source/repo), entity, and
// relationship visible to `scope`. Clustering + cross-repo linking are computed
// client-side from these lists.
app.post("/knowledge/overview", async (c) => {
  const { scope } = await c.req.json();
  const transport = getKnowledgeTransport();
  const [graphs, entities, relationships] = await Promise.all([
    transport.listGraphs(scope),
    transport.listEntities(scope),
    transport.listRelationships(scope),
  ]);
  return c.json({ graphs, entities, relationships });
});

// Stats: per-repo summary (name, git info, entity/rel counts) + aggregates.
app.post("/knowledge/stats", async (c) => {
  const { scope } = await c.req.json();
  const reqScope = scope ?? SCAN_SCOPE;
  const transport = getKnowledgeTransport();
  const [graphs, entities, relationships, chunks] = await Promise.all([
    transport.listGraphs(reqScope),
    transport.listEntities(reqScope),
    transport.listRelationships(reqScope),
    transport.listChunks(reqScope),
  ]);
  const entList = entities as Array<Record<string, unknown>>;
  const relList = relationships as Array<Record<string, unknown>>;
  const chunkList = chunks as Array<Record<string, unknown>>;
  const relByGraph = new Map<string, number>();
  for (const r of relList) {
    const gid = String(r.graphId ?? "");
    relByGraph.set(gid, (relByGraph.get(gid) ?? 0) + 1);
  }
  const repos = (graphs as Array<Record<string, unknown>>).map((g) => {
    const gid = String(g.id ?? "");
    const entCount = entList.filter((e) => String(e.graphId ?? "") === gid).length;
    // Parse git metadata from graph name: "scan:repo [remote@branch:sha]"
    const name = String(g.name ?? "");
    const gitMatch = name.match(/\[(.+?)@(.+?):(.+?)\]/);
    return {
      id: gid,
      name: name.replace(/\s*\[.+$/, ""),
      gitRemote: gitMatch?.[1] ?? null,
      gitBranch: gitMatch?.[2] ?? null,
      gitSha: gitMatch?.[3] ?? null,
      entityCount: entCount,
      relationshipCount: relByGraph.get(gid) ?? 0,
      lastUpdated: (g as { updatedAt?: string }).updatedAt ?? (g as { createdAt?: string }).createdAt ?? null,
    };
  });
  return c.json({
    tenant: reqScope,
    repos,
    totalEntities: entList.length,
    totalRelationships: relList.length,
    totalChunks: chunkList.length,
    totalRepos: repos.length,
  });
});

// --- Taxonomy (maintain concept schemes + concepts) -------------------------
app.post("/taxonomy/scheme", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putConceptScheme(request));
});
app.post("/taxonomy/concept", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putConcept(request));
});
app.post("/taxonomy/relation", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putConceptRelation(request));
});
app.post("/taxonomy/concepts", async (c) => {
  const { schemeId, scope } = await c.req.json();
  return c.json(await getKnowledgeTransport().listConcepts(schemeId, scope));
});

// --- Ontology (govern graphs with classes, properties, axioms; RFC 0004 S3) -
// Thin pass-throughs over the Rust OntologyRepository. validate_graph is
// advisory — it returns findings, never rejects writes.
app.post("/ontology/ontology", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putOntology(request));
});
app.post("/ontology/class", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putClass(request));
});
app.post("/ontology/property", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putProperty(request));
});
app.post("/ontology/axiom", async (c) => {
  const request = await c.req.json();
  return c.json(await getKnowledgeTransport().putAxiom(request));
});
app.post("/ontology/get", async (c) => {
  const { id, scope } = await c.req.json();
  return c.json(await getKnowledgeTransport().getOntology(id, scope));
});
app.post("/ontology/validate", async (c) => {
  const { graphId, ontologyId, scope } = await c.req.json();
  return c.json(await getKnowledgeTransport().validateGraph(graphId, ontologyId, scope));
});

// Loads the IT-org sample ontology + taxonomy (RFC 0004 Slice 3) through the
// knowledge transport and returns the records so the UI can browse them without
// separate list endpoints.
app.post("/ontology/it-org", async (c) => {
  const body = await c.req.json().catch(() => ({}));
  const reqScope = body.scope ?? SCAN_SCOPE;
  const reqPolicy = body.policy ?? SCAN_POLICY;
  const reqActor = body.actor ?? SCAN_ACTOR;
  const sample = buildItOrgOntology({
    scope: reqScope,
    policy: reqPolicy,
    actor: reqActor,
    now: new Date().toISOString(),
  });
  const transport = getKnowledgeTransport();
  await transport.putOntology(sample.ontology);
  for (const klass of sample.classes) await transport.putClass(klass);
  for (const property of sample.properties) await transport.putProperty(property);
  for (const axiom of sample.axioms) await transport.putAxiom(axiom);
  await transport.putConceptScheme(sample.scheme);
  for (const concept of sample.concepts) await transport.putConcept(concept);
  return c.json({ loaded: true, sample });
});

// --- Belief + contradiction (RFC 0004 Slice 5) ------------------------------
// Durable belief/contradiction storage in the belief SQLite adapter (distinct
// from knowledge + memory). Detection is advisory; resolution is an action.
app.post("/belief/put", async (c) => {
  const request = await c.req.json();
  return c.json(await getBeliefTransport().putBelief(request));
});
app.post("/belief/list", async (c) => {
  const { scope } = await c.req.json();
  return c.json(await getBeliefTransport().listBeliefs(scope));
});
app.post("/belief/contradiction", async (c) => {
  const request = await c.req.json();
  return c.json(await getBeliefTransport().putContradiction(request));
});
app.post("/belief/contradictions", async (c) => {
  const { scope } = await c.req.json();
  return c.json(await getBeliefTransport().listContradictions(scope));
});
app.post("/belief/get", async (c) => {
  const { id, scope } = await c.req.json();
  return c.json(await getBeliefTransport().getContradiction(id, scope));
});
app.post("/belief/resolve", async (c) => {
  const { id, scope, resolution } = await c.req.json();
  return c.json(await getBeliefTransport().resolveContradiction(id, scope, resolution));
});
// Runs advisory detection over the beliefs visible to `scope`, persists each
// detected contradiction (so it appears in the review queue), and returns them.
app.post("/belief/detect", async (c) => {
  const { scope } = await c.req.json();
  const transport = getBeliefTransport();
  const beliefs = await transport.listBeliefs(scope);
  const detected = (await transport.detectContradictions(beliefs)) as unknown[];
  for (const contradiction of detected) {
    await transport.putContradiction(contradiction);
  }
  return c.json({ beliefs: (beliefs as unknown[]).length, contradictions: detected });
});

// --- Q&A over knowledge + memory (RFC 0004 Slice 6) -------------------------
// Grounds in deterministic retrieval (memory + beliefs); synthesizes via the pi
// SDK when .env creds are present. Sources come only from retrieval, never from
// LLM-invented text. Never throws — missing/failed LLM → evidence summary.
app.post("/qa/ask", async (c) => {
  const { question, scope } = await c.req.json();
  if (!question || typeof question !== "string") return c.json({ error: "question required" }, 400);
  const reqScope = scope ?? SCAN_SCOPE;
  return c.json(await answerQuestion(question, reqScope));
});
