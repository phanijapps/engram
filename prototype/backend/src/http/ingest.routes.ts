// Ingest routes — extraction, background scan jobs.

import path from "node:path";
import type { Hono } from "hono";
import { getIngestTransport, scanManifestPath } from "../adapters/engram.client.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerIngestRoutes(app: Hono): void {
  app.post("/ingest/extract", async (c) => {
    const request = await c.req.json();
    return c.json(await getIngestTransport().ingestExtract(request));
  });

  // Background repo indexing: starts a Rust rayon-parallel scan on a background
  // thread; returns a job id. Progress is polled via GET /ingest/jobs/:id.
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
}
