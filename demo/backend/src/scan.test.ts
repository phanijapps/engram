import { afterEach, beforeEach, describe, expect, it } from "vitest";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { safeReadText, walk } from "./scan.js";

let tmp: string;

beforeEach(async () => {
  tmp = await fs.mkdtemp(path.join(os.tmpdir(), "scan-"));
});

afterEach(async () => {
  await fs.rm(tmp, { recursive: true, force: true });
});

describe("walk", () => {
  it("includes source files and skips denylisted / secret / unknown", async () => {
    await fs.mkdir(path.join(tmp, "node_modules"), { recursive: true });
    await fs.writeFile(path.join(tmp, "a.ts"), "export const x = 1;");
    await fs.writeFile(path.join(tmp, "README.md"), "# hi");
    await fs.writeFile(path.join(tmp, ".env"), "SECRET=1");
    await fs.writeFile(path.join(tmp, "logo.png"), "bytes");
    await fs.writeFile(path.join(tmp, "node_modules", "pkg.js"), "x");

    const files = [];
    for await (const f of walk(tmp)) files.push(f);

    const included = files.filter((f) => f.include).map((f) => f.relPath).sort();
    expect(included).toEqual(["README.md", "a.ts"].sort());
    expect(files.find((f) => f.relPath === "a.ts")?.kind).toBe("code");
    expect(files.find((f) => f.relPath === ".env")?.reason).toBe("secret");
    expect(files.find((f) => f.relPath === "logo.png")?.reason).toBe("not-text-or-code");
    // node_modules contents are never reached (denylisted dir is pruned).
    expect(files.some((f) => f.relPath.includes("node_modules"))).toBe(false);
  });
});

describe("safeReadText confinement", () => {
  it("reads files inside the root", async () => {
    await fs.writeFile(path.join(tmp, "in.txt"), "hello");
    const text = await safeReadText(tmp, path.join(tmp, "in.txt"));
    expect(text).toBe("hello");
  });

  it("rejects a symlink that escapes the root", async () => {
    const outside = await fs.mkdtemp(path.join(os.tmpdir(), "out-"));
    try {
      await fs.writeFile(path.join(outside, "secret.txt"), "nope");
      await fs.symlink(
        path.join(outside, "secret.txt"),
        path.join(tmp, "link.txt"),
        "file"
      );
      await expect(safeReadText(tmp, path.join(tmp, "link.txt"))).rejects.toThrow(
        /escapes root/
      );
    } finally {
      await fs.rm(outside, { recursive: true, force: true });
    }
  });
});
