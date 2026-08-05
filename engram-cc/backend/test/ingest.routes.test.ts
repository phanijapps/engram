//! Ingest routes — scan-job lifecycle (fake spawn, no real child) + live counts.
//! Proves: POST /ingest/scan → 202 {jobId}; the job transitions running→done with
//! the parsed ScanSummary on a 0-exit, or →error on non-zero/missing-JSON; input
//! validation (422); 404 for unknown jobs. /ingest/counts returns the record_counts
//! subset over the live store.

import { describe, it, expect } from "vitest";
import { existsSync } from "node:fs";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import type { ChildProcess } from "node:child_process";

import { dbPath, loadConfig } from "../src/config.ts";
import { ingestRoute, type IngestSpawner } from "../src/routes/ingest.ts";

/** A fake ChildProcess: emits stdout/stderr (as Buffer, like a real pipe) then
 *  `exit`s with `code`. The route only touches .stdout/.stderr/.on('exit'|'error'). */
function fakeChild(opts: {
  stdout?: string;
  stderr?: string;
  code?: number;
}): ChildProcess {
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const child = Object.assign(new EventEmitter(), { stdout, stderr, pid: 12345 });
  setImmediate(() => {
    stdout.write(Buffer.from(opts.stdout ?? "", "utf8"));
    stdout.end();
    stderr.write(Buffer.from(opts.stderr ?? "", "utf8"));
    stderr.end();
    child.emit("exit", opts.code ?? 0);
  });
  return child as unknown as ChildProcess;
}

const scanJson = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

describe("ingest routes — scan job lifecycle (fake spawn)", () => {
  it("POST /ingest/scan → 202 {jobId}; job → done with parsed summary", async () => {
    const spawn: IngestSpawner = () =>
      fakeChild({ stdout: `{"scanned":3,"ingested":2,"entities":5,"relationships":4}\n` });
    const app = ingestRoute(loadConfig(), { spawn });

    const res = await app.request("/ingest/scan", scanJson({ root: "/tmp/x", kind: "auto" }));
    expect(res.status).toBe(202);
    const { jobId } = (await res.json()) as { jobId: string };
    expect(jobId).toMatch(/^job-\d+$/);

    await new Promise((r) => setTimeout(r, 20)); // fake child exits on setImmediate
    const j = await (await app.request(`/ingest/jobs/${jobId}`)).json();
    expect(j.status).toBe("done");
    expect(j.summary.entities).toBe(5);
    expect(j.summary.relationships).toBe(4);
  });

  it("records error on non-zero exit (uses stderr)", async () => {
    const spawn: IngestSpawner = () => fakeChild({ stderr: "boom: bad path\n", code: 2 });
    const app = ingestRoute(loadConfig(), { spawn });

    const { jobId } = (await (await app.request("/ingest/scan", scanJson({ root: "/tmp/x" }))).json()) as {
      jobId: string;
    };
    await new Promise((r) => setTimeout(r, 20));
    const j = await (await app.request(`/ingest/jobs/${jobId}`)).json();
    expect(j.status).toBe("error");
    expect(j.error).toContain("boom");
  });

  it("records error on 0-exit with no JSON summary line", async () => {
    const spawn: IngestSpawner = () => fakeChild({ stdout: "not json at all\n", code: 0 });
    const app = ingestRoute(loadConfig(), { spawn });

    const { jobId } = (await (await app.request("/ingest/scan", scanJson({ root: "/tmp/x" }))).json()) as {
      jobId: string;
    };
    await new Promise((r) => setTimeout(r, 20));
    const j = await (await app.request(`/ingest/jobs/${jobId}`)).json();
    expect(j.status).toBe("error");
  });

  it("rejects empty root (422) and invalid kind (422)", async () => {
    const app = ingestRoute(loadConfig(), { spawn: () => fakeChild({}) });
    const noRoot = await app.request("/ingest/scan", scanJson({}));
    expect(noRoot.status).toBe(422);
    const badKind = await app.request("/ingest/scan", scanJson({ root: "/tmp/x", kind: "nope" }));
    expect(badKind.status).toBe(422);
  });

  it("GET /ingest/jobs/unknown → 404", async () => {
    const app = ingestRoute(loadConfig(), { spawn: () => fakeChild({}) });
    const res = await app.request("/ingest/jobs/job-does-not-exist");
    expect(res.status).toBe(404);
  });

  it("parses the LAST stdout line as the summary (tolerates leading noise)", async () => {
    const spawn: IngestSpawner = () =>
      fakeChild({ stdout: `engram-mcp: checkpoint...\n{"scanned":1,"entities":9}\n` });
    const app = ingestRoute(loadConfig(), { spawn });
    const { jobId } = (await (await app.request("/ingest/scan", scanJson({ root: "/tmp/x" }))).json()) as {
      jobId: string;
    };
    await new Promise((r) => setTimeout(r, 20));
    const j = await (await app.request(`/ingest/jobs/${jobId}`)).json();
    expect(j.status).toBe("done");
    expect(j.summary.entities).toBe(9);
  });
});

const cfg = loadConfig();
const ready = existsSync(dbPath(cfg));
describe.skipIf(!ready)("ingest counts (live agentzero store)", () => {
  it("/ingest/counts returns scoped counts (scopeCounts)", async () => {
    const res = await ingestRoute(cfg).request("/ingest/counts");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty("entities");
    expect(body).toHaveProperty("relationships");
    expect(body).toHaveProperty("memories");
    expect(body).toHaveProperty("beliefs");
    expect(body).toHaveProperty("hierarchyNodes");
    expect(body).toHaveProperty("hierarchyRelations");
  });
});
