/**
 * Review r6 finding 7 regression: the task-owned recipe helper ALWAYS lets
 * Cargo validate/build each recipe — a previously "fresh" artifact can never
 * be reused as-is after coordinator/recipe inputs change. Proven here
 * mechanically: a CORRUPTED artifact is replaced by Cargo's freshly linked
 * product on the next helper invocation (the shared cdylib is removed before
 * each recipe build, so the copy is always THIS recipe's link).
 */
import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { ensureRecipeArtifacts } from "./helpers/build-native-recipes.mjs";

const sha = (path: string) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

describe("task_020 helper freshness", () => {
  it("cargo_validates_and_replaces_a_corrupted_artifact", () => {
    const paths = ensureRecipeArtifacts() as Record<string, string>;
    const sqlite = paths.sqlite as string;
    const before = sha(sqlite);

    // Corrupt the artifact (models stale implementation bytes on disk).
    writeFileSync(sqlite, Buffer.from("not an addon"));
    const corrupted = sha(sqlite);
    expect(corrupted).not.toBe(before);

    const again = ensureRecipeArtifacts() as Record<string, string>;
    const after = sha(again.sqlite as string);
    expect(after).not.toBe(corrupted); // the corrupted bytes are never reused
    // The loadable addon is restored (require would throw otherwise) and is
    // cargo's product for the same inputs.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const addon = require(again.sqlite as string);
    expect(typeof addon.NativeProvider).toBe("function");
    expect(after).toBe(before);
  }, 15 * 60 * 1000);
});
