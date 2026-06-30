// Pure per-file decisions for the scan job (RFC 0004 Slice 1).
//
// These functions are filesystem-free and deterministic so the scan's safety +
// filtering logic is unit-testable without touching disk. The walker (scan.ts)
// composes them after canonicalizing paths.

import path from "node:path";

// Directory names never descended into, and file suffixes never read.
const DENY_DIRS = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  "build",
  "coverage",
  ".fastembed_cache",
  "__pycache__",
  ".venv",
  "venv",
  ".next",
  ".cache",
  ".idea",
  ".vscode",
]);
const DENY_FILE_RE = /\.(db|sqlite|sqlite3|node|log|pyc|lock)$/i;

// Secret-bearing files: skipped by default, never read past name/size.
const SECRET_FILE_RE = /(^\.env(\..+)?$)|\.(key|pem|cert|crt|p12|pfx)$/i;
const SECRET_NAMES = new Set([
  "id_rsa",
  "id_dsa",
  "id_ecdsa",
  "id_ed25519",
]);
// `.env.*` templates that document variables without holding secrets.
const SAFE_TEMPLATES = new Set([
  ".env.example",
  ".env.sample",
  ".env.template",
  ".env.defaults",
  ".env.schema",
]);

const CODE_EXTENSIONS = new Set([
  "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts",
  "scala", "clj", "cljs", "ex", "exs", "erl", "hs", "ml", "mli", "lua", "php",
  "pl", "pm", "r", "rb", "sh", "bash", "zsh", "fish", "ps1", "c", "h", "cpp",
  "cc", "cxx", "hpp", "hxx", "cs", "swift", "dart", "vue", "svelte", "sql",
  "proto", "graphql", "gradle", "groovy", "vim",
]);
const CODE_NAMES = new Set([
  "dockerfile", "makefile", "rakefile", "gemfile", "cmake", "justfile",
]);
const TEXT_EXTENSIONS = new Set([
  "md", "markdown", "txt", "rst", "org", "tex", "adoc", "yml", "yaml", "json",
  "toml", "xml", "html", "htm", "css", "scss", "sass", "less", "ini", "cfg",
  "conf", "properties", "csv", "tsv",
]);

export type FileKind = "code" | "text";

/** True if `target` is `root` or inside it (after resolution). */
export function isWithinRoot(target: string, root: string): boolean {
  const rel = path.relative(path.resolve(root), path.resolve(target));
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

/** True if any path segment is a deny dir, or the file suffix is denylisted. */
export function isDenylisted(relPath: string): boolean {
  const segs = relPath.split(/[/\\]/);
  if (segs.some((s) => DENY_DIRS.has(s))) return true;
  const base = segs[segs.length - 1] ?? "";
  return DENY_FILE_RE.test(base);
}

/** True if the file name looks like a credential/secret carrier. */
export function isSecretFile(name: string): boolean {
  const base = path.basename(name).toLowerCase();
  if (SAFE_TEMPLATES.has(base)) return false;
  return SECRET_FILE_RE.test(base) || SECRET_NAMES.has(base);
}

/** True if the file is larger than the configured per-file cap. */
export function isOverSize(size: number, maxBytes: number): boolean {
  return size > maxBytes;
}

/** Classify a file by name: include + code/text. Unknown → not included. */
export function classifyFile(name: string): { include: boolean; kind: FileKind } {
  const base = path.basename(name).toLowerCase();
  if (CODE_NAMES.has(base)) return { include: true, kind: "code" };
  const ext = base.includes(".") ? base.slice(base.lastIndexOf(".") + 1) : "";
  if (CODE_EXTENSIONS.has(ext)) return { include: true, kind: "code" };
  if (TEXT_EXTENSIONS.has(ext)) return { include: true, kind: "text" };
  return { include: false, kind: "text" };
}

export const SCAN_DEFAULTS = {
  maxBytes: 1024 * 1024, // 1 MiB per file
};
