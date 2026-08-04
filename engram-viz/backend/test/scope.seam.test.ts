//! T4 — the multi-user seam guard. `scope.ts` is the SINGLE resolver of the
//! engram `{tenant, workspace}` the BFF reads with; route handlers must take
//! scope from `resolveScope` (injected), never construct it inline. This test
//! holds that invariant so a future auth + per-user-scope layer slots in by
//! changing one module.

import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { resolveScope } from "../src/scope.ts";
import type { VizConfig } from "../src/config.ts";

const ROUTES_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "src",
  "routes",
);

// A hardcoded scope property literal: `tenant: "x"` / `workspace: "y"`. Routes
// that built scope inline would match this; routes that take scope from
// `resolveScope` do not.
const SCOPE_LITERAL = /\b(?:tenant|workspace)\s*:\s*["'][^"']+["']/;

const ROUTE_FILES = readdirSync(ROUTES_DIR).filter((f) => f.endsWith(".ts"));

describe("multi-user scope seam", () => {
  it("resolveScope builds {tenant, workspace} from config (not hardcoded)", () => {
    const cfg: VizConfig = {
      storageDir: "/tmp",
      dbFile: "x.db",
      tenant: "default",
      workspace: "agentzero",
      port: 0,
      corsOrigins: [],
      migrationMode: "DryRun",
      enableVector: false,
    };
    expect(resolveScope(cfg)).toEqual({ tenant: "default", workspace: "agentzero" });
    const noWorkspace: VizConfig = { ...cfg, workspace: "" };
    expect(resolveScope(noWorkspace)).toEqual({ tenant: "default" });
  });

  it("no route handler hardcodes a tenant/workspace literal", () => {
    expect(ROUTE_FILES.length, "route files present").toBeGreaterThan(0);
    const offenders = ROUTE_FILES.filter((f) =>
      SCOPE_LITERAL.test(readFileSync(join(ROUTES_DIR, f), "utf8")),
    );
    expect(offenders).toEqual([]);
  });
});
