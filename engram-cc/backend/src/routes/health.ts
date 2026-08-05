//! `/api/health` + `/api/capabilities`. Health constructs the provider (the
//! load-bearing open-store check) and reports scope + the capability report;
//! a failed open returns 503 + an `Error` shape (fail-closed, per `reference.md`).
//! Also reports the MCP server (:8788) status via a quick TCP port check.

import { Hono } from "hono";
import { Socket } from "node:net";

import type { VizConfig } from "../config.ts";
import { getProvider } from "../engram/provider.ts";
import { resolveScope } from "../scope.ts";

/** Quick TCP port check — is anything listening? (Faster than an HTTP handshake.) */
function checkPort(port: number, host = "127.0.0.1", timeoutMs = 1000): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = new Socket();
    socket.setTimeout(timeoutMs);
    socket.once("connect", () => { socket.destroy(); resolve(true); });
    socket.once("timeout", () => { socket.destroy(); resolve(false); });
    socket.once("error", () => { socket.destroy(); resolve(false); });
    socket.connect(port, host);
  });
}

export function healthRoute(cfg: VizConfig): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);

  app.get("/health", async (c) => {
    try {
      const capabilities = await getProvider(cfg).capabilities();
      const mcpPort = Number(process.env.MCP_PORT ?? "8788");
      const mcpUp = await checkPort(mcpPort);
      return c.json({ status: "ok" as const, scope, capabilities, mcp: mcpUp ? "up" : "down" });
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
