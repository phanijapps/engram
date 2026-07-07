// Vector retrieval routes — index a passage, search by query.

import type { Hono } from "hono";
import { getRetrievalTransport } from "../adapters/engram.client.js";

export function registerRetrievalRoutes(app: Hono): void {
  app.post("/retrieval/index", async (c) => {
    const { text } = await c.req.json();
    return c.json(await getRetrievalTransport().index(text));
  });

  app.post("/retrieval/search", async (c) => {
    const { query, topK } = await c.req.json();
    return c.json(await getRetrievalTransport().search(query, topK));
  });
}
