//! App config for the engram-viz BFF, resolved from the environment.
//!
//! The viz reads engram's `agentzero` store in-process via `@engram/node`
//! (never engram-mcp for the browser). All store/scope knobs are env-driven so
//! the same backend points at any engram store without code changes — the
//! multi-user seam (`scope.ts`) builds on this.

import { homedir } from "node:os";
import { join } from "node:path";

export interface VizConfig {
  /** Directory holding the single-file engram store. */
  storageDir: string;
  /** Bare db file name inside `storageDir` (e.g. "engram_data.db"). */
  dbFile: string;
  /** Scope the viz reads (resolved centrally by `scope.ts`). */
  tenant: string;
  workspace: string;
  /** Hono listen port. */
  port: number;
  /** Allowed browser origins (CORS). */
  corsOrigins: string[];
  /** SQLite migration mode — `DryRun` for a read-only viz over an existing store. */
  migrationMode: "DryRun" | "Apply";
  /** Runtime vector kill-switch — `false` skips the FastEmbed model load. */
  enableVector: boolean;
}

export function loadConfig(): VizConfig {
  const storageDir =
    process.env.ENGRAM_STORAGE ?? join(homedir(), ".engram", "agentzero");
  const dbFile = process.env.ENGRAM_DB_FILE ?? "engram_data.db";
  return {
    storageDir,
    dbFile,
    tenant: process.env.ENGRAM_TENANT ?? "default",
    workspace: process.env.ENGRAM_WORKSPACE ?? "agentzero",
    port: Number(process.env.PORT ?? "3001"),
    corsOrigins: (
      process.env.CORS_ORIGINS ??
      "http://localhost:5173,http://127.0.0.1:5173"
    ).split(","),
    migrationMode: (
      process.env.ENGRAM_MIGRATION_MODE ?? "DryRun"
    ) as VizConfig["migrationMode"],
    enableVector: process.env.ENGRAM_ENABLE_VECTOR === "true",
  };
}

/** Absolute path to the single-file store. */
export function dbPath(cfg: VizConfig): string {
  return join(cfg.storageDir, cfg.dbFile);
}
