import { readFileSync } from "node:fs";

import type { Scope } from "@engram/contracts";

/**
 * A scan result summary. The Phase A facade types `scan` as `Promise<unknown>`
 * (and `@engram/contracts` has no `ScanSummary`), so the runtime mirrors the
 * Rust shape (`adapters/ingest/src/scanner.rs`) here for logging + assertions.
 */
export interface ScanSummary {
  scanned: number;
  ingested: number;
  unchanged: number;
  skipped: number;
  entities: number;
  relationships: number;
  errors: number;
  git_remote: string | null;
  git_branch: string | null;
  git_sha: string | null;
}

/**
 * Builds the serialized `EngramConfig` (v1, snake_case) from a `--config` arg:
 * inline JSON when it starts with `{`, otherwise a file path that is read as
 * UTF-8. The JSON is validated (parsed) but returned as a string — the facade's
 * `createNativeProviderTransport` decodes `EngramConfig` from it.
 */
export function buildEngramConfig(configArg: string): string {
  const json = configArg.trimStart().startsWith("{")
    ? configArg
    : readFileSync(configArg, "utf8");
  JSON.parse(json); // validate
  return json;
}

/**
 * Builds a {@link Scope} from the two fields a caller normally provides. Under
 * `exactOptionalPropertyTypes`, `workspace` is omitted entirely when unset
 * (not set to `undefined`).
 */
export function buildScope(opts: {
  tenant: string;
  workspace?: string;
}): Scope {
  const scope: Scope = { tenant: opts.tenant };
  if (opts.workspace !== undefined) {
    scope.workspace = opts.workspace;
  }
  return scope;
}
