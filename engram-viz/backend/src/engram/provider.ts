//! In-process `@engram/node` provider transport — the structured read path to
//! engram's store. This is the BFF's data plane; the browser never speaks
//! engram-mcp (it stays the LLM-agent surface — ADR-0022).
//!
//! `buildConfigJson` serializes the v1 `EngramConfig` the binding decodes. For a
//! read-only graph viz over an existing single-file store it uses `DryRun`
//! (no migration) and `enable_vector=false` + `provider_type=none` to skip the
//! FastEmbed model load at boot.

import {
  createNativeProviderTransport,
  type NativeProviderTransport,
} from "@engram/node";

import type { VizConfig } from "../config.ts";

export function buildConfigJson(cfg: VizConfig): string {
  return JSON.stringify({
    storage_path: cfg.storageDir,
    trusted_root: cfg.storageDir,
    scope_policy: "Strict",
    embedding_provider: {
      provider_type: "none",
      model: "none",
      dimensions: 384,
      prompt_profile: "query",
    },
    migration_mode: cfg.migrationMode,
    capability_policy: "FailClosed",
    sqlite_storage_layout: { kind: "single_file", file_name: cfg.dbFile },
    enable_vector: cfg.enableVector,
  });
}

let cached: NativeProviderTransport | null = null;

/**
 * Lazy singleton over the held `NativeProvider`. Construction (`new
 * NativeProvider(configJson)`) opens the store synchronously — the load-bearing
 * health check. Throws on failure; callers map that to a 503. One provider per
 * process; the agentzero store is opened read-only with WAL concurrent-reader
 * compatibility.
 */
export function getProvider(cfg: VizConfig): NativeProviderTransport {
  if (cached) return cached;
  cached = createNativeProviderTransport({ configJson: buildConfigJson(cfg) });
  return cached;
}

/** Test-only: drop the cached provider so each test opens its own fixture store. */
export function _resetProviderForTests(): void {
  cached = null;
}
