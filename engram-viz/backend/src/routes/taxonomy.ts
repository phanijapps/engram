//! GET /api/taxonomy — concept schemes, concepts, and relations.
//!
//! Returns all taxonomy data for the current scope. The N-API surface lacks a
//! list-all-schemes method, so the engine discovers scheme IDs via SQLite and
//! returns the full hierarchy. When no taxonomy is indexed the response is
//! `{ schemes: [], concepts: [], relations: [] }`.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const taxonomyRoute = new Hono();

taxonomyRoute.get("/", (c) => {
  try {
    const data = engine.taxonomy();
    return c.json(data);
  } catch (err) {
    // If the table doesn't exist or the DB is locked, return an honest empty.
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[taxonomy] error:", msg);
    return c.json({ schemes: [], concepts: [], relations: [] });
  }
});
