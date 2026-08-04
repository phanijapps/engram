//! engram-viz backend — Hono Backend-for-Frontend reading engram IN-PROCESS via
//! `@engram/node`. The browser never speaks engram-mcp; this BFF is the wire.
//!
//! S1 foundation: health + capabilities. Graph / memory / observatory routes
//! land in later slices (S1 T4+, S2, S3, S4).

import { serve } from "@hono/node-server";
import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";

import { loadConfig } from "./config.ts";
import { graphRoute } from "./routes/graph.ts";
import { healthRoute } from "./routes/health.ts";
import { memoryRoute } from "./routes/memory.ts";

const cfg = loadConfig();
const app = new Hono();

app.use("*", logger());
app.use(
  "*",
  cors({ origin: cfg.corsOrigins, allowMethods: ["GET", "POST", "OPTIONS"] }),
);

app.route("/api", healthRoute(cfg));
app.route("/api", graphRoute(cfg));
app.route("/api", memoryRoute(cfg));

serve({ fetch: app.fetch, port: cfg.port }, (info) => {
  console.log(`engram-viz backend listening on http://localhost:${info.port}`);
  console.log(
    `  storage: ${cfg.storageDir}/${cfg.dbFile}  scope: ${cfg.tenant}/${cfg.workspace}  vectors: ${cfg.enableVector}`,
  );
});
