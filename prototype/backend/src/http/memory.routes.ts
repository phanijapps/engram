// Memory lifecycle routes — thin JSON transport over the Rust memory service.

import type { Hono } from "hono";
import { getTransport } from "../adapters/engram.client.js";

export function registerMemoryRoutes(app: Hono): void {
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
}
