//! engram-viz backend — Hono REST server on :3001.
//!
//! A thin transport over the @engram/node codegraph engine. Each route proxies
//! one codegraph operation; there is no business logic here.

import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { compress } from "hono/compress";
import { cors } from "hono/cors";
import { logger } from "hono/logger";

import { engine } from "./lib/engine.ts";
import { graphRoute } from "./routes/graph.ts";
import { insightsRoute } from "./routes/insights.ts";
import { nodeRoute } from "./routes/node.ts";
import { ontologyRoute } from "./routes/ontology.ts";
import { pathRoute } from "./routes/path.ts";
import { scanRoute } from "./routes/scan.ts";
import { searchRoute } from "./routes/search.ts";
import { statsRoute } from "./routes/stats.ts";
import { taxonomyRoute } from "./routes/taxonomy.ts";
import { timelineRoute } from "./routes/timeline.ts";

const app = new Hono();

app.use("*", logger());
app.use(
  "*",
  cors({
    origin: ["http://localhost:5173", "http://127.0.0.1:5173"],
    allowMethods: ["GET", "POST", "OPTIONS"],
  }),
);
// gzip the (large) JSON graph payloads over the wire.
app.use("*", compress());

app.get("/api/health", (c) =>
  c.json({ status: "ok", scope: engine.scope }),
);

// Search index readiness (warmed on boot).
app.get("/api/search/ready", (c) =>
  c.json({
    ready: engine.lexicalSearchReady,
    building: engine.lexicalSearchBuilding,
  }),
);

// Seed code-derived taxonomy + ontology from existing entity data.
app.post("/api/seed-taxonomy-ontology", (c) => {
  try {
    const result = engine.seedTaxonomyOntology();
    return c.json(result);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[seed] error:", msg);
    return c.json({ seeded: false, message: msg }, 500);
  }
});

// Seed configurable enterprise taxonomy + ontology (Business/IT/Support/Customer).
app.post("/api/seed-enterprise-taxonomy", async (c) => {
  try {
    const body = await c.req.text().catch(() => "");
    const result = engine.seedEnterpriseTaxonomyOntology(body || undefined);
    return c.json(result);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[seed-enterprise] error:", msg);
    return c.json({ seeded: false, message: msg }, 500);
  }
});

// On-demand auto-tagging: classify entities into taxonomy concepts by file path.
// Pass custom rules as JSON body { "path/pattern": "conceptId" }. Omit for defaults.
app.post("/api/auto-tag", async (c) => {
  try {
    const body = await c.req.text().catch(() => "");
    const customRules = body ? JSON.parse(body) : undefined;
    const result = engine.autoTag(customRules);
    return c.json(result);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[auto-tag] error:", msg);
    return c.json({ tagged: 0, untagged: 0, byConcept: {}, error: msg }, 500);
  }
});

// Get entities filtered by a taxonomy concept.
app.get("/api/by-concept/:conceptId", (c) => {
  const conceptId = c.req.param("conceptId");
  const entities = engine.entitiesByConcept(conceptId);
  return c.json({ conceptId, count: entities.length, entities: entities.map((e) => ({ id: e.id, name: e.name, kind: e.kind, file: e.file })) });
});

// List indexed repositories (distinct stable_source_key) with entity/relationship
// counts for each. The stable_source_key is the value the `?source=` graph filter
// and entitiesBySource() expect — NOT the KnowledgeSource id.
app.get("/api/sources", (c) => {
  return c.json({ sources: engine.repos() });
});

app.route("/api/stats", statsRoute);
app.route("/api/graph", graphRoute);
app.route("/api/insights", insightsRoute);
app.route("/api/node", nodeRoute);
app.route("/api/search", searchRoute);
app.route("/api/timeline", timelineRoute);
app.route("/api/taxonomy", taxonomyRoute);
app.route("/api/ontology", ontologyRoute);
app.route("/api/path", pathRoute);
app.route("/api/scan", scanRoute);

const port = Number(process.env.PORT ?? "3001");

serve({ fetch: app.fetch, port }, (info) => {
  console.log(`engram-viz backend listening on http://localhost:${info.port}`);
  console.log(`  scope: ${JSON.stringify(engine.scope)}`);
  // NOTE: the BM25 lexical index is built lazily on the first /api/search
  // (ensureLexical), NOT pre-warmed at startup. indexForSearchJson is a
  // synchronous N-API call that blocks the event loop; pre-warming it at
  // startup froze the whole server (incl. graph loads) for the multi-second
  // build over ~18k entities. Offloading it to a worker thread is deferred.
});
