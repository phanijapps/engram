//! Singleton wrapper around the native ingest engine for repository scanning.
//!
//! Scans run on a background Rust thread inside the native engine; Node polls
//! `getScanJobJson` until `status !== "running"`. The ingest engine opens the
//! same SQLite db file as the knowledge engine, so scanned data is visible to
//! graph queries after `engine.invalidateCache()` is called.

import { loadNativeBinding } from "@engram/node";
import os from "node:os";
import path from "node:path";
import { engine } from "./engine.ts";

interface ScanJobState {
  status: "running" | "done" | "error";
  currentFile: string | null;
  processed: number;
  ingested: number;
  unchanged: number;
  skipped: number;
  errors: number;
  summary: {
    scanned: number;
    ingested: number;
    unchanged: number;
    skipped: number;
    entities: number;
    relationships: number;
    errors: number;
    git_remote?: string;
    git_branch?: string;
    git_sha?: string;
  } | null;
  error: string | null;
}

interface NativeIngestEngine {
  startScanJobJson(req: string): string;
  getScanJobJson(req: string): string;
}

function defaultDbPath(): string {
  const fromEnv = process.env.ENGRAM_DB;
  if (fromEnv && fromEnv.length > 0) return fromEnv;
  return path.join(os.homedir(), ".engram", "codegraph-mem-alpha.db");
}

const binding = loadNativeBinding();
const native = new (binding as unknown as {
  NativeIngestEngine: new (path: string | null) => NativeIngestEngine;
}).NativeIngestEngine(defaultDbPath());

/** Default scan policy + actor, matching the codegraph MCP server configuration. */
function scanRequest(root: string): string {
  return JSON.stringify({
    root,
    sourceName: "engram-viz",
    maxBytes: 0,
    scope: engine.scope,
    policy: {
      visibility: "workspace",
      retention: "durable",
      sensitivity: null,
      allowedUses: ["retrieval"],
      expiresAt: null,
      deleteMode: null,
    },
    actor: {
      id: "engram-viz",
      kind: "agent",
      displayName: "Engram Viz",
    },
    force: true,
  });
}

/** Starts a background repository scan; returns the job id immediately. */
export function startScan(root: string): { jobId: string } {
  const raw = native.startScanJobJson(scanRequest(root));
  return JSON.parse(raw) as { jobId: string };
}

/** Polls the state of a scan job. */
export function getScanJob(jobId: string): ScanJobState {
  const raw = native.getScanJobJson(JSON.stringify({ jobId }));
  return JSON.parse(raw) as ScanJobState;
}

/**
 * Starts a scan and resolves once it completes (success or error). Invalidates
 * the knowledge cache so subsequent graph reads observe the new data.
 */
export async function scanToCompletion(
  root: string,
): Promise<ScanJobState> {
  const { jobId } = startScan(root);
  // Poll every 400ms up to ~5 minutes.
  for (let i = 0; i < 750; i++) {
    await new Promise((r) => setTimeout(r, 400));
    const state = getScanJob(jobId);
    if (state.status !== "running") {
      engine.invalidateCache();
      return state;
    }
  }
  return { ...getScanJob(jobId), status: "error", error: "scan timed out" };
}
