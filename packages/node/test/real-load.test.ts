import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

// Proves the real Rust bridge — not the injected fake from transport.test.ts.
// Skips when the addon has not been built so default `pnpm test` stays green
// without a Rust toolchain. Run `pnpm --filter @engram/node build:native` first.

const here = dirname(fileURLToPath(import.meta.url));
const addonPath = join(here, "..", "engram_node.node");
const examples = join(here, "..", "..", "..", "contracts", "v1", "examples");

const fixture = (name: string): unknown =>
  JSON.parse(readFileSync(join(examples, name), "utf8"));

const require = createRequire(import.meta.url);

describe.skipIf(!existsSync(addonPath))(
  "@engram/node real load (run `pnpm --filter @engram/node build:native` if skipped)",
  () => {
    it("round-trips write -> retrieve -> forget against real Rust", () => {
      const { NativeMemoryEngine } = require(addonPath) as {
        NativeMemoryEngine: new () => {
          writeMemoryJson(requestJson: string): string;
          retrieveJson(requestJson: string): string;
          forgetJson(requestJson: string): string;
        };
      };
      const engine = new NativeMemoryEngine();

      const write = JSON.parse(
        engine.writeMemoryJson(JSON.stringify(fixture("write-memory-request.json")))
      ) as { record: { id: string } };
      expect(write.record.id).toBeTruthy();

      const retrieve = JSON.parse(
        engine.retrieveJson(JSON.stringify(fixture("retrieval-request.json")))
      ) as { items: unknown[] };
      expect(Array.isArray(retrieve.items)).toBe(true);
      expect(retrieve.items.length).toBeGreaterThan(0);

      const forgetRequest = {
        ...(fixture("forget-request.json") as object),
        targetId: write.record.id,
      };
      const forget = JSON.parse(
        engine.forgetJson(JSON.stringify(forgetRequest))
      ) as { status: string };
      expect(forget.status).toBeTruthy();
    });
  }
);
