import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { app } from "../src/app.js";

// Proves the demo backend routes real Rust memory behavior end to end. Skips
// when the native addon has not been built so default `pnpm test` stays green
// without a Rust toolchain. Build order: `pnpm --filter @engram/node build:native`
// then `pnpm --filter @engram/node build`.

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
const examples = join(here, "..", "..", "..", "contracts", "v1", "examples");
const fixture = (name: string): unknown =>
  JSON.parse(readFileSync(join(examples, name), "utf8"));

describe.skipIf(!existsSync(addonPath))(
  "demo/backend /memory API (real Rust)",
  () => {
    it("writes, retrieves, and forgets a memory through the Hono app", async () => {
      const writeRes = await app.request("/memory/write", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(fixture("write-memory-request.json")),
      });
      const write = (await writeRes.json()) as { record: { id: string } };
      expect(writeRes.status).toBe(200);
      expect(write.record.id).toBeTruthy();

      const retrieveRes = await app.request("/memory/retrieve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(fixture("retrieval-request.json")),
      });
      const retrieve = (await retrieveRes.json()) as { items: unknown[] };
      expect(retrieveRes.status).toBe(200);
      expect(Array.isArray(retrieve.items)).toBe(true);
      expect(retrieve.items.length).toBeGreaterThan(0);

      const forgetRes = await app.request("/memory/forget", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ...(fixture("forget-request.json") as object),
          targetId: write.record.id,
        }),
      });
      const forget = (await forgetRes.json()) as { status: string };
      expect(forgetRes.status).toBe(200);
      expect(forget.status).toBeTruthy();
    });
  }
);
