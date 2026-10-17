// Opt-in inference over the real addon and actual model assets:
// ENGRAM_TEST_EMBEDDING_PRESETS=default,local-alternative pnpm --filter @engram/node test
// Model assets must already be cached, or this explicitly requested run downloads them.
import { describe, expect, it } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { resolveEmbeddingConfig, type EmbeddingConfigOverrides } from "../src/embedding-config.js";
import { createNativeProviderTransport } from "../src/provider.js";

const presets = (process.env.ENGRAM_TEST_EMBEDDING_PRESETS ?? "")
  .split(",").filter(Boolean);

describe.skipIf(presets.length === 0)("real native embedding inference (opt-in)", () => {
  for (const preset of presets) {
    it(`resolves ${preset} and generates query, passage and batch vectors`, async () => {
      const base = mkdtempSync(join(tmpdir(), "engram-native-embedding-"));
      try {
        const config = resolveEmbeddingConfig({ overrides: {
          preset: preset as NonNullable<EmbeddingConfigOverrides["preset"]>,
        } });
        const provider = createNativeProviderTransport({ configJson: JSON.stringify({
          storage_path: join(base, "store"), trusted_root: base,
          scope_policy: "Strict", embedding_provider: config,
          migration_mode: "Apply", capability_policy: "FailClosed", enable_vector: true,
        }) });
        const embedding = provider.embedding();
        const space = await embedding.embeddingSpace() as { model: string; dimensions: number };
        expect(space.model).toBe(config.model);
        expect(space.dimensions).toBe(config.dimensions);
        expect(await embedding.providerId()).toBe("fastembed");
        const query = await embedding.embedQuery("Retrieve a user by their identifier");
        const passage = await embedding.embedPassage("function getUser(id) { return db.find(id); }");
        const batches = await embedding.embedBatch([
          "function getUser(id) { return db.find(id); }", "function deleteUser(id) { return db.delete(id); }",
        ]);
        for (const vector of [query, passage, ...batches]) {
          expect(vector).toHaveLength(space.dimensions);
          expect(vector.every(Number.isFinite)).toBe(true);
          expect(vector.some((value) => value !== 0)).toBe(true);
        }
        expect(batches).toHaveLength(2);
        expect(Math.max(...batches[0]!.map((value, index) => Math.abs(value - passage[index]!))))
          .toBeLessThan(0.00001);
        expect(batches[0]).not.toEqual(batches[1]);
      } finally {
        rmSync(base, { recursive: true, force: true });
      }
    }, 180_000);
  }
});
