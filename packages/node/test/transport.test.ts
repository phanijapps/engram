import { describe, expect, it } from "vitest";

import {
  createNativeConsolidationTransport,
  createNativeEvalTransport,
  createNativeHierarchyTransport,
  createNativeKnowledgeTransport,
  createNativeMemoryTransport,
  createNativeProviderTransport,
  type NativeBinding
} from "../src/index.js";

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

/** Minimal NativeProvider stub for fixtures that mock NativeBinding.
 *  The transport tests do not exercise the provider; this only satisfies the
 *  NativeBinding shape now that NativeProvider is a required member. */
class StubNativeProvider {
  constructor(_configJson?: string) {}
  capabilitiesJson(): string {
    return "{}";
  }
  consolidateJson(): string {
    return '{"status":"Complete","tasks":[]}';
  }
  scanRepositoryJson(): string {
    return '{"scanned":0,"ingested":0,"unchanged":0,"skipped":0,"entities":0,"relationships":0,"errors":0}';
  }
  requireMemoryApi() {
    return {
      searchJson(): string {
        return "";
      },
      writeJson(): string {
        return "";
      },
      forgetJson(): string {
        return "";
      },
      listMemoriesPagedJson(): string {
        return '{"items":[],"nextCursor":null}';
      }
    };
  }
  requireRecallApi() {
    return {
      recallJson(): string {
        return "";
      }
    };
  }
  requireGraphApi() {
    return {
      getEntityJson(): string {
        return "null";
      },
      putEntityJson(): string {
        return "null";
      },
      putRelationshipJson(): string {
        return "null";
      },
      neighborsJson(): string {
        return "[]";
      }
    };
  }
  requireGraphMaintenanceApi() {
    return {
      buildPlanJson(): string {
        return '{"mutations":[],"fingerprint":"","previews":[],"scope":{"tenant":"t"},"policy":{},"actor":{"id":"a","kind":"system"}}';
      },
      applyPlanJson(): string {
        return '{"applied":0,"unchanged":0,"failed":0,"byKind":[],"verifyFindings":[],"atomicity":"single_transaction","planFingerprint":""}';
      },
      listMaintenanceCandidatesJson(): string {
        return "[]";
      },
      graphHealthJson(): string {
        return '{"scope":{"tenant":"t"},"orphanCount":0,"lowConfidenceCount":0,"unsupportedCount":0,"duplicateCount":0,"archivedEntityCount":0,"archivedRelationshipCount":0}';
      }
    };
  }
  requireBatchApi() {
    return {
      ingestJson(): string {
        return "";
      },
      transactionGuarantee(): string {
        return '"BestEffort"';
      }
    };
  }
  requireBeliefsApi() {
    return {
      getBeliefJson(): string {
        return "null";
      },
      upsertBeliefJson(): string {
        return "null";
      },
      retractBeliefJson(): string {
        return "null";
      },
      listStaleBeliefsJson(): string {
        return "[]";
      },
      listBeliefsJson(): string {
        return "[]";
      },
      listBeliefsPagedJson(): string {
        return '{"items":[],"nextCursor":null}';
      },
      listContradictionsJson(): string {
        return "[]";
      },
      putContradictionJson(): string {
        return "null";
      }
    };
  }
  requireObservabilityApi() {
    return {
      diagnosticsJson(): string {
        return '{"record_counts":{}}';
      }
    };
  }
  requireCommunityQueryApi() {
    return {
      overviewJson(): string {
        return '{"communities":[],"edges":[],"totalCommunities":0}';
      },
      memberIndexJson(): string {
        return "{}";
      },
      communityOfJson(): string {
        return "null";
      },
      scopeCountsJson(): string {
        return '{"entities":0,"relationships":0,"memories":0,"beliefs":0,"hierarchyNodes":0,"hierarchyRelations":0}';
      }
    };
  }
  requireKnowledgeQueryApi() {
    return {
      listEntitiesJson(): string {
        return "[]";
      },
      listRelationshipsJson(): string {
        return "[]";
      },
      listGraphsJson(): string {
        return "[]";
      },
      listChunksByDocumentJson(): string {
        return "[]";
      }
    };
  }
  requireHierarchyApi() {
    return {
      pathForJson(): string {
        return "null";
      },
      buildHierarchyJson(): string {
        return JSON.stringify({
          clusterCount: 0,
          entitiesClustered: 0,
          totalEntities: 0,
          totalRelationships: 0,
          interClusterRelationCount: 0,
        });
      }
    };
  }
  requireProceduresApi() {
    return {
      upsertJson(): string {
        return "null";
      },
      listJson(): string {
        return "[]";
      },
      incrementSuccessJson(): string {
        return "null";
      },
      incrementFailureJson(): string {
        return "null";
      }
    };
  }
  static fromProfileFile(_path: string): StubNativeProvider {
    return new StubNativeProvider();
  }
}

