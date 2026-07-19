//! GET /api/path?from=X&to=Y — shortest call path between two symbols.
//!
//! Calls the `dependencyPathJson` N-API method. Returns `{ path: [symbol, ...] }`
//! or `{ path: null }` when the two symbols are unreachable.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const pathRoute = new Hono();

pathRoute.get("/", (c) => {
  const from = c.req.query("from") ?? "";
  const to = c.req.query("to") ?? "";
  if (!from || !to) {
    return c.json(
      { error: "query params 'from' and 'to' are both required" },
      400,
    );
  }

  const fromId = engine.resolveEntityId(from);
  const toId = engine.resolveEntityId(to);
  const path = engine.dependencyPath(fromId, toId);

  // Enrich path symbols with entity metadata for the frontend.
  const enriched = (path ?? []).map((key) => {
    const resolved = engine.resolveEntityId(key);
    const entity = engine.entityByKey(resolved);
    const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
    return {
      id: entity?.id ?? resolved,
      name: entity?.name ?? key,
      kind: entity?.kind ?? "unknown",
      file: loc?.path,
    };
  });

  return c.json({ from: fromId, to: toId, path: enriched });
});
