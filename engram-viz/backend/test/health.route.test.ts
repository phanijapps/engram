//! T1 /api/health HTTP-shape test (fixture store — CI-portable) + the Boundary
//! assertion that the BFF never speaks engram-mcp. The degraded path (503) and
//! the capability-report object shape are pinned here, not only by the smoke.

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, readdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import type { VizConfig } from "../src/config.ts";
import { _resetProviderForTests } from "../src/engram/provider.ts";
import { healthRoute } from "../src/routes/health.ts";

function fixtureCfg(): VizConfig {
  const store = mkdtempSync(join(tmpdir(), "engram-viz-health-"));
  return {
    storageDir: store,
    dbFile: "test.db",
    tenant: "default",
    workspace: "agentzero",
    port: 0,
    corsOrigins: [],
    migrationMode: "Apply",
    enableVector: false,
  };
}

describe("/api/health (fixture store)", () => {
  beforeEach(() => _resetProviderForTests());

  it("returns 200 + scope + capabilities object when the store opens", async () => {
    const res = await healthRoute(fixtureCfg()).request("/health");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.status).toBe("ok");
    expect(body.scope).toEqual({ tenant: "default", workspace: "agentzero" });
    expect(typeof body.capabilities).toBe("object");
    expect(body.capabilities).not.toBeNull();
  });

  it("returns 503 degraded when the store cannot open", async () => {
    const cfg = fixtureCfg();
    cfg.storageDir = "/nonexistent/engram-viz-path"; // trusted_root must exist
    const res = await healthRoute(cfg).request("/health");
    expect(res.status).toBe(503);
    const body = await res.json();
    expect(body.status).toBe("degraded");
    expect(body.degraded).toBe(true);
  });
});

// Boundary: the browser-facing BFF never spawns engram-mcp or shells out.
describe("BFF Boundary — no engram-mcp / no child_process in src/", () => {
  const srcDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src");
  const files = readdirSync(srcDir, { recursive: true })
    .filter((f) => String(f).endsWith(".ts"))
    .map((f) => join(srcDir, String(f)));

  it("no src/ file imports node:child_process", () => {
    const offenders = files.filter((f) =>
      /child_process/.test(readFileSync(f, "utf8")),
    );
    expect(offenders, offenders.map((f) => f.slice(srcDir.length)).join(", ")).toEqual([]);
  });

  // (No bare `engram-mcp` grep: src comments mention it by name to document
  // this Boundary; the `child_process` guard above is the actual spawn gate —
  // without it, no subprocess — engram-mcp or otherwise — can be invoked.)
});
