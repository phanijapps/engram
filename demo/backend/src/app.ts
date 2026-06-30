import { Hono } from "hono";
import { cors } from "hono/cors";
import {
  getIngestTransport,
  getKnowledgeTransport,
  getRetrievalTransport,
  getTransport,
} from "./engram.js";

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
