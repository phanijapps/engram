// Ontology routes — classes, properties, axioms, graph validation, and the
// IT-org sample loader. validate_graph is advisory — it returns findings,
// never rejects writes.

import type { Hono } from "hono";
import { getKnowledgeTransport } from "../adapters/engram.client.js";
import { buildItOrgOntology } from "../data/it-org-ontology.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerOntologyRoutes(app: Hono): void {
  app.post("/ontology/ontology", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putOntology(request));
  });
  app.post("/ontology/class", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putClass(request));
  });
  app.post("/ontology/property", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putProperty(request));
  });
  app.post("/ontology/axiom", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putAxiom(request));
  });
  app.post("/ontology/get", async (c) => {
    const { id, scope } = await c.req.json();
    return c.json(await getKnowledgeTransport().getOntology(id, scope));
  });
  app.post("/ontology/validate", async (c) => {
    const { graphId, ontologyId, scope } = await c.req.json();
    return c.json(await getKnowledgeTransport().validateGraph(graphId, ontologyId, scope));
  });

  // Loads the IT-org sample ontology + taxonomy through the knowledge transport
  // and returns the records so the UI can browse them without separate list
  // endpoints.
  app.post("/ontology/it-org", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const reqScope = body.scope ?? SCAN_SCOPE;
    const reqPolicy = body.policy ?? SCAN_POLICY;
    const reqActor = body.actor ?? SCAN_ACTOR;
    const sample = buildItOrgOntology({
      scope: reqScope,
      policy: reqPolicy,
      actor: reqActor,
      now: new Date().toISOString(),
    });
    const transport = getKnowledgeTransport();
    await transport.putOntology(sample.ontology);
    for (const klass of sample.classes) await transport.putClass(klass);
    for (const property of sample.properties) await transport.putProperty(property);
    for (const axiom of sample.axioms) await transport.putAxiom(axiom);
    await transport.putConceptScheme(sample.scheme);
    for (const concept of sample.concepts) await transport.putConcept(concept);
    return c.json({ loaded: true, sample });
  });
}
