// Consolidation routes — plan memory consolidation batches.

import type { Hono } from "hono";
import { getConsolidationTransport } from "../adapters/engram.client.js";

export function registerConsolidationRoutes(app: Hono): void {
  app.post("/consolidation/plan", async (c) => {
    const request = await c.req.json();
    return c.json(await getConsolidationTransport().plan(request));
  });
}
