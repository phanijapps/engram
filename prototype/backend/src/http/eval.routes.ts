// Evaluation routes — architecture-coverage scoring.

import type { Hono } from "hono";
import { getEvalTransport } from "../adapters/engram.client.js";

export function registerEvalRoutes(app: Hono): void {
  app.post("/eval/architecture-coverage", async (c) => {
    const { cases } = await c.req.json();
    return c.json(await getEvalTransport().architectureCoverage(cases ?? []));
  });
}
