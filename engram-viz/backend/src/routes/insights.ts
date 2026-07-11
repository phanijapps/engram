//! GET /api/insights — dead code, central symbols, and bridge symbols.
//!
//! Analytics keys resolve to entity metadata (name/kind/file) via the entity
//! lookup. Each item carries the entity id so the frontend can highlight the
//! matching graph node.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const insightsRoute = new Hono();

interface InsightItem {
  id: string;
  name: string;
  kind: string;
  file?: string;
  score?: number;
  category?: string;
}

/** Classify zero-caller symbols to filter dead-code false positives. */
function deadCodeCategory(name: string): string {
  if (["main", "run", "start", "handler", "__main__"].includes(name)) {
    return "entry_point";
  }
  if (
    name.startsWith("test_") ||
    name.startsWith("tests") ||
    name.startsWith("it_") ||
    name.startsWith("should_")
  ) {
    return "test";
  }
  return "candidate";
}

function resolveItem(key: string): InsightItem {
  const entity = engine.entityByKey(key);
  const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
  return {
    // Resolve to a real entity id when possible so the frontend can fetch
    // node detail and focus a valid graph node.
    id: entity?.id ?? key,
    name: entity?.name ?? key,
    kind: entity?.kind ?? "unknown",
    file: loc?.path,
  };
}

insightsRoute.get("/", (c) => {
  const limit = Number(c.req.query("limit") ?? "20");

  const deadKeys = engine.deadCode();
  const deadCode: InsightItem[] = deadKeys
    .slice(0, Math.max(limit, 50))
    .map((key) => {
      const item = resolveItem(key);
      return {
        ...item,
        category: deadCodeCategory(item.name),
      };
    });

  const centralSymbols = engine
    .centralSymbols(limit)
    .map((s) => ({ ...resolveItem(s.key), score: s.score }));

  const bridgeSymbols = engine
    .bridgeSymbols(limit)
    .map((s) => ({ ...resolveItem(s.key), score: s.score }));

  return c.json({
    deadCode,
    centralSymbols,
    bridgeSymbols,
  });
});
