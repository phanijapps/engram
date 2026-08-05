//! The multi-user seam.
//!
//! `resolveScope` is the SINGLE source of the engram `{tenant, workspace}` the
//! BFF reads with. Route handlers MUST NOT hardcode `tenant`/`workspace`
// literals — they take scope from here, so a future auth + per-user-scope layer
// slots in by changing this one module. (T4 adds the no-literal guard test.)

import type { Scope } from "@engram/contracts";

import type { VizConfig } from "./config.ts";

export function resolveScope(cfg: VizConfig): Scope {
  const scope: Scope = { tenant: cfg.tenant };
  if (cfg.workspace) {
    scope.workspace = cfg.workspace;
  }
  return scope;
}
