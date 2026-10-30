import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { withRecipeArtifacts } from "./helpers/build-native-recipes.mjs";

const sha = (path: string) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

describe("task_020 helper freshness", () => {
  it("cargo_validates_and_replaces_a_corrupted_artifact", () => {
    const artifactsRoot = mkdtempSync(join(tmpdir(), "engram-node-freshness-"));
    try {
      withRecipeArtifacts(({ paths, digests, rebuild }) => {
        const sqlite = paths.sqlite as string;
        const before = digests.sqlite as string;

        // This regression owns an isolated artifact set, so its destructive
        // write cannot race a shared consumer path.
        writeFileSync(sqlite, Buffer.from("not an addon"));
        const corrupted = sha(sqlite);
        expect(corrupted).not.toBe(before);
        // The corrupt nonempty file must fail an actual addon load while the
        // isolated artifact transaction remains locked.
        expect(() => require(sqlite)).toThrow();

        const refreshed = rebuild();
        expect(refreshed.digests.sqlite).not.toBe(corrupted);
        expect(refreshed.digests.sqlite).toBe(before);
        // Loading and validating the repaired addon remains inside the lock.
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const refreshedPath = refreshed.paths.sqlite as string;
        const refreshedDigest = refreshed.digests.sqlite as string;
        const addon = require(refreshedPath);
        expect(typeof addon.NativeProvider).toBe("function");

        // A later transaction publishes a new immutable version rather than
        // rewriting the inode this already-loaded consumer maps.
        const later = rebuild();
        expect(later.paths.sqlite).not.toBe(refreshedPath);
        expect(existsSync(refreshedPath)).toBe(true);
        expect(sha(refreshedPath)).toBe(refreshedDigest);
        expect(typeof addon.NativeProvider).toBe("function");
      }, { artifactsRoot });
    } finally {
      rmSync(artifactsRoot, { force: true, recursive: true });
    }
  }, 15 * 60 * 1000);
});
