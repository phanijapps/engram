// Builds the real (native-backed) ToolDeps for the MCP tools.
//
// Isolated from mcp-tools.ts so the protocol test can stub deps without pulling
// in the @engram/node native binding.

import { getIngestTransport, getKnowledgeTransport, scanManifestPath } from "./engram.js";
import { answerQuestion } from "./qa.js";
import { SCAN_ACTOR, SCAN_POLICY, SCAN_SCOPE } from "./scan-defaults.js";
import type { ToolDeps, ToolScope } from "./mcp-executors.js";

export function buildToolDeps(scope: ToolScope = SCAN_SCOPE): ToolDeps {
  return {
    ingest: {
      startScanJob: (input) => getIngestTransport().startScanJob(input),
      getScanJob: (jobId) => getIngestTransport().getScanJob(jobId),
    },
    knowledge: {
      listEntities: (s) => getKnowledgeTransport().listEntities(s),
    },
    answerQuestion,
    scope,
    policy: SCAN_POLICY,
    actor: SCAN_ACTOR,
    manifestPath: scanManifestPath() ?? undefined,
  };
}
