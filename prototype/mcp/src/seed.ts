// Idempotent startup seeder for the MCP server.
//
// Loads the Code Repo + IT SDLC ontology and taxonomy into the knowledge
// transport on first run. Subsequent calls are safe — the transport treats
// putOntology / putClass / etc. as upserts keyed by id.
//
// Called once from mcp-stdio.ts before the server starts accepting requests.

import { getKnowledgeTransport } from "./adapters/engram.client.js";
import { buildCodeSdlcOntology } from "./data/code-sdlc-ontology.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "./data/scan-defaults.js";

export async function seedOntologies(): Promise<void> {
  const transport = getKnowledgeTransport();
  const sample = buildCodeSdlcOntology({
    scope: SCAN_SCOPE,
    policy: SCAN_POLICY,
    actor: SCAN_ACTOR,
    now: new Date().toISOString(),
  });

  await transport.putOntology(sample.ontology);
  for (const klass of sample.classes) await transport.putClass(klass);
  for (const property of sample.properties) await transport.putProperty(property);
  for (const axiom of sample.axioms) await transport.putAxiom(axiom);
  for (const scheme of sample.schemes) await transport.putConceptScheme(scheme);
  for (const concept of sample.concepts) await transport.putConcept(concept);

  console.error(
    `[engram-mcp] seeded code-sdlc ontology: ${sample.classes.length} classes, ` +
    `${sample.properties.length} properties, ${sample.axioms.length} axioms, ` +
    `${sample.schemes.length} schemes, ${sample.concepts.length} concepts`,
  );
}
