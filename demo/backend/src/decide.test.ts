import { describe, expect, it } from "vitest";
import {
  classifyFile,
  isDenylisted,
  isOverSize,
  isSecretFile,
  isWithinRoot,
} from "./decide.js";

describe("isWithinRoot", () => {
  it("accepts the root itself and paths inside it", () => {
    expect(isWithinRoot("/a/b", "/a/b")).toBe(true);
    expect(isWithinRoot("/a/b/c.ts", "/a/b")).toBe(true);
    expect(isWithinRoot("/a/b/x/y/c.ts", "/a/b")).toBe(true);
  });
  it("rejects traversal and siblings outside the root", () => {
    expect(isWithinRoot("/a/c.ts", "/a/b")).toBe(false);
    expect(isWithinRoot("/a/b/../c.ts", "/a/b")).toBe(false);
  });
});

describe("isDenylisted", () => {
  it("flags deny dirs anywhere in the path", () => {
    expect(isDenylisted("node_modules/x/index.js")).toBe(true);
    expect(isDenylisted("src/.git/config")).toBe(true);
    expect(isDenylisted("target/debug/app")).toBe(true);
  });
  it("flags deny file suffixes", () => {
    expect(isDenylisted("data/local.db")).toBe(true);
    expect(isDenylisted("build/engram_node.node")).toBe(true);
    expect(isDenylisted("logs/run.log")).toBe(true);
  });
  it("passes normal source files", () => {
    expect(isDenylisted("src/lib.rs")).toBe(false);
    expect(isDenylisted("README.md")).toBe(false);
  });
});

describe("isSecretFile", () => {
  it("flags env and key material", () => {
    expect(isSecretFile(".env")).toBe(true);
    expect(isSecretFile(".env.production")).toBe(true);
    expect(isSecretFile("tls.key")).toBe(true);
    expect(isSecretFile("cert.pem")).toBe(true);
    expect(isSecretFile("id_rsa")).toBe(true);
    expect(isSecretFile("id_ed25519")).toBe(true);
  });
  it("passes non-secret files", () => {
    expect(isSecretFile("main.rs")).toBe(false);
    expect(isSecretFile(".env.example")).toBe(false);
  });
});

describe("isOverSize", () => {
  it("respects the byte cap", () => {
    expect(isOverSize(0, 100)).toBe(false);
    expect(isOverSize(100, 100)).toBe(false);
    expect(isOverSize(101, 100)).toBe(true);
  });
});

describe("classifyFile", () => {
  it("classifies code by extension and known names", () => {
    expect(classifyFile("lib.rs")).toEqual({ include: true, kind: "code" });
    expect(classifyFile("App.tsx")).toEqual({ include: true, kind: "code" });
    expect(classifyFile("Dockerfile")).toEqual({ include: true, kind: "code" });
    expect(classifyFile("Makefile")).toEqual({ include: true, kind: "code" });
  });
  it("classifies docs/config as text", () => {
    expect(classifyFile("README.md")).toEqual({ include: true, kind: "text" });
    expect(classifyFile("package.json")).toEqual({ include: true, kind: "text" });
    expect(classifyFile("config.toml")).toEqual({ include: true, kind: "text" });
  });
  it("excludes unknown / binary files", () => {
    expect(classifyFile("logo.png")).toEqual({ include: false, kind: "text" });
    expect(classifyFile("archive.zip")).toEqual({ include: false, kind: "text" });
  });
});
