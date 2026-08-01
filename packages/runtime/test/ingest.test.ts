import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import type { NativeProviderTransport } from "@engram/node";

import { parseIngestArgs, runIngest } from "../src/ingest/cli.js";

/** A mock transport: only `scan` is exercised; the rest are no-op stubs. */
function mockTransport(
  scanImpl: () => Promise<unknown> = async () => ({ scanned: 1, entities: 1 })
): NativeProviderTransport {
  return {
    capabilities: vi.fn(async () => ({})),
    recall: vi.fn(async () => ({})),
    write: vi.fn(async () => ({})),
    scan: vi.fn(scanImpl),
    consolidate: vi.fn(async () => ({})),
    putEntity: vi.fn(async () => ({})),
    batchIngest: vi.fn(async () => ({}))
  } as unknown as NativeProviderTransport;
}

describe("runIngest dispatch", () => {
  it("one-shot: calls scan once with {path, scope}", async () => {
    const t = mockTransport();
    await runIngest({
      transport: t,
      path: "/repo",
      scope: { tenant: "t", workspace: "w" }
    });
    expect(t.scan).toHaveBeenCalledTimes(1);
    expect(t.scan).toHaveBeenCalledWith({
      path: "/repo",
      scope: { tenant: "t", workspace: "w" }
    });
  });

  it("every unset or <= 0 is one-shot", async () => {
    for (const every of [undefined, 0] as const) {
      const t = mockTransport();
      await runIngest({
        transport: t,
        path: "/r",
        scope: { tenant: "t" },
        ...(every !== undefined ? { every } : {})
      });
      expect(t.scan).toHaveBeenCalledTimes(1);
    }
  });

  it("one-shot scan error propagates (bin exits non-zero)", async () => {
    const t = mockTransport(async () => {
      throw new Error("boom");
    });
    await expect(
      runIngest({ transport: t, path: "/r", scope: { tenant: "t" } })
    ).rejects.toThrow("boom");
  });
});

describe("runIngest periodic (fake timers)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("every=50: schedules with no immediate run; 3 scans over 150ms", async () => {
    const t = mockTransport();
    const handle = await runIngest({
      transport: t,
      path: "/r",
      scope: { tenant: "t" },
      every: 50
    });
    expect(handle).toBeDefined();
    expect(t.scan).toHaveBeenCalledTimes(0); // cron-like: no immediate run

    await vi.advanceTimersByTimeAsync(150);
    expect(t.scan).toHaveBeenCalledTimes(3); // at 50, 100, 150

    handle?.stop();
  });
});

describe("parseIngestArgs", () => {
  it("parses required + optional flags", () => {
    const a = parseIngestArgs([
      "--config",
      '{"a":1}',
      "--path",
      "/r",
      "--tenant",
      "t",
      "--workspace",
      "w",
      "--every",
      "50"
    ]);
    expect(a).toEqual({
      config: '{"a":1}',
      path: "/r",
      tenant: "t",
      workspace: "w",
      every: 50
    });
  });

  it("omits workspace/every when unset", () => {
    const a = parseIngestArgs(["--config", "{}", "--path", "/r", "--tenant", "t"]);
    expect(a).toEqual({ config: "{}", path: "/r", tenant: "t" });
  });

  it("rejects missing required flags", () => {
    expect(() => parseIngestArgs(["--path", "/r"])).toThrow(/required/);
  });

  it("rejects negative or non-numeric --every", () => {
    const base = ["--config", "{}", "--path", "/r", "--tenant", "t"];
    // `--every=-5` (parseArgs passes the value through; space-separated `-5` is
    // an ambiguous-flag error at the parseArgs layer, which also rejects it).
    expect(() => parseIngestArgs([...base, "--every=-5"])).toThrow(/non-negative/);
    expect(() => parseIngestArgs([...base, "--every", "abc"])).toThrow(/non-negative/);
  });
});
