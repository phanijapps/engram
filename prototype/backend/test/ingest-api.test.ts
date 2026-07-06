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

const baseRequest = {
  sourceKind: "filesystem",
  sourceName: "demo-ingest-test",
  scope: { tenant: "tenant-demo", workspace: "engram", environment: "local" },
  document: { path: "snippet.rs" },
  text: "fn alpha() { beta(); }\nfn beta() {}\nstruct Widget;\n",
  policy: {
    visibility: "workspace",
    retention: "durable",
    sensitivity: "low",
    allowedUses: ["retrieval"],
    deleteMode: "tombstone",
  },
  actor: { id: "actor-demo", kind: "agent" },
};

describe.skipIf(!existsSync(addonPath))(
  "demo/backend /ingest/extract API (real Rust)",
  () => {
    it("ingests code and returns an extracted graph", async () => {
      const res = await app.request("/ingest/extract", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...baseRequest, documentKind: "code" }),
      });
      expect(res.status).toBe(200);
      const result = (await res.json()) as {
        entities: { name: string }[];
        relationships: { subject: { name?: string }; object: { name?: string } }[];
        chunkCount: number;
      };
      const names = result.entities.map((e) => e.name);
      expect(names).toEqual(expect.arrayContaining(["alpha", "beta", "Widget"]));
      expect(
        result.relationships.some(
          (r) => r.subject.name === "alpha" && r.object.name === "beta"
        )
      ).toBe(true);
      expect(result.chunkCount).toBeGreaterThan(0);
    });
  }
);
