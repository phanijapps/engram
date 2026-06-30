// Filesystem walker for the scan job (RFC 0004 Slice 1).
//
// Walks a root, applies .gitignore (root + per-directory, scoped) plus the pure
// decisions from decide.ts, and yields one ScanFile per file with an include/skip
// verdict + reason. It never reads file contents — only stats — so secret and
// oversized files are rejected on name/size alone. `safeReadText` is the single
// place content is read, and it enforces root confinement against symlink escape.

import fs from "node:fs/promises";
import type { Dirent } from "node:fs";
import ignore from "ignore";
import path from "node:path";
import {
  classifyFile,
  isDenylisted,
  isOverSize,
  isSecretFile,
  isWithinRoot,
  SCAN_DEFAULTS,
  type FileKind,
} from "./decide.js";

export type ScanFile = {
  absPath: string;
  relPath: string;
  size: number;
  include: boolean;
  kind: FileKind;
  reason?: string;
};

export type ScanOptions = {
  maxBytes?: number;
  signal?: AbortSignal;
};

const toPosix = (p: string): string => p.split(path.sep).join("/");

/**
 * Scope a directory's .gitignore patterns to its path relative to the root.
 * Note: negation (`!`) and root-anchored (`/`) gitignore edge cases are
 * intentionally simplified here (patterns are prefixed by directory); good
 * enough for a demo, not a full gitignore engine.
 */
function scopeGitignore(contents: string, relDir: string): string[] {
  const prefix = relDir === "" || relDir === "." ? "" : `${relDir}/`;
  return contents
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#"))
    .flatMap((l) => (l.startsWith("!") ? [`!${prefix}${l.slice(1)}`] : [`${prefix}${l}`]));
}

async function* walkDir(
  dirAbs: string,
  rootAbs: string,
  ig: ReturnType<typeof ignore>,
  maxBytes: number,
  signal?: AbortSignal
): AsyncGenerator<ScanFile> {
  if (signal?.aborted) return;
  let entries: Dirent[];
  try {
    entries = await fs.readdir(dirAbs, { withFileTypes: true });
  } catch {
    return; // unreadable directory → skip silently
  }

  const relDir = toPosix(path.relative(rootAbs, dirAbs));
  try {
    const gitignore = await fs.readFile(path.join(dirAbs, ".gitignore"), "utf8");
    ig.add(scopeGitignore(gitignore, relDir));
  } catch {
    // no .gitignore here — fine
  }

  for (const entry of entries) {
    if (entry.name === ".gitignore") continue;
    const abs = path.join(dirAbs, entry.name);
    const rel = toPosix(path.relative(rootAbs, abs));

    // `withFileTypes` reports symlinks as neither directory nor file, so
    // symlinked entries (including symlinked directories) fall through and are
    // skipped — the scan never follows links, so it cannot descend out of root.
    if (entry.isDirectory()) {
      if (isDenylisted(rel) || ig.ignores(rel)) continue;
      yield* walkDir(abs, rootAbs, ig, maxBytes, signal);
    } else if (entry.isFile()) {
      if (isDenylisted(rel)) {
        yield { absPath: abs, relPath: rel, size: 0, include: false, kind: "text", reason: "denylisted" };
        continue;
      }
      if (ig.ignores(rel)) {
        yield { absPath: abs, relPath: rel, size: 0, include: false, kind: "text", reason: "gitignored" };
        continue;
      }
      if (isSecretFile(entry.name)) {
        yield { absPath: abs, relPath: rel, size: 0, include: false, kind: "text", reason: "secret" };
        continue;
      }
      let stat;
      try {
        stat = await fs.stat(abs);
      } catch {
        yield { absPath: abs, relPath: rel, size: 0, include: false, kind: "text", reason: "stat-failed" };
        continue;
      }
      if (isOverSize(stat.size, maxBytes)) {
        yield { absPath: abs, relPath: rel, size: stat.size, include: false, kind: "text", reason: "oversized" };
        continue;
      }
      const { include, kind } = classifyFile(entry.name);
      if (!include) {
        yield { absPath: abs, relPath: rel, size: stat.size, include: false, kind, reason: "not-text-or-code" };
        continue;
      }
      yield { absPath: abs, relPath: rel, size: stat.size, include: true, kind };
    }
  }
}

/** Walks `root`, yielding a ScanFile verdict per file (no contents read here). */
export async function* walk(root: string, opts: ScanOptions = {}): AsyncGenerator<ScanFile> {
  const maxBytes = opts.maxBytes ?? SCAN_DEFAULTS.maxBytes;
  const rootAbs = path.resolve(root);
  yield* walkDir(rootAbs, rootAbs, ignore().add([]), maxBytes, opts.signal);
}

/**
 * Reads file text after enforcing root confinement against symlink escape.
 * Throws if the resolved path is outside `root`; never reads otherwise.
 */
export async function safeReadText(root: string, absPath: string): Promise<string> {
  const rootAbs = path.resolve(root);
  const real = await fs.realpath(absPath);
  if (!isWithinRoot(real, rootAbs)) {
    throw new Error(`path escapes root: ${absPath}`);
  }
  return fs.readFile(real, "utf8");
}
