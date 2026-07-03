import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { app } from "../src/app.js";

const here = dirname(fileURLToPath(import.meta.url));
const addonPath = join(
  here,
  "..",
  "..",
  "..",
  "packages",
  "node",
  "engram_node.node"
);

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const now = "2026-07-02T12:00:00Z";
const actor = { id: "actor-demo", kind: "agent" };
const requester = { actor, roles: [], permissions: [] };
const provenance = {
  source: "architecture-api-test",
  actor,
  observedAt: now,
  confidence: 1,
  method: "manual",
};

async function post(path: string, body: unknown): Promise<unknown> {
  const res = await app.request(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  expect(res.status).toBe(200);
  return res.json();
}

describe.skipIf(!existsSync(addonPath))(
  "demo/backend architecture API (real Rust)",
  () => {
    it("exposes taxonomy, hierarchy, consolidation, and eval Rust behavior", async () => {
      const taxonomy = (await post("/taxonomy/validate", {
        proposal: {
          id: "proposal-demo",
          schemeId: "scheme-demo",
          status: "proposed",
          changes: [],
          semanticDrift: [],
          proposer: actor,
          provenance,
          createdAt: now,
        },
        concepts: [],
        relations: [],
      })) as { status: string };
      expect(taxonomy.status).toBe("passed");

      const hierarchy = (await post("/hierarchy/validate-parentage", {
        nodes: [],
      })) as { valid: boolean };
      expect(hierarchy.valid).toBe(true);

      const consolidation = (await post("/consolidation/plan", {
        request: {
          scope: SCOPE,
          requester,
          strategy: "hybrid",
          dryRun: true,
        },
        plannedAt: now,
      })) as { operations: unknown[] };
      expect(consolidation.operations.length).toBeGreaterThan(0);

      const coverage = (await post("/eval/architecture-coverage", {
        cases: [
          {
            caseId: "accepted",
            capabilities: ["accepted_recall"],
            passed: true,
            failures: [],
          },
        ],
      })) as { missing: unknown[] };
      expect(coverage.missing.length).toBeGreaterThan(0);
    });
  }
);
