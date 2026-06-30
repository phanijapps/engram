import { describe, expect, it } from "vitest";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
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
const now = "2026-06-30T00:00:00Z";
const provenance = {
  source: "demo-test",
  actor: { id: "actor-demo", kind: "agent" },
  observedAt: now,
  confidence: 1,
  method: "manual",
};
const policy = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
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
  "demo/backend /taxonomy API (real Rust)",
  () => {
    it("creates a scheme, adds concepts, and lists them", async () => {
      const scheme = (await post("/taxonomy/scheme", {
        id: "scheme-test",
        uri: "urn:scheme:test",
        name: "Test Scheme",
        scope: SCOPE,
        version: "1.0.0",
        provenance,
        policy,
        createdAt: now,
      })) as { id: string };
      expect(scheme.id).toBe("scheme-test");

      for (const id of ["concept-a", "concept-b"]) {
        await post("/taxonomy/concept", {
          id,
          uri: `urn:concept:${id}`,
          schemeId: "scheme-test",
          prefLabel: { value: id },
          altLabels: [],
          status: "active",
          provenance,
          createdAt: now,
        });
      }

      const list = (await post("/taxonomy/concepts", {
        schemeId: "scheme-test",
        scope: SCOPE,
      })) as Array<{ id: string }>;
      expect(Array.isArray(list)).toBe(true);
      expect(list).toHaveLength(2);
      expect(list.map((c) => c.id)).toEqual(["concept-a", "concept-b"]);
    });
  }
);
