//! GET /api/ontology — ontology classes, properties, and axioms.
//!
//! Returns all ontology data for the current scope. The N-API
//! `getOntologyJson` returns only the Ontology header; child collections
//! (classes, properties, axioms) require direct SQLite reads. When no ontology
//! is indexed the response is `{ ontologies: [], classes: [], properties: [], axioms: [] }`.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const ontologyRoute = new Hono();

ontologyRoute.get("/", (c) => {
  try {
    const data = engine.ontology();
    return c.json(data);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error("[ontology] error:", msg);
    return c.json({ ontologies: [], classes: [], properties: [], axioms: [] });
  }
});
