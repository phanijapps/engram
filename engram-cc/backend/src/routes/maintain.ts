//! Maintenance routes — run + monitor pi-mono LLM maintenance (reflect-llm /
//! contradict-llm) + deterministic consolidate, and read beliefs/contradictions.
//!
//! `POST /maintain/run` spawns the `engram-maintain` CLI as a **child process**
//! (same nested-executor-safe pattern as `/ingest/scan` — the sync facade's
//! `block_on` panics under a nested runtime; the LLM op also blocks on a network
//! call, so it must NOT run on the BFF's event loop). The CLI emits one JSON
//! result line on stdout; this route parses the last line on a 0-exit.
//!
//! PRIVACY: reflect-llm / contradict-llm route the scope's memories/beliefs to a
//! third-party LLM (Anthropic by default; PI_PROVIDER=ollama for local). The route
//! itself does not gate this — disclosure + confirmation live in the UI; the
//! operator can also disable the LLM ops by leaving PI_PROVIDER unset (the CLI
//! then fails fast at call time, surfaced as a job error).

import { Hono } from "hono";
import { spawn, type ChildProcess } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

import type { VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
import { buildConfigJson, getProvider } from "../engram/provider.ts";

export type MaintainOp = "reflect-llm" | "contradict-llm" | "consolidate";
const OPS: readonly MaintainOp[] = ["reflect-llm", "contradict-llm", "consolidate"];

/** Mirrors the `engram-maintain` result shapes (reflection / contradiction / consolidate). */
export interface MaintainResult {
  memoriesRead?: number;
  beliefsRead?: number;
  beliefsWritten?: number;
  contradictionsWritten?: number;
  skipped?: number;
  status?: string;
  tasks?: unknown[];
  [key: string]: unknown;
}

export interface MaintainJob {
  jobId: string;
  op: MaintainOp;
  status: "running" | "done" | "error";
  startedAt: number;
  result?: MaintainResult;
  error?: string;
}

interface MaintainJobInternal extends MaintainJob {
  child?: ChildProcess;
  stdoutChunks: Buffer[];
  stderrChunks: Buffer[];
}

export type MaintainSpawner = (cmd: string, args: string[]) => ChildProcess;

const jobs = new Map<string, MaintainJobInternal>();
let jobCounter = 0;

function msg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Resolves the `engram-maintain` bin path (see ingest.ts for the exports-map note). */
const require = createRequire(import.meta.url);
function maintainBin(): string {
  const runtimeMain = require.resolve("@engram/runtime");
  return join(dirname(runtimeMain), "maintenance", "bin.js");
}

export function maintainRoute(
  cfg: VizConfig,
  deps: { spawn?: MaintainSpawner } = {},
): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);
  const spawnFn: MaintainSpawner =
    deps.spawn ??
    ((cmd, args) => spawn(cmd, args, { stdio: ["ignore", "pipe", "pipe"] }));

  app.post("/maintain/run", async (c) => {
    let body: { op?: unknown };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "invalid JSON body" }, 400);
    }
    const op = body.op;
    if (typeof op !== "string" || !OPS.includes(op as MaintainOp)) {
      return c.json({ error: `op must be one of: ${OPS.join(", ")}` }, 422);
    }

    let bin: string;
    try {
      bin = maintainBin();
    } catch (err) {
      return c.json(
        { error: `engram-maintain bin not resolvable (is @engram/runtime built?): ${msg(err)}` },
        500,
      );
    }

    const jobId = `mjob-${++jobCounter}`;
    const args = [
      bin,
      "--config",
      buildConfigJson(cfg),
      "--tenant",
      scope.tenant,
      ...(scope.workspace ? ["--workspace", scope.workspace] : []),
      "--op",
      op,
    ];

    const child = spawnFn("node", args);
    const job: MaintainJobInternal = {
      jobId,
      op: op as MaintainOp,
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
          job.result = JSON.parse(lastLine) as MaintainResult;
          job.status = "done";
        } catch {
          job.status = "error";
          job.error = `exited 0 but no JSON result${stderr ? `: ${stderr}` : ""}`;
        }
      } else {
        job.status = "error";
        job.error = stderr || `exited with code ${code}`;
      }
      delete job.child;
    });
    child.on("error", (err) => {
      job.status = "error";
      job.error = `failed to start: ${msg(err)}`;
      delete job.child;
    });

    return c.json({ jobId }, 202);
  });

  app.get("/maintain/jobs/:jobId", (c) => {
    const job = jobs.get(c.req.param("jobId"));
    if (!job) return c.json({ error: "job not found" }, 404);
    return c.json({
      jobId: job.jobId,
      op: job.op,
      status: job.status,
      startedAt: job.startedAt,
      ...(job.result !== undefined ? { result: job.result } : {}),
      ...(job.error !== undefined ? { error: job.error } : {}),
    });
  });

  app.get("/maintain/beliefs", async (c) => {
    try {
      return c.json(await getProvider(cfg).listBeliefs(scope));
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  app.get("/maintain/contradictions", async (c) => {
    try {
      return c.json(await getProvider(cfg).listContradictions(scope));
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  return app;
}
