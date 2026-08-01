/* Subprocess integration test: spawns the REAL built `engram-maintain` bin against
 * the real addon, asserting the maintenance path emits a ConsolidationRun. Skips
 * when the build chain isn't ready (addon absent, or @engram/node / runtime not
 * built). Asserts on `status` (always present); `tasks` is omitted on an empty
 * corpus (`#[serde(skip_serializing_if = "Vec::is_empty")]`). */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const BIN = fileURLToPath(new URL("../dist/maintenance/bin.js", import.meta.url));
const ADDON = fileURLToPath(new URL("../../node/engram_node.node", import.meta.url));
const NODE_DIST = fileURLToPath(new URL("../../node/dist/index.js", import.meta.url));

const ready =
  existsSync(BIN) && existsSync(ADDON) && existsSync(NODE_DIST);

describe.skipIf(!ready)("engram-maintain CLI (subprocess, real addon)", () => {
  it("runs consolidation and emits a ConsolidationRun with a status", () => {
    const store = mkdtempSync(join(tmpdir(), "engram-maintain-cli-"));
    const config = JSON.stringify({
      storage_path: join(store, "db"),
      trusted_root: store,
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

    const stdout = execFileSync(
      "node",
      [BIN, "--config", config, "--tenant", "t", "--workspace", "w"],
      { encoding: "utf8", timeout: 60_000 }
    );

    const line = stdout.trim().split("\n").pop();
    expect(line, `stdout was: ${stdout}`).toBeTruthy();
    const run = JSON.parse(line as string) as { status?: string };
    expect(typeof run.status).toBe("string");
  }, 60_000);
});
