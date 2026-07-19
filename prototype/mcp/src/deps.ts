import { getIngestTransport, getKnowledgeTransport, scanManifestPath } from "./adapters/engram.client.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "./data/scan-defaults.js";
import type { ToolDeps, ToolScope } from "./executors.js";

export function buildToolDeps(scope: ToolScope = SCAN_SCOPE): ToolDeps {
  return {
    ingest: {
      startScanJob: (input) => getIngestTransport().startScanJob(input),
      getScanJob: (jobId) => getIngestTransport().getScanJob(jobId),
    },
    knowledge: {
      listEntities: (s) => getKnowledgeTransport().listEntities(s),
      listEntitiesBySource: (key, s) => getKnowledgeTransport().listEntitiesBySource(key, s),
      listRelationshipsBySource: (key, s) => getKnowledgeTransport().listRelationshipsBySource(key, s),
    },
    scope,
    policy: SCAN_POLICY,
    actor: SCAN_ACTOR,
    manifestPath: scanManifestPath() ?? undefined,
  };
}
