//! GET /api/stats — high-level repository counts and available sources.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const statsRoute = new Hono();

statsRoute.get("/", (c) => {
  const entities = engine.entities();
  const relationships = engine.relationships();
  const callsEdges = relationships.filter((r) => r.predicate === "calls");
  const sources = engine.sources().map((s) => ({
    id: s.id,
    kind: s.kind ?? "unknown",
    name: s.name ?? "unknown",
  }));
  return c.json({
    nodeCount: entities.length,
    edgeCount: callsEdges.length,
    relationshipCount: relationships.length,
    sources,
  });
});
