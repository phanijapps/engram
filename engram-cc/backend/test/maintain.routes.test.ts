//! Maintain routes — run-job lifecycle (fake spawn) + live belief/contradiction
//! reads. Proves: POST /maintain/run validates op → 202 {jobId}; the job goes
//! running→done with the parsed result on 0-exit (or →error); /beliefs +
//! /contradictions return the facade lists over the live store.

import { describe, it, expect } from "vitest";
import { existsSync } from "node:fs";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import type { ChildProcess } from "node:child_process";

import { dbPath, loadConfig } from "../src/config.ts";
import { maintainRoute, type MaintainSpawner } from "../src/routes/maintain.ts";

function fakeChild(opts: {
  stdout?: string;
  stderr?: string;
  code?: number;
}): ChildProcess {
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const child = Object.assign(new EventEmitter(), { stdout, stderr, pid: 54321 });
  setImmediate(() => {
    stdout.write(Buffer.from(opts.stdout ?? "", "utf8"));
    stdout.end();
    stderr.write(Buffer.from(opts.stderr ?? "", "utf8"));
    stderr.end();
    child.emit("exit", opts.code ?? 0);
  });
  return child as unknown as ChildProcess;
}

const runJson = (op: unknown): RequestInit => ({
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ op }),
});

describe("maintain routes — run-job lifecycle (fake spawn)", () => {
  it("POST /maintain/run reflect-llm → 202; job → done with parsed result", async () => {
    const spawn: MaintainSpawner = () =>
      fakeChild({ stdout: `{"memoriesRead":3,"beliefsWritten":2,"skipped":0}\n` });
    const app = maintainRoute(loadConfig(), { spawn });

    const res = await app.request("/maintain/run", runJson("reflect-llm"));
    expect(res.status).toBe(202);
    const { jobId } = (await res.json()) as { jobId: string };
    expect(jobId).toMatch(/^mjob-\d+$/);

    await new Promise((r) => setTimeout(r, 20));
    const j = await (await app.request(`/maintain/jobs/${jobId}`)).json();
    expect(j.status).toBe("done");
    expect(j.op).toBe("reflect-llm");
    expect(j.result.beliefsWritten).toBe(2);
  });

  it("records error on non-zero exit", async () => {
    const spawn: MaintainSpawner = () => fakeChild({ stderr: "no ANTHROPIC_API_KEY\n", code: 1 });
    const app = maintainRoute(loadConfig(), { spawn });
    const { jobId } = (await (await app.request("/maintain/run", runJson("contradict-llm"))).json()) as {
      jobId: string;
    };
    await new Promise((r) => setTimeout(r, 20));
    const j = await (await app.request(`/maintain/jobs/${jobId}`)).json();
    expect(j.status).toBe("error");
    expect(j.error).toContain("ANTHROPIC_API_KEY");
  });

  it("rejects an invalid op (422) and 404s unknown jobs", async () => {
    const app = maintainRoute(loadConfig(), { spawn: () => fakeChild({}) });
    const bad = await app.request("/maintain/run", runJson("bogus"));
    expect(bad.status).toBe(422);
    const missing = await app.request("/maintain/jobs/nope");
    expect(missing.status).toBe(404);
  });
});

const cfg = loadConfig();
const ready = existsSync(dbPath(cfg));
describe.skipIf(!ready)("maintain reads (live agentzero store)", () => {
  it("/maintain/beliefs + /maintain/contradictions return arrays", async () => {
    const app = maintainRoute(cfg);
    const beliefs = await (await app.request("/maintain/beliefs")).json();
    expect(Array.isArray(beliefs)).toBe(true);
    const contras = await (await app.request("/maintain/contradictions")).json();
    expect(Array.isArray(contras)).toBe(true);
  });
});
