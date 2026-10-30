/**
 * Bound code-binding surface for `@engram/client` (TASK-020; r6 finding 6:
 * generated-contract truth).
 *
 * The client is a typed facade: the RESULT types (`CodeBinding`,
 * `CodeBindingStatus`, `CodeRefObservation`, …) are the GENERATED
 * `@engram/contracts` shapes and the `@engram/node` transport types built on
 * them — no independently maintained duplicate record, no weakened `unknown`
 * scope. Every method is a thin delegate. Denial parity: the Rust surfaces
 * reject unbound code calls with `BindingRequired` / `BindingConflict`
 * messages; [`isBindingDenied`] recognizes that exact envelope so callers can
 * branch on it without string-matching internals.
 */
import type { CodeBinding, Scope } from "@engram/contracts";
import type {
  CodeBindingCheckpoint,
  CodeBindingStatus,
  CodeFreshnessState,
  CodeRefFreshness,
  CodeRefObservation
} from "@engram/node";

// The Rust-serialized result shapes (single definition in `@engram/node`,
// built on the generated contracts) re-exported for transport implementors.
export type {
  CodeBindingCheckpoint,
  CodeBindingStatus,
  CodeFreshnessState,
  CodeRefFreshness,
  CodeRefObservation
};
export type { CodeBinding, Scope };

/** Bind request (`{ scope, repo, branchRef }`). */
export interface CodeBindRequest {
  scope: Scope;
  repo: string;
  branchRef: string;
}

/** Status request (`{ scope, repo }`). */
export interface CodeStatusRequest {
  scope: Scope;
  repo: string;
}

/** Registration request (`{ scope, repo, path, mode? }`). */
export interface CodeRegisterRequest {
  scope: Scope;
  repo: string;
  path: string;
  mode?: "local-only" | "configured-remote";
}

/** Refresh request (`{ scope, repo }`) — the registered-locator refresh. */
export interface CodeRefreshRequest {
  scope: Scope;
  repo: string;
}

/** Bound code-scan request (persisted binding mandatory). */
export interface CodeScanRequest {
  path: string;
  scope: Scope;
  repo: string;
  branchRef?: string;
  scanFilter?: unknown;
}

/** Typed error thrown when a transport lacks the code-binding surface. */
export class EngramCodeBindingUnsupportedError extends Error {
  constructor(message = "code-binding surface is not available on this transport") {
    super(message);
    this.name = "EngramCodeBindingUnsupportedError";
  }
}

/** Recognizes the Rust binding-denial envelope (`BindingRequired` /
 * `BindingConflict`) across the provider, legacy, and client surfaces — the
 * surfaces agree on these denials by contract (TASK-020). */
export function isBindingDenied(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("BindingRequired") || message.includes("BindingConflict");
}
