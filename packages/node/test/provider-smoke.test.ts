/* End-to-end smoke: exercises the REAL built addon (engram_node.node) through the
 * provider facade. Skips when the addon is absent (fresh checkout / CI without
 * `pnpm run build:native`). Proves the two integration facts mocks cannot:
 * scan lands entities in the knowledge store, and consolidation EXECUTION
 * returns a ConsolidationRun — both through the held NativeProvider over napi. */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

import { createNativeProviderTransport } from "../src/provider.js";

const ADDON_PATH = fileURLToPath(new URL("../engram_node.node", import.meta.url));

describe.skipIf(!existsSync(ADDON_PATH))("NativeProviderTransport end-to-end (real addon)", () => {
  it("constructs from config, scans a repo, and runs consolidation execution", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-smoke-"));
    const store = join(base, "store");
    const repo = join(base, "repo");
    mkdirSync(repo, { recursive: true });
    writeFileSync(join(repo, "fixture.rs"), "pub fn smoke() -> u32 { 42 }\n");

    const configJson = JSON.stringify({
      storage_path: store,
      trusted_root: base,
      scope_policy: "Strict",
      embedding_provider: {
        provider_type: "none",
        model: "none",
        dimensions: 384,
        prompt_profile: "query"
      },
      migration_mode: "Apply",
      capability_policy: "FailClosed"
    });

    try {
      const transport = createNativeProviderTransport({ configJson });

      // 1. The held provider opens from a config (the keystone) and reports capabilities.
      const caps = (await transport.capabilities()) as { families?: unknown[] };
      expect(caps).toBeDefined();

      // 2. The explicit legacy lane scans documents through the facade + N-API
      //    boundary. Bound ingestion is covered by the registered-repository
      //    recipe suite because it requires an authorized git registration.
      const scope = { tenant: "smoke", workspace: "smoke" };
      writeFileSync(join(repo, "README.md"), "# smoke\n");
      const summary = await transport.scan({ path: repo, scope });
      // eslint-disable-next-line no-console
      console.log("[smoke] scan:", JSON.stringify(summary));
      expect(summary.mode).toBe("legacy-document-only");
      if (summary.mode !== "legacy-document-only") {
        throw new Error(`expected legacy document scan, got ${summary.mode}`);
      }
      expect(summary.scanned).toBeGreaterThanOrEqual(1);

      // 3. Consolidation EXECUTION (non-dry-run) returns a ConsolidationRun.
      const run = (await transport.consolidate({ scope, dryRun: false })) as {
        status?: unknown;
        tasks?: unknown[];
      };
      // eslint-disable-next-line no-console
      console.log("[smoke] consolidate:", JSON.stringify(run));
      expect(run.tasks).toBeInstanceOf(Array);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  }, 60_000);
});
