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
} from "./engram.js";
import { walk, safeReadText, type ScanFile } from "./scan.js";
import { hashContent, isUnchanged, loadManifest, saveManifest } from "./manifest.js";
import { enhanceWithLLM } from "./enhance.js";
import { buildItOrgOntology } from "./itOrgOntology.js";

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

// --- Scale ingestion: point at a folder/repo and index it (RFC 0004 Slice 1) --
// Walks the tree (.gitignore + secret blocklist + size bound + path confinement),
// ingests each text/code file via the existing Rust path, and streams NDJSON
// progress. Re-scans are incremental (content-hash manifest).
app.post("/ingest/scan", async (c) => {
  const { path: root, scope, policy, maxBytes, enhance } = await c.req.json();
  const doEnhance = enhance === true;
  if (!root || typeof root !== "string") return c.json({ error: "path required" }, 400);
  let stat;
  try {
    stat = await fs.stat(root);
  } catch {
    return c.json({ error: `not found: ${root}` }, 400);
  }
  if (!stat.isDirectory()) return c.json({ error: "not a directory" }, 400);

  const rootAbs = path.resolve(root);
  const reqScope = scope ?? SCAN_SCOPE;
  const reqPolicy = policy ?? SCAN_POLICY;

  const files: ScanFile[] = [];
  for await (const f of walk(rootAbs, { maxBytes })) files.push(f);
  const included = files.filter((f) => f.include);
  const total = included.length;

  return stream(c, async (s) => {
    const manifest = await loadManifest();
    let ingested = 0;
    let skipped = 0;
    let unchanged = 0;
    let entities = 0;
    let relationships = 0;
    let llmEntities = 0;
    let llmRelationships = 0;
    let errors = 0;

    for (const f of files) {
      if (!f.include) {
        skipped++;
        await s.write(
          JSON.stringify({ type: "skip", file: f.relPath, reason: f.reason }) + "\n"
        );
      }
    }

    let index = 0;
    for (const f of included) {
      index++;
      try {
        const text = await safeReadText(rootAbs, f.absPath);
        const hash = hashContent(text);
        if (isUnchanged(manifest, f.relPath, hash)) {
          unchanged++;
          await s.write(
            JSON.stringify({ type: "progress", index, total, file: f.relPath, unchanged: true }) +
              "\n"
          );
          continue;
        }
        const result = await getIngestTransport().ingestExtract({
          sourceKind: "filesystem",
          sourceName: `scan:${path.basename(rootAbs)}`,
          scope: reqScope,
          documentKind: f.kind,
          document: { path: f.relPath },
          text,
          policy: reqPolicy,
          actor: SCAN_ACTOR,
        });
        manifest[f.relPath] = hash;
        const e = Array.isArray(result.entities) ? result.entities.length : 0;
        const r = Array.isArray(result.relationships) ? result.relationships.length : 0;
        entities += e;
        relationships += r;

        // Optional LLM enhancement persisted into the same graph (RFC 0004 S2).
        let emittedEntities: unknown[] = result.entities;
        let emittedRelationships: unknown[] = result.relationships;
        let llm: string | { entities: number; relationships: number } | undefined;
        if (doEnhance) {
          const gid =
            result.graph && typeof (result.graph as { id?: unknown }).id === "string"
              ? (result.graph as { id: string }).id
              : "";
          if (gid) {
            const enh = await enhanceWithLLM({
              text,
              kind: f.kind === "code" ? "code" : "text",
              graphId: gid,
              scope: reqScope,
              source: `scan:${path.basename(rootAbs)}`,
              actor: SCAN_ACTOR,
            });
            if (enh.status === "ok") {
              emittedEntities = [...result.entities, ...enh.entities];
              emittedRelationships = [...result.relationships, ...enh.relationships];
              llmEntities += enh.entities.length;
              llmRelationships += enh.relationships.length;
            }
            llm = enh.status;
          }
        }

        ingested++;
        await s.write(
          JSON.stringify({
            type: "progress",
            index,
            total,
            file: f.relPath,
            entities: emittedEntities,
            relationships: emittedRelationships,
            llm,
          }) + "\n"
        );
        // Persist incrementally so an aborted scan does not re-ingest files
        // that already succeeded on the next run.
        await saveManifest(manifest);
      } catch (err) {
        errors++;
        await s.write(
          JSON.stringify({
            type: "error",
            file: f.relPath,
            message: String(err instanceof Error ? err.message : err),
          }) + "\n"
        );
      }
    }

    await saveManifest(manifest);
    await s.write(
      JSON.stringify({
        type: "done",
        summary: {
          scanned: files.length,
          ingested,
          unchanged,
          skipped,
          entities,
          relationships,
          llmEntities,
          llmRelationships,
          errors,
        },
      }) + "\n"
    );
  });
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
