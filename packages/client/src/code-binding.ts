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
import type { CodeBinding, CodeResetJournal, CodeResetPreview, Scope } from "@engram/contracts";
import type {
  CodeBindingCheckpoint,
  CodeBindingStatus,
  CodeFreshnessState,
  CodeRefFreshness,
  CodeRefObservation,
  CodeScanFilter,
  CodeScanResult as NodeCodeScanResult,
  BoundCodeScanRequest as NodeBoundCodeScanRequest,
  LegacyDocumentScanRequest as NodeLegacyDocumentScanRequest,
  BoundCodeScanResult,
  LegacyDocumentScanResult
} from "@engram/node";

/**
 * Rust-serialized binding result shapes re-exported for transport implementors.
 * Their single definition is in `@engram/node` and generated contracts.
 */
export type {
  CodeBindingCheckpoint,
  CodeBindingStatus,
  CodeFreshnessState,
  CodeRefFreshness,
  CodeRefObservation,
  CodeScanFilter,
  BoundCodeScanResult,
  LegacyDocumentScanResult
};
/** Generated portable binding and scope contract shapes. */
export type { CodeBinding, CodeResetJournal, CodeResetPreview, Scope };

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

/** Reset request and response contracts are generated portable types. */
export interface CodeResetPreviewRequest { scope: Scope; repo: string; }
export interface CodeResetApplyRequest { journal: CodeResetJournal; decision: string; }
export interface CodeResetResumeRequest { journal: CodeResetJournal; }

/** Bound or explicit legacy document-only scan request. */
export type CodeScanRequest = NodeBoundCodeScanRequest | NodeLegacyDocumentScanRequest;

/** Typed outcome from the selected scan lane. */
export type CodeScanResult = NodeCodeScanResult;

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