describe("@engram/node", () => {
  it("translates generated contract objects through the native JSON binding", async () => {
    let engine: FakeNativeMemoryEngine | undefined;
    const binding: NativeBinding = {
      NativeProvider: StubNativeProvider,
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
        listEntitiesBySourceJson(): string { return "[]"; }
        listRelationshipsBySourceJson(): string { return "[]"; }
        listChunksJson(): string { return "[]"; }
        listChunksByDocumentJson(): string { return "[]"; }
        listSourcesJson(): string { return "[]"; }
        graphCandidatesJson(): string { return "[]"; }
        associativeGraphCandidatesJson(): string { return "[]"; }
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

  it("delegates architecture surfaces to native JSON transports", async () => {
    const calls: string[] = [];
    const binding: NativeBinding = {
      NativeProvider: StubNativeProvider,
      NativeMemoryEngine: class extends FakeNativeMemoryEngine {},
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
        validateTaxonomyProposalJson(requestJson: string): string {
          calls.push(`taxonomy:${JSON.parse(requestJson).proposal.id}`);
          return '{"status":"passed","findings":[]}';
        }
        putOntologyJson(): string { return "null"; }
        getOntologyJson(): string { return "null"; }
        putClassJson(): string { return "null"; }
        putPropertyJson(): string { return "null"; }
        putAxiomJson(): string { return "null"; }
        validateGraphJson(): string { return "[]"; }
        listGraphsJson(): string { return "[]"; }
        listEntitiesJson(): string { return "[]"; }
        listRelationshipsJson(): string { return "[]"; }
        listEntitiesBySourceJson(): string { return "[]"; }
        listRelationshipsBySourceJson(): string { return "[]"; }
        listChunksJson(): string { return "[]"; }
        listChunksByDocumentJson(): string { return "[]"; }
        listSourcesJson(): string { return "[]"; }
        graphCandidatesJson(): string { return "[]"; }
        associativeGraphCandidatesJson(): string { return "[]"; }
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
        validateParentageJson(requestJson: string): string {
          calls.push(`hierarchy:${JSON.parse(requestJson).length}`);
          return '{"valid":true}';
        }
      },
      NativeConsolidationEngine: class {
        planJson(requestJson: string): string {
          calls.push(`consolidation:${JSON.parse(requestJson).request.strategy}`);
          return '{"operations":[{"kind":"evaluation_gate"}]}';
        }
      },
      NativeEvalEngine: class {
        architectureCoverageJson(requestJson: string): string {
          calls.push(`eval:${JSON.parse(requestJson).length}`);
          return '{"missing":[],"failing":[]}';
        }
      },
      NativeRetrievalEngine: class {
        indexJson(): string { return '{"indexed":0}'; }
        searchJson(): string { return "[]"; }
        indexChunkJson(): string { return '{"embedded":false,"total":0}'; }
        cacheStatsJson(): string { return '{"embedded":0}'; }
        clearJson(): string { return '{"cleared":true}'; }
      }
    };

    await createNativeKnowledgeTransport({ binding }).validateTaxonomyProposal({
      proposal: { id: "proposal-1" },
      concepts: [],
      relations: []
    });
    await createNativeHierarchyTransport({ binding }).validateParentage([{}]);
    await createNativeConsolidationTransport({ binding }).plan({
      request: { strategy: "hybrid" }
    });
    await createNativeEvalTransport({ binding }).architectureCoverage([{}]);

    expect(calls).toEqual([
      "taxonomy:proposal-1",
      "hierarchy:1",
      "consolidation:hybrid",
      "eval:1"
    ]);
  });
});

describe("NativeProviderTransport list_graphs + list_chunks_by_document (RFC-0020 T4)", () => {
  // The extract-knowledge op reaches these through the provider transport:
  // listGraphs (discover documents) + listChunksByDocument (per-document reads).
  // This pins the request-shape encode + decode so a binding/wiring regression
  // (N-API js_name, the {documentId, scope} shape) is caught without a real
  // addon — the op test mocks the whole transport, which would hide it.
  function fakeProvider(capture: { listGraphs?: string; listChunks?: string }) {
    return {
      requireKnowledgeQueryApi: () => ({
        listGraphsJson: (scopeJson: string) => {
          capture.listGraphs = scopeJson;
          return JSON.stringify([{ id: "graph-1", metadata: { document_id: "doc-1" } }]);
        },
        listChunksByDocumentJson: (requestJson: string) => {
          capture.listChunks = requestJson;
          return JSON.stringify([
            { id: "chunk-1", text: "alpha" },
            { id: "chunk-2", text: "beta" },
          ]);
        },
      }),
    } as never;
  }

  it("listGraphs encodes the scope and decodes the graph list", async () => {
    const capture: { listGraphs?: string; listChunks?: string } = {};
    const transport = createNativeProviderTransport({ provider: fakeProvider(capture) });
    const graphs = (await transport.listGraphs({ tenant: "t", workspace: "w" })) as Array<{
      id: string;
      metadata: { document_id: string };
    }>;
    expect(JSON.parse(capture.listGraphs!)).toEqual({ tenant: "t", workspace: "w" });
    expect(graphs).toEqual([{ id: "graph-1", metadata: { document_id: "doc-1" } }]);
  });

  it("listChunksByDocument encodes {documentId, scope} and decodes the chunk list", async () => {
    const capture: { listGraphs?: string; listChunks?: string } = {};
    const transport = createNativeProviderTransport({ provider: fakeProvider(capture) });
    const chunks = (await transport.listChunksByDocument("doc-42", {
      tenant: "t",
      workspace: "w",
    })) as Array<{ id: string; text: string }>;
    const req = JSON.parse(capture.listChunks!);
    expect(req.documentId).toBe("doc-42");
    expect(req.scope).toEqual({ tenant: "t", workspace: "w" });
    expect(chunks).toEqual([
      { id: "chunk-1", text: "alpha" },
      { id: "chunk-2", text: "beta" },
    ]);
  });
});

describe("NativeProviderTransport buildHierarchy (hierarchy-build wiring)", () => {
  // The auto-build-after-ingest + maintenance_run op=hierarchy-build paths reach
  // transport.buildHierarchy → requireHierarchyApi().buildHierarchyJson. This
  // pins the request-shape encode ({ scope, maxPasses? }) and the stats decode
  // so an N-API js_name / shape regression is caught without a real addon — the
  // runtime op test mocks the whole transport, which would hide it.
  function fakeProvider(capture: { built?: string }) {
    return {
      requireHierarchyApi: () => ({
        pathForJson: () => "null",
        buildHierarchyJson: (requestJson: string) => {
          capture.built = requestJson;
          return JSON.stringify({
            clusterCount: 3,
            entitiesClustered: 12,
            totalEntities: 15,
            totalRelationships: 40,
            interClusterRelationCount: 2,
          });
        },
      }),
    } as never;
  }

  it("buildHierarchy encodes {scope} by default and decodes the stats", async () => {
    const capture: { built?: string } = {};
    const transport = createNativeProviderTransport({ provider: fakeProvider(capture) });
    const stats = (await transport.buildHierarchy({ tenant: "t", workspace: "w" })) as {
      clusterCount: number;
      totalRelationships: number;
    };
    expect(JSON.parse(capture.built!)).toEqual({ scope: { tenant: "t", workspace: "w" } });
    expect(stats.clusterCount).toBe(3);
    expect(stats.totalRelationships).toBe(40);
  });

  it("buildHierarchy forwards maxPasses when provided", async () => {
    const capture: { built?: string } = {};
    const transport = createNativeProviderTransport({ provider: fakeProvider(capture) });
    await transport.buildHierarchy({ tenant: "t" }, 8);
    expect(JSON.parse(capture.built!)).toEqual({ scope: { tenant: "t" }, maxPasses: 8 });
  });
});
