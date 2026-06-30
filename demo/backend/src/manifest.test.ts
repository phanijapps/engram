import { describe, expect, it } from "vitest";
import { hashContent, isUnchanged } from "./manifest.js";

describe("hashContent", () => {
  it("is deterministic for the same input", () => {
    expect(hashContent("hello world")).toBe(hashContent("hello world"));
  });
  it("differs for different input", () => {
    expect(hashContent("hello world")).not.toBe(hashContent("hello earth"));
  });
  it("returns a short hex string", () => {
    expect(hashContent("x")).toMatch(/^[0-9a-f]{16}$/);
  });
});

describe("isUnchanged", () => {
  const manifest = { "a.rs": "aa", "b.ts": "bb" };
  it("true when the stored hash matches", () => {
    expect(isUnchanged(manifest, "a.rs", "aa")).toBe(true);
  });
  it("false when the hash differs or is new", () => {
    expect(isUnchanged(manifest, "a.rs", "zz")).toBe(false);
    expect(isUnchanged(manifest, "c.go", "cc")).toBe(false);
  });
});
