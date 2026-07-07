import { describe, expect, it } from "vitest";

import { createNativeEngramClient } from "../src/index.js";

describe("@engram/client native transport", () => {
  it("creates a client over the node native transport boundary", async () => {
    const client = createNativeEngramClient({
      binding: {
        NativeMemoryEngine: class {
          writeMemoryJson(): string {
            return JSON.stringify({
              record: { id: "memory-client-1" },
              event: { id: "event-client-1" },
              deduplicated: false
            });
          }

          retrieveJson(): string {
            return JSON.stringify({
              items: [{ targetId: "memory-client-1" }],
              omitted: [],
              sourceFailures: [],
              createdAt: "2026-06-29T12:00:00Z"
            });
          }

          forgetJson(): string {
            return JSON.stringify({
              targetType: "memory",
              targetId: "memory-client-1",
              status: "deleted"
            });
          }
        },
        NativeKnowledgeEngine: class {
          putEntityJson(): string { return "null"; }
          putRelationshipJson(): string { return "null"; }
          getEntityJson(): string { return "null"; }
          putGraphJson(): string { return "null"; }
          getGraphJson(): string { return "null"; }
          neighborsJson(): string { return "[]"; }
          putConceptSchemeJson(): string { return "null"; }
          getConceptSchemeJson(): string { return "null"; }
          putConceptJson(): string { return "null"; }
          putConceptRelationJson(): string { return "null"; }
          listConceptsJson(): string { return "[]"; }
          validateTaxonomyProposalJson(): string { return '{"status":"passed","findings":[]}'; }
          putOntologyJson(): string { return "null"; }
          getOntologyJson(): string { return "null"; }
          putClassJson(): string { return "null"; }
          putPropertyJson(): string { return "null"; }
          putAxiomJson(): string { return "null"; }
          validateGraphJson(): string { return "[]"; }
          listGraphsJson(): string { return "[]"; }
          listEntitiesJson(): string { return "[]"; }
          listRelationshipsJson(): string { return "[]"; }
          listChunksJson(): string { return "[]"; }
          listSourcesJson(): string { return "[]"; }
          graphCandidatesJson(): string { return "[]"; }
          fuseRrfJson(): string { return "[]"; }
          fuseRrfIdsJson(): string { return "[]"; }
        },
        NativeIngestEngine: class {
          ingestExtractJson(): string {
            return '{"graph":{},"entities":[],"relationships":[],"chunkCount":0}';
          }
          startScanJobJson(): string { return '{"jobId":"job-0"}'; }
          getScanJobJson(): string {
            return '{"status":"done","processed":0,"ingested":0,"unchanged":0,"skipped":0,"errors":0}';
          }
        },
        NativeBeliefEngine: class {
          putBeliefJson(): string { return "null"; }
          listBeliefsJson(): string { return "[]"; }
          putContradictionJson(): string { return "null"; }
          listContradictionsJson(): string { return "[]"; }
          getContradictionJson(): string { return "null"; }
          resolveContradictionJson(): string { return "null"; }
          detectContradictionsJson(): string { return "[]"; }
        },
        NativeHierarchyEngine: class {
          validateParentageJson(): string { return '{"valid":true}'; }
        },
        NativeConsolidationEngine: class {
          planJson(): string { return '{"operations":[]}'; }
        },
        NativeEvalEngine: class {
          architectureCoverageJson(): string { return '{"missing":[],"failing":[]}'; }
        },
        NativeRetrievalEngine: class {
          indexJson(): string { return '{"indexed":0}'; }
          searchJson(): string { return "[]"; }
          indexChunkJson(): string { return "{}"; }
          cacheStatsJson(): string { return "{}"; }
          clearJson(): string { return "{}"; }
        }
      }
    });

    const write = await client.writeMemory({} as never);
    const context = await client.retrieve({} as never);
    const forget = await client.forget({} as never);

    expect(write.record.id).toBe("memory-client-1");
    expect(context.items[0]?.targetId).toBe("memory-client-1");
    expect(forget.status).toBe("deleted");
  });
});
