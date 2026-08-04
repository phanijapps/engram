//! `/api/health` + `/api/capabilities`. Health constructs the provider (the
//! load-bearing open-store check) and reports scope + the capability report;
//! a failed open returns 503 + an `Error` shape (fail-closed, per `reference.md`).

import { Hono } from "hono";

import type { VizConfig } from "../config.ts";
import { getProvider } from "../engram/provider.ts";
import { resolveScope } from "../scope.ts";

export function healthRoute(cfg: VizConfig): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);

  app.get("/health", async (c) => {
    try {
      const capabilities = await getProvider(cfg).capabilities();
      return c.json({ status: "ok" as const, scope, capabilities });
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      console.error("[health] provider unavailable:", error);
      return c.json({ status: "degraded" as const, error, degraded: true }, 503);
    }
  });

  app.get("/capabilities", async (c) => {
    try {
      const capabilities = await getProvider(cfg).capabilities();
      return c.json(capabilities);
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      return c.json({ error, degraded: true }, 503);
    }
  });

  return app;
}
