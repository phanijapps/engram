//! T1 wiring test — deterministic, against a throwaway fixture store (NOT the
//! 2 GB agentzero store). Proves: the `configJson` shape serializes a valid
//! single-file `EngramConfig`, the native binding loads, and the provider opens
//! + reports a non-null capability report. The live agentzero store is
//! validated separately by a `/api/health` smoke (goal-based, T1 `Done when:`).

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  buildConfigJson,
  getProvider,
  _resetProviderForTests,
} from "../src/engram/provider.ts";
import type { VizConfig } from "../src/config.ts";

function fixtureConfig(): VizConfig {
  const store = mkdtempSync(join(tmpdir(), "engram-cc-t1-"));
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

describe("provider wiring (fixture store)", () => {
  beforeEach(() => _resetProviderForTests());

  it("serializes a valid single-file EngramConfig", () => {
    const cfg = fixtureConfig();
    const parsed = JSON.parse(buildConfigJson(cfg));
    expect(parsed.storage_path).toBe(cfg.storageDir);
    expect(parsed.trusted_root).toBe(cfg.storageDir);
    expect(parsed.scope_policy).toBe("Strict");
    expect(parsed.migration_mode).toBe("Apply");
    expect(parsed.capability_policy).toBe("FailClosed");
    expect(parsed.enable_vector).toBe(false);
    expect(parsed.embedding_provider).toEqual({
      provider_type: "none",
      model: "none",
      dimensions: 384,
      prompt_profile: "query",
    });
    expect(parsed.sqlite_storage_layout).toEqual({
      kind: "single_file",
      file_name: "test.db",
    });
  });

  it("opens a provider and reports a non-null capability report", async () => {
    const provider = getProvider(fixtureConfig());
    const capabilities = await provider.capabilities();
    expect(capabilities).not.toBeNull();
    expect(typeof capabilities).toBe("object");
  });
});
