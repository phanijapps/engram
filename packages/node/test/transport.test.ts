import { describe, expect, it } from "vitest";

import { createNativeMemoryTransport, type NativeBinding } from "../src/index.js";

class FakeNativeMemoryEngine {
  readonly calls: string[] = [];

  writeMemoryJson(requestJson: string): string {
    this.calls.push(`write:${JSON.parse(requestJson).idempotencyKey ?? ""}`);
    return JSON.stringify({
      record: { id: "memory-native-1" },
      event: { id: "event-native-1" },
      deduplicated: false
    });
  }

  retrieveJson(requestJson: string): string {
    this.calls.push(`retrieve:${JSON.parse(requestJson).query ?? ""}`);
    return JSON.stringify({
      items: [],
      omitted: [],
      sourceFailures: [],
      createdAt: "2026-06-29T12:00:00Z"
    });
  }

  forgetJson(requestJson: string): string {
    this.calls.push(`forget:${JSON.parse(requestJson).targetId ?? ""}`);
    return JSON.stringify({
      targetType: "memory",
      targetId: "memory-native-1",
      status: "deleted"
    });
  }
}

describe("@engram/node", () => {
  it("translates generated contract objects through the native JSON binding", async () => {
    let engine: FakeNativeMemoryEngine | undefined;
    const binding: NativeBinding = {
      NativeMemoryEngine: class extends FakeNativeMemoryEngine {
        constructor() {
          super();
          engine = this;
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
      NativeRetrievalEngine: class {
        indexJson(): string { return '{"indexed":0}'; }
        searchJson(): string { return "[]"; }
        indexChunkJson(): string { return '{"embedded":false,"total":0}'; }
        cacheStatsJson(): string { return '{"embedded":0}'; }
        clearJson(): string { return '{"cleared":true}'; }
      }
    };
    const transport = createNativeMemoryTransport({ binding });

    await transport.writeMemory({ idempotencyKey: "test-key" } as never);
    await transport.retrieve({ query: "stack" } as never);
    await transport.forget({ targetId: "memory-native-1" } as never);

    expect(engine?.calls).toEqual([
      "write:test-key",
      "retrieve:stack",
      "forget:memory-native-1"
    ]);
  });
});
