import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const helperUrl = new URL("./helpers/build-native-recipes.mjs", import.meta.url).href;
const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const sharedTargetLock = join(repoRoot, "target", ".engram-node-recipe-build.lock");

type RecipeResult = Record<string, { path: string; sha256: string }>;
type ParticipantResult = { result: RecipeResult; enteredAt: number; releasedAt: number };

function runHelperParticipant(participant: number, artifactsRoot?: string, holdMilliseconds = 0): Promise<ParticipantResult> {
  const program = `
    import { withRecipeArtifacts } from ${JSON.stringify(helperUrl)};
    const holdMilliseconds = ${holdMilliseconds};
    const transaction = withRecipeArtifacts(({ paths, digests, rebuild: _rebuild }) => {
      const enteredAt = Date.now();
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

async function waitForSharedTargetLock() {
  const deadline = Date.now() + 30_000;
  while (!existsSync(sharedTargetLock)) {
    if (Date.now() >= deadline) {
      throw new Error(`timed out waiting for shared target lock: ${sharedTargetLock}`);
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
    try {
      const shared = runHelperParticipant(1, undefined, 750);
      await waitForSharedTargetLock();
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
