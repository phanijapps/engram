// Taxonomy routes — concept schemes, concepts, and relations.

import type { Hono } from "hono";
import { getKnowledgeTransport } from "../adapters/engram.client.js";

export function registerTaxonomyRoutes(app: Hono): void {
  app.post("/taxonomy/scheme", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putConceptScheme(request));
  });
  app.post("/taxonomy/concept", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putConcept(request));
  });
  app.post("/taxonomy/relation", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putConceptRelation(request));
  });
  app.post("/taxonomy/concepts", async (c) => {
    const { schemeId, scope } = await c.req.json();
    return c.json(await getKnowledgeTransport().listConcepts(schemeId, scope));
  });
  app.post("/taxonomy/validate", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().validateTaxonomyProposal(request));
  });
}
