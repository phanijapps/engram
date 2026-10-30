import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const helperUrl = new URL("./helpers/build-native-recipes.mjs", import.meta.url).href;

type RecipeResult = Record<string, { path: string; sha256: string }>;
type ParticipantResult = { result: RecipeResult; enteredAt: number; releasedAt: number };

function runHelperParticipant(participant: number, artifactsRoot?: string, holdMilliseconds = 0, entryMarker?: string): Promise<ParticipantResult> {
  const program = `
    import { writeFileSync } from "node:fs";
    import { withRecipeArtifacts } from ${JSON.stringify(helperUrl)};
    const holdMilliseconds = ${holdMilliseconds};
    const entryMarker = ${JSON.stringify(entryMarker)};
    const transaction = withRecipeArtifacts(({ paths, digests, rebuild: _rebuild }) => {
      const enteredAt = Date.now();
      if (entryMarker) {
        writeFileSync(entryMarker, "entered");
      }
      if (holdMilliseconds > 0) {
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, holdMilliseconds);
      }
      return {
        result: Object.fromEntries(Object.entries(paths).map(([recipe, path]) => [recipe, {
          path,
          sha256: digests[recipe]
        }])),
        enteredAt,
        releasedAt: Date.now()
      };
    }, ${JSON.stringify(artifactsRoot ? { artifactsRoot } : {})});
    console.log("TASK_020_RECIPE_RESULT=" + JSON.stringify(transaction));
  `;
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["--input-type=module", "--eval", program], {
      stdio: ["ignore", "pipe", "pipe"]
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.once("error", reject);
    child.once("close", (code) => {
      if (code !== 0) {
        reject(new Error(`participant ${participant} exited ${code}: ${stderr}`));
        return;
      }
      const lines = stdout.split("\n");
      let line: string | undefined;
      for (let index = lines.length - 1; index >= 0; index -= 1) {
        if (lines[index]?.startsWith("TASK_020_RECIPE_RESULT=")) {
          line = lines[index];
          break;
        }
      }
      if (!line) {
        reject(new Error(`participant ${participant} produced no recipe result: ${stdout}`));
        return;
      }
      resolve(JSON.parse(line.slice("TASK_020_RECIPE_RESULT=".length)));
    });
  });
}

async function waitForParticipantEntry(entryMarker: string) {
  const deadline = Date.now() + 30_000;
  while (!existsSync(entryMarker)) {
    if (Date.now() >= deadline) {
      throw new Error(`timed out waiting for shared participant entry: ${entryMarker}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

describe("task_020 helper cross-process serialization", () => {
  it("serializes four concurrent artifact transactions without corrupt outputs", async () => {
    const participants = await Promise.all([0, 1, 2, 3].map((participant) => runHelperParticipant(participant)));
    for (const recipe of ["sqlite", "pgvector"] as const) {
      const results = participants.map((participant) => participant.result[recipe]);
      for (const result of results) {
        expect(result).toBeDefined();
        if (!result) {
          throw new Error(`participant omitted ${recipe}`);
        }
        expect(result.sha256).toHaveLength(64);
      }
      const complete = results.filter((result): result is { path: string; sha256: string } => result !== undefined);
      expect(new Set(complete.map((result) => result.sha256)).size).toBe(1);
      // Digests are calculated before the shared-target transaction releases.
    }
  }, 15 * 60 * 1000);

  it("makes an isolated-root transaction wait for the shared-target pipeline", async () => {
    const isolatedRoot = mkdtempSync(join(tmpdir(), "engram-node-isolated-lock-"));
    const sharedEntryMarker = join(isolatedRoot, "shared-entered");
    try {
      const shared = runHelperParticipant(1, undefined, 750, sharedEntryMarker);
      await waitForParticipantEntry(sharedEntryMarker);
      const isolated = runHelperParticipant(2, isolatedRoot);
      const [sharedResult, isolatedResult] = await Promise.all([shared, isolated]);

      expect(isolatedResult.enteredAt).toBeGreaterThanOrEqual(sharedResult.releasedAt);
      const sqlite = isolatedResult.result.sqlite;
      const pgvector = isolatedResult.result.pgvector;
      expect(sqlite).toBeDefined();
      expect(pgvector).toBeDefined();
      expect(sqlite?.sha256).toHaveLength(64);
      expect(pgvector?.sha256).toHaveLength(64);
    } finally {
      rmSync(isolatedRoot, { force: true, recursive: true });
    }
  }, 15 * 60 * 1000);
});
