//! engram-viz backend — Hono REST server on :3001.
//!
//! A thin transport over the @engram/node codegraph engine. Each route proxies
//! one codegraph operation; there is no business logic here.

import { serve } from "@hono/node-server";
import { Hono } from "hono";
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
  // Pre-warm the BM25 lexical index in the background so the first user
  // search is not blocked by a multi-minute index build.
  console.log("[engine] pre-warming lexical search index in the background…");
  engine.prewarmLexical();
});
