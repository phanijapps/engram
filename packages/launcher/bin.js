#!/usr/bin/env node
"use strict";
/**
 * engram-mcp launcher (RFC-0021).
 *
 * Resolves the platform binary package for this machine and spawns it with
 * stdio inherited — the whole MCP contract is newline-delimited JSON-RPC
 * over stdio, so this launcher NEVER touches the stream.
 *
 * Trust properties:
 *   - zero dependencies (nothing to typosquat or audit)
 *   - no install scripts (nothing runs at npm install time)
 *   - hard failure with a precise fix on: missing platform package,
 *     unsupported platform, or launcher/binary version mismatch
 *   - ENGRAM_MCP_BIN env override for development and vendored installs
 */

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const LAUNCHER_VERSION = require("./package.json").version;
const BIN_NAME = process.platform === "win32" ? "engram-mcp.exe" : "engram-mcp";

function fail(message) {
  process.stderr.write(`engram-mcp: ${message}\n`);
  process.exit(1);
}

/** Map process.platform/process.arch to the platform package suffix. */
function platformPackage() {
  const { platform, arch } = process;
  if (platform === "darwin" && arch === "arm64") return "@engram/mcp-darwin-arm64";
  if (platform === "darwin" && arch === "x64") return "@engram/mcp-darwin-x64";
  if (platform === "win32" && arch === "x64") return "@engram/mcp-win32-x64";
  if (platform === "linux" && (arch === "x64" || arch === "arm64")) {
    // v1 ships gnu only (RFC-0021 non-goal: musl). Detect the runtime libc
    // so musl users get a precise error instead of a broken binary.
    const isMusl = (() => {
      try {
        const report = process.report.getSyncReport();
        return JSON.parse(report).header?.glibcVersionRuntime == null;
      } catch {
        return false; // assume gnu when detection is unavailable
      }
    })();
    if (isMusl) {
      fail(
        `musl libc is not supported in v1 (see RFC-0021). ` +
          `Install the gnu build via Docker/glibc, or +1 the musl issue: ` +
          `https://github.com/phanijapps/engram/issues`
      );
    }
    return arch === "x64" ? "@engram/mcp-linux-x64-gnu" : "@engram/mcp-linux-arm64-gnu";
  }
  fail(
    `unsupported platform ${platform}-${arch}. ` +
      `Supported: darwin-arm64, darwin-x64, linux-x64-gnu, linux-arm64-gnu, win32-x64.`
  );
}

/** Resolve the binary path: env override → platform package. */
function resolveBinary() {
  if (process.env.ENGRAM_MCP_BIN) {
    const p = process.env.ENGRAM_MCP_BIN;
    if (!fs.existsSync(p)) fail(`ENGRAM_MCP_BIN points at a missing file: ${p}`);
    return p;
  }
  const pkg = platformPackage();
  let manifest;
  try {
    manifest = require.resolve(`${pkg}/package.json`);
  } catch {
    fail(
      `platform package ${pkg} is not installed.\n` +
        `This should have been installed automatically as an optionalDependency.\n` +
        `Fix: npm install engram-mcp --force  (or: npm install ${pkg})`
    );
  }
  const binPath = path.join(path.dirname(manifest), BIN_NAME);
  if (!fs.existsSync(binPath)) {
    fail(`platform package ${pkg} is present but the binary is missing: ${binPath}`);
  }
  // Version lockstep: the platform package must match the launcher version.
  const binVersion = require(manifest).version;
  if (binVersion !== LAUNCHER_VERSION) {
    fail(
      `version mismatch: launcher ${LAUNCHER_VERSION} != platform package ${binVersion}.\n` +
        `Mixed versions are unsupported. Fix: npm install engram-mcp@${LAUNCHER_VERSION}`
    );
  }
  return binPath;
}

// Subcommands that do NOT need the binary.
const argv = process.argv.slice(2);
if (argv[0] === "install") {
  require("./install.js").install(argv.slice(1));
  process.exit(0);
}

const bin = resolveBinary();
const child = spawn(bin, argv, { stdio: "inherit" });
child.on("error", (err) => fail(`failed to start binary: ${err.message}`));
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code == null ? 1 : code);
  }
});
