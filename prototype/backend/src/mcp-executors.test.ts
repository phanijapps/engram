import { describe, expect, it } from "vitest";
import { indexRepo, type ToolDeps } from "./mcp/executors.js";

// Regression: index_repo must forward a non-null Policy (and actor) to
// startScanJob. The Rust ingest engine requires a struct Policy; the HTTP
// /ingest/jobs route supplies one, but the MCP path historically did not,
// producing "invalid type: null, expected struct Policy".
describe("indexRepo → startScanJob payload", () => {
  it("forwards a non-null policy and actor, not just {root, scope, force}", async () => {
    let captured: Record<string, unknown> | undefined;
    const deps: ToolDeps = {
      ingest: {
        startScanJob: async (input) => {
          captured = input as Record<string, unknown>;
          return { jobId: "job-1" };
        },
        getScanJob: async () => ({}),
      },
      knowledge: { listEntities: async () => [] },
      answerQuestion: async () => ({}),
      scope: { tenant: "t", workspace: "w", environment: "test" },
      policy: { visibility: "workspace", retention: "durable" },
      actor: { id: "actor-demo", kind: "agent" },
    };

    await indexRepo(deps, { path: "/abs/path/to/repo", force: true });

    expect(captured).toBeDefined();
    expect(captured!.policy).toBeTruthy();
    expect(captured!.policy).toMatchObject({ visibility: "workspace" });
    expect(captured!.actor).toBeTruthy();
    expect(captured!.root).toBe("/abs/path/to/repo");
    expect(captured!.scope).toEqual(deps.scope);
    expect(captured!.force).toBe(true);
    // a human-readable source name is derived from the path
    expect(typeof captured!.sourceName).toBe("string");
  });
});
