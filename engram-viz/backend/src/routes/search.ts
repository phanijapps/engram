//! GET /api/search?q=... — BM25 keyword search over indexed symbols.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const searchRoute = new Hono();

searchRoute.get("/", (c) => {
  const query = c.req.query("q") ?? "";
  if (query.trim().length === 0) {
    return c.json({ results: [] });
  }
  const limit = Number(c.req.query("limit") ?? "20");
  const hits = engine.search(query, limit);
  const byId = engine.entityByIdMap();

  const results = hits.map((hit) => {
    const entity = byId.get(hit.id);
    const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
    return {
      id: hit.id,
      name: entity?.name ?? hit.id,
      kind: entity?.kind ?? "unknown",
      file: loc?.path,
      score: hit.score,
    };
  });

  return c.json({ query, results });
});
