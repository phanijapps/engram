// Incremental-scan manifest (RFC 0004 Slice 1).
//
// A sidecar JSON mapping each ingested file's relative path to a short content
// hash. On re-scan, a file whose hash is unchanged is skipped (not re-ingested).
// Stored next to the demo DB so it survives restarts.

import crypto from "node:crypto";
import fs from "node:fs/promises";

export type Manifest = Record<string, string>;

export function manifestPath(): string {
  const db = process.env.ENGRAM_DB ?? "demo-engram.db";
  return `${db}.scan-manifest.json`;
}

export async function loadManifest(): Promise<Manifest> {
  try {
    const raw = await fs.readFile(manifestPath(), "utf8");
    const parsed = JSON.parse(raw);
    return typeof parsed === "object" && parsed !== null ? (parsed as Manifest) : {};
  } catch {
    return {};
  }
}

export async function saveManifest(manifest: Manifest): Promise<void> {
  await fs.writeFile(manifestPath(), JSON.stringify(manifest, null, 2), "utf8");
}

/** Short, deterministic content hash (16 hex chars of sha256). */
export function hashContent(text: string): string {
  return crypto.createHash("sha256").update(text).digest("hex").slice(0, 16);
}

export function isUnchanged(manifest: Manifest, relPath: string, hash: string): boolean {
  return manifest[relPath] === hash;
}
