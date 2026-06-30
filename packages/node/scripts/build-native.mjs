#!/usr/bin/env node
// Builds the engram-node N-API cdylib and places it where @engram/node loads it.
//
// Local single-platform build: `cargo build --release` then place the cdylib
// artifact as `engram_node.node`. This is the minimal path that produces a
// loadable addon for the demo; multi-triple prebuilds / packaging via
// @napi-rs/cli are future work (see docs/specs/napi-bridge-completion).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
// scripts/ -> node/ -> packages/ -> repo root
const repoRoot = join(here, "..", "..", "..");
const pkgRoot = join(here, "..");

const artifactByPlatform = {
  linux: "libengram_node.so",
  darwin: "libengram_node.dylib",
  win32: "engram_node.dll",
};
const artifact = artifactByPlatform[process.platform];
if (!artifact) {
  throw new Error(`unsupported platform: ${process.platform}`);
}

console.log(`cargo build --release -p engram-node (${process.platform})`);
// No shell, no interpolation: argv is a fixed literal.
execFileSync("cargo", ["build", "--release", "-p", "engram-node"], {
  stdio: "inherit",
  cwd: repoRoot,
});

const src = join(repoRoot, "target", "release", artifact);
const dest = join(pkgRoot, "engram_node.node");
if (!existsSync(src)) {
  throw new Error(`build artifact not found: ${src}`);
}
copyFileSync(src, dest);
console.log(`placed native addon: ${dest}`);
