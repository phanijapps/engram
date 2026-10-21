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
  if (platform === "darwin" && arch === "arm64") return "@engramlabs/mcp-darwin-arm64";
  if (platform === "darwin" && arch === "x64") return "@engramlabs/mcp-darwin-x64";
  if (platform === "win32" && arch === "x64") return "@engramlabs/mcp-win32-x64";
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
    return arch === "x64" ? "@engramlabs/mcp-linux-x64-gnu" : "@engramlabs/mcp-linux-arm64-gnu";
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
  // Prefer the -full variant (local fastembed embeddings) when installed at
  // the launcher's version; fall back to the slim platform package.
  // ENGRAM_NO_FULL=1 forces slim even when -full is installed (e.g. to skip
  // the first-use model download on constrained machines).
  const wantFull = !process.env.ENGRAM_NO_FULL;
  const norm = (v) => String(v).replace(/^v/, "");
  const tryManifest = (name) => {
    try {
      const m = require.resolve(`${name}/package.json`);
      const binVersion = norm(require(m).version);
      return binVersion === norm(LAUNCHER_VERSION) ? m : null;
    } catch {
      return null;
    }
  };
  let manifest = wantFull ? tryManifest(`${pkg}-full`) : null;
  if (manifest) process.stderr.write("engram-mcp: using -full binary (local embeddings)\n");
  if (!manifest) {
    manifest = tryManifest(pkg);
  }
  if (!manifest) {
    fail(
      `platform package ${pkg} is not installed.\n` +
        `This should have been installed automatically as an optionalDependency.\n` +
        `Fix: npm install @engramlabs/mcp --force  (or: npm install ${pkg})`
    );
  }
  // Version lockstep: the platform package must match the launcher version.
  // Both sides are normalized (leading `v` stripped) — releases before
  // 0.1.8 stamped platform packages with the git tag's `v` prefix
  // ("v0.1.7-alpha"), which npm tolerates on install but a literal string
  // compare rejects. Normalizing keeps those already-installed packages
  // working instead of forcing a reinstall. (tryManifest already enforced
  // the same lockstep when choosing between -full and slim.)
  const binPath = path.join(path.dirname(manifest), BIN_NAME);
  if (!fs.existsSync(binPath)) {
    fail(`platform package is present but the binary is missing: ${binPath}`);
  }
  return binPath;
}

// Subcommands that do NOT need the binary.
const argv = process.argv.slice(2);
if (argv[0] === "install" && (argv[1] === "--client" || argv.length === 1)) {
  require("./install.js").install(argv.slice(1));
  process.exit(0);
}
if (argv[0] === "install" && argv[1] === "--agent") {
  require("./agent-install.js").agentInstall(argv.slice(1));
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
