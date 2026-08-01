import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import type { NativeProviderTransport } from "@engram/node";

import { parseMaintainArgs, runMaintain } from "../src/maintenance/cli.js";

/** A mock transport: only `consolidate` is exercised; the rest are no-op stubs. */
function mockTransport(
  consolidateImpl: () => Promise<unknown> = async () => ({
    status: "completed",
    tasks: []
  })
): NativeProviderTransport {
  return {
    capabilities: vi.fn(async () => ({})),
    recall: vi.fn(async () => ({})),
    write: vi.fn(async () => ({})),
    scan: vi.fn(async () => ({})),
    consolidate: vi.fn(consolidateImpl),
    putEntity: vi.fn(async () => ({})),
    batchIngest: vi.fn(async () => ({}))
  } as unknown as NativeProviderTransport;
}

describe("runMaintain dispatch", () => {
  it("one-shot: calls consolidate once with {scope}", async () => {
    const t = mockTransport();
    await runMaintain({
      transport: t,
      scope: { tenant: "t", workspace: "w" }
    });
    expect(t.consolidate).toHaveBeenCalledTimes(1);
    expect(t.consolidate).toHaveBeenCalledWith({
      scope: { tenant: "t", workspace: "w" }
    });
  });

  it("forwards dryRun when set; omits the key when unset", async () => {
    const t = mockTransport();
    await runMaintain({ transport: t, scope: { tenant: "t" }, dryRun: true });
    expect(t.consolidate).toHaveBeenCalledWith({
      scope: { tenant: "t" },
      dryRun: true
    });

    const t2 = mockTransport();
    await runMaintain({ transport: t2, scope: { tenant: "t" } });
    expect(t2.consolidate).toHaveBeenCalledWith({ scope: { tenant: "t" } });
  });

  it("forwards since/until when set", async () => {
    const t = mockTransport();
    const since = "2026-01-01T00:00:00Z";
    const until = "2026-02-01T00:00:00Z";
    await runMaintain({ transport: t, scope: { tenant: "t" }, since, until });
    expect(t.consolidate).toHaveBeenCalledWith({
      scope: { tenant: "t" },
      since,
      until
    });
  });

  it("every unset or <= 0 is one-shot", async () => {
    for (const every of [undefined, 0] as const) {
      const t = mockTransport();
      await runMaintain({
        transport: t,
        scope: { tenant: "t" },
        ...(every !== undefined ? { every } : {})
      });
      expect(t.consolidate).toHaveBeenCalledTimes(1);
    }
  });

  it("one-shot consolidate error propagates (bin exits non-zero)", async () => {
    const t = mockTransport(async () => {
      throw new Error("boom");
    });
    await expect(
      runMaintain({ transport: t, scope: { tenant: "t" } })
    ).rejects.toThrow("boom");
  });
});

describe("runMaintain periodic (fake timers)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("every=50: 3 consolidations over 150ms; no immediate run", async () => {
    const t = mockTransport();
    const handle = await runMaintain({
      transport: t,
      scope: { tenant: "t" },
      every: 50
    });
    expect(handle).toBeDefined();
    try {
      expect(t.consolidate).toHaveBeenCalledTimes(0);
      await vi.advanceTimersByTimeAsync(150);
      expect(t.consolidate).toHaveBeenCalledTimes(3);
    } finally {
      handle?.stop();
    }
  });

  it("stop() clears the interval — no further runs after stop", async () => {
    const t = mockTransport();
    const handle = await runMaintain({
      transport: t,
      scope: { tenant: "t" },
      every: 50
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(t.consolidate).toHaveBeenCalledTimes(2);
    handle?.stop();
    await vi.advanceTimersByTimeAsync(200);
    expect(t.consolidate).toHaveBeenCalledTimes(2);
  });

  it("periodic consolidate errors are swallowed — the schedule survives", async () => {
    const t = mockTransport(async () => {
      throw new Error("boom");
    });
    const handle = await runMaintain({
      transport: t,
      scope: { tenant: "t" },
      every: 50
    });
    try {
      await vi.advanceTimersByTimeAsync(150);
      expect(t.consolidate).toHaveBeenCalledTimes(3);
    } finally {
      handle?.stop();
    }
  });
});

describe("parseMaintainArgs", () => {
  it("parses required + optional flags (--dry-run is a boolean)", () => {
    const a = parseMaintainArgs([
      "--config",
      "{}",
      "--tenant",
      "t",
      "--workspace",
      "w",
      "--dry-run",
      "--since",
      "2026-01-01T00:00:00Z",
      "--until",
      "2026-02-01T00:00:00Z",
      "--every",
      "50"
    ]);
    expect(a).toEqual({
      config: "{}",
      tenant: "t",
      workspace: "w",
      dryRun: true,
      since: "2026-01-01T00:00:00Z",
      until: "2026-02-01T00:00:00Z",
      every: 50
    });
  });

  it("omits optional flags when unset", () => {
    const a = parseMaintainArgs(["--config", "{}", "--tenant", "t"]);
    expect(a).toEqual({ config: "{}", tenant: "t" });
  });

  it("rejects missing required flags", () => {
    expect(() => parseMaintainArgs(["--config", "{}"])).toThrow(/required/);
  });

  it("rejects negative, non-numeric, or non-integer --every", () => {
    const base = ["--config", "{}", "--tenant", "t"];
    expect(() => parseMaintainArgs([...base, "--every=-5"])).toThrow(/non-negative/);
    expect(() => parseMaintainArgs([...base, "--every", "abc"])).toThrow(/non-negative/);
    expect(() => parseMaintainArgs([...base, "--every", "50.5"])).toThrow(/non-negative/);
  });
});
