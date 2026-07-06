// Health probes. `/health` is canonical; `/healthz` is an alias for clients
// that assume the `-z` convention.

import type { Hono } from "hono";

export function registerHealthRoutes(app: Hono): void {
  app.get("/health", (c) => c.json({ status: "ok" }));
  app.get("/healthz", (c) => c.json({ status: "ok" }));
}
