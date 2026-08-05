//! Ingest routes — drive + monitor repo/document scans from the Control Center.
//!
//! `POST /ingest/scan` spawns the `engram-ingest` CLI as a **child process** (not
//! in-process, not a worker_thread): the sync facade's `block_on` panics under a
//! nested runtime (docs/guides/how-to/extend-storage.md:213), so scan must run in
//! its own process — the repo's established pattern. The CLI emits a `ScanSummary`
//! JSON line on stdout (packages/runtime/src/ingest/cli.ts); this route parses the
//! **last** stdout line on a 0-exit. Progress/logs land on stderr.
//!
//! Jobs live in an in-process `Map` (single BFF; lost on restart — acceptable for a
//! dev Control Center). No `node:sqlite` on this path — the scan writes to the
//! agentzero store via the facade (the store the user is viewing).

import { Hono } from "hono";
import { spawn, type ChildProcess } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

import type { VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
import { buildConfigJson, getProvider } from "../engram/provider.ts";

/** Mirrors `packages/runtime/src/shared/config.ts::ScanSummary` (loosely typed). */
export interface ScanSummary {
  scanned?: number;
  ingested?: number;
  unchanged?: number;
  skipped?: number;
  entities?: number;
  relationships?: number;
  errors?: number;
  git_remote?: string | null;
  git_branch?: string | null;
  git_sha?: string | null;
}

export interface IngestJob {
  jobId: string;
  status: "running" | "done" | "error";
  startedAt: number;
  summary?: ScanSummary;
  error?: string;
}

interface IngestJobInternal extends IngestJob {
  child?: ChildProcess;
  stdoutChunks: Buffer[];
  stderrChunks: Buffer[];
}

/** Spawns a command + returns the ChildProcess. Injectable so tests fake the scan. */
export type IngestSpawner = (cmd: string, args: string[]) => ChildProcess;

const KINDS = new Set(["code", "doc", "auto"]);
const jobs = new Map<string, IngestJobInternal>();
let jobCounter = 0;

function msg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Resolves the `engram-ingest` bin path. `@engram/runtime`'s `exports` map blocks
 * deep imports, so resolve the package main + derive `dist/ingest/bin.js` (the
 * `files: ["dist"]` publish includes it). Uses `createRequire` (Node's native CJS
 * resolver) rather than `import.meta.resolve`, which vitest/vite shims.
 */
const require = createRequire(import.meta.url);
function ingestBin(): string {
  const runtimeMain = require.resolve("@engram/runtime");
  return join(dirname(runtimeMain), "ingest", "bin.js");
}

export function ingestRoute(
  cfg: VizConfig,
  deps: { spawn?: IngestSpawner } = {},
): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);
  const spawnFn: IngestSpawner =
    deps.spawn ??
    ((cmd, args) => spawn(cmd, args, { stdio: ["ignore", "pipe", "pipe"] }));

  app.post("/ingest/scan", async (c) => {
    let body: { root?: unknown; kind?: unknown };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "invalid JSON body" }, 400);
    }

    const root = typeof body.root === "string" ? body.root.trim() : "";
    if (!root) return c.json({ error: "root path required" }, 422);

    // `kind` is validated + surfaced to the UI; the CLI scans uniformly until the
    // T-doc slice wires code/doc selection. Accepted now for forward-compatibility.
    const kind = body.kind;
    if (kind !== undefined && (typeof kind !== "string" || !KINDS.has(kind))) {
      return c.json({ error: "kind must be one of: code, doc, auto" }, 422);
    }

    let bin: string;
    try {
      bin = ingestBin();
    } catch (err) {
      return c.json(
        { error: `engram-ingest bin not resolvable (is @engram/runtime built?): ${msg(err)}` },
        500,
      );
    }

    const jobId = `job-${++jobCounter}`;
    const args = [
      bin,
      "--config",
      buildConfigJson(cfg),
      "--path",
      root,
      "--tenant",
      scope.tenant,
      ...(scope.workspace ? ["--workspace", scope.workspace] : []),
    ];

    const child = spawnFn("node", args);
    const job: IngestJobInternal = {
      jobId,
      status: "running",
      startedAt: Date.now(),
      stdoutChunks: [],
      stderrChunks: [],
    };
    job.child = child;
    jobs.set(jobId, job);

    child.stdout?.on("data", (chunk: Buffer) => job.stdoutChunks.push(chunk));
    child.stderr?.on("data", (chunk: Buffer) => job.stderrChunks.push(chunk));
    child.on("exit", (code) => {
      const stdout = Buffer.concat(job.stdoutChunks).toString("utf8").trim();
      const stderr = Buffer.concat(job.stderrChunks).toString("utf8").trim();
      if (code === 0) {
        const lastLine = stdout.split("\n").filter(Boolean).pop() ?? "";
        try {
          job.summary = JSON.parse(lastLine) as ScanSummary;
          job.status = "done";
        } catch {
          job.status = "error";
          job.error = `scan exited 0 but produced no JSON summary${
            stderr ? `: ${stderr}` : ""
          }`;
        }
      } else {
        job.status = "error";
        job.error = stderr || `scan exited with code ${code}`;
      }
      delete job.child;
    });
    child.on("error", (err) => {
      job.status = "error";
      job.error = `failed to start scan: ${msg(err)}`;
      delete job.child;
    });

    return c.json({ jobId }, 202);
  });

  app.get("/ingest/jobs/:jobId", (c) => {
    const job = jobs.get(c.req.param("jobId"));
    if (!job) return c.json({ error: "job not found" }, 404);
    return c.json({
      jobId: job.jobId,
      status: job.status,
      startedAt: job.startedAt,
      ...(job.summary !== undefined ? { summary: job.summary } : {}),
      ...(job.error !== undefined ? { error: job.error } : {}),
    });
  });

  app.get("/ingest/counts", async (c) => {
    try {
      // scopeCounts (CommunityQuery) — real, agentzero-scoped counts. NOT
      // diagnostics().record_counts, which is scoped to a fixed `engram-diagnostics`
      // tenant and returns 0 for the user's scope. sources/chunks aren't available
      // scoped without a Rust port — deferred (the job ScanSummary shows the
      // immediate scan result meanwhile).
      const counts = await getProvider(cfg).scopeCounts(scope);
      return c.json({
        entities: counts.entities,
        relationships: counts.relationships,
        memories: counts.memories,
        beliefs: counts.beliefs,
        hierarchyNodes: counts.hierarchyNodes,
        hierarchyRelations: counts.hierarchyRelations,
      });
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  return app;
}
