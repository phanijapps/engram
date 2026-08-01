import { describe, it, expect } from "vitest";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { buildEngramConfig, buildScope } from "../src/shared/config.js";

describe("shared config helpers", () => {
  it("buildEngramConfig passes inline JSON through (validated)", () => {
    const json = '{"storage_path":"/tmp/x"}';
    expect(buildEngramConfig(json)).toBe(json);
  });

  it("buildEngramConfig reads a file path", () => {
    const dir = mkdtempSync(join(tmpdir(), "engram-runtime-"));
    const file = join(dir, "cfg.json");
    const json = '{"storage_path":"/tmp/y"}';
    writeFileSync(file, json);
    expect(buildEngramConfig(file)).toBe(json);
  });

  it("buildEngramConfig throws on invalid JSON", () => {
    expect(() => buildEngramConfig("{not json")).toThrow();
  });

  it("buildScope omits workspace when unset (exactOptionalPropertyTypes)", () => {
    const scope = buildScope({ tenant: "t" });
    expect(scope).toEqual({ tenant: "t" });
    expect("workspace" in scope).toBe(false);
  });

  it("buildScope includes workspace when set", () => {
    expect(buildScope({ tenant: "t", workspace: "w" })).toEqual({
      tenant: "t",
      workspace: "w"
    });
  });
});
