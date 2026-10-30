import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const helperUrl = new URL("./helpers/build-native-recipes.mjs", import.meta.url).href;

function runHelperParticipant(participant: number): Promise<Record<string, { path: string; sha256: string }>> {
  const program = `
    import { createHash } from "node:crypto";
    import { readFileSync } from "node:fs";
    import { ensureRecipeArtifacts } from ${JSON.stringify(helperUrl)};
    const paths = ensureRecipeArtifacts();
    const result = Object.fromEntries(Object.entries(paths).map(([recipe, path]) => [recipe, {
      path,
      sha256: createHash("sha256").update(readFileSync(path)).digest("hex")
    }]));
    console.log("TASK_020_RECIPE_RESULT=" + JSON.stringify(result));
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

describe("task_020 helper cross-process serialization", () => {
  it("serializes four concurrent artifact transactions without corrupt outputs", async () => {
    const participants = await Promise.all([0, 1, 2, 3].map(runHelperParticipant));
    for (const recipe of ["sqlite", "pgvector"] as const) {
      const results = participants.map((participant) => participant[recipe]);
      for (const result of results) {
        expect(result).toBeDefined();
        if (!result) {
          throw new Error(`participant omitted ${recipe}`);
        }
        expect(result.sha256).toHaveLength(64);
      }
      const complete = results.filter((result): result is { path: string; sha256: string } => result !== undefined);
      expect(new Set(complete.map((result) => result.sha256)).size).toBe(1);
      // Each child hashes its artifact before releasing its transaction.
      // A later independent Vitest worker may legally begin a new transaction
      // before this parent observes the shared path, so only the child-local
      // validated hashes prove this concurrent transaction's outcome.
    }
  }, 15 * 60 * 1000);
});
