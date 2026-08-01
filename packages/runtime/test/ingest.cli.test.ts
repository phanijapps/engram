/* Subprocess integration test: spawns the REAL built `engram-ingest` bin against
 * a fixture repo + the real addon, asserting the write path lands entities. This
 * is the automated regression net for the CLI (T3 mocks the transport; this
 * exercises the built artifact end-to-end). Skips when the build chain isn't
 * ready (addon absent, or @engram/node / runtime not built). */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../../../", import.meta.url));
const BIN = fileURLToPath(new URL("../dist/ingest/bin.js", import.meta.url));
const ADDON = fileURLToPath(new URL("../../node/engram_node.node", import.meta.url));
const NODE_DIST = fileURLToPath(new URL("../../node/dist/index.js", import.meta.url));
const FIXTURE = join(ROOT, "examples/rust-integration");

const ready =
  existsSync(BIN) && existsSync(ADDON) && existsSync(NODE_DIST);

describe.skipIf(!ready)("engram-ingest CLI (subprocess, real addon)", () => {
  it("scans a fixture repo and reports entities >= 1", () => {
    const store = mkdtempSync(join(tmpdir(), "engram-ingest-cli-"));
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
      [BIN, "--config", config, "--path", FIXTURE, "--tenant", "t", "--workspace", "w"],
      { encoding: "utf8", timeout: 60_000 }
    );

    const summaryLine = stdout.trim().split("\n").pop();
    expect(summaryLine, `stdout was: ${stdout}`).toBeDefined();
    const summary = JSON.parse(summaryLine as string) as { entities?: number };
    expect(summary.entities).toBeGreaterThanOrEqual(1);
  }, 60_000);
});
