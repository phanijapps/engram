/**
 * Bound code-binding surface for `@engram/client` (TASK-020).
 *
 * The client is a typed facade: these types mirror the Rust coordinator's
 * serialized (camelCase) shapes — the same values `@engram/node`'s native
 * transport returns — and every method is a thin delegate. Denial parity:
 * the Rust surfaces reject unbound code calls with `BindingRequired` /
 * `BindingConflict` messages; [`isBindingDenied`] recognizes that exact
 * envelope so callers can branch on it without string-matching internals.
 */

/** Serialized `CodeBinding` (matches `@engram/contracts` `CodeBinding`). */
export interface CodeBindingRecord {
  scope: unknown;
  repoId: string;
  branchRef: string;
  version: number;
  activeGeneration: string | null;
  epoch: number;
}

/** One on-request ref observation (policy `on_request`). */
export interface CodeRefObservation {
  branchRef: string;
  observedOid: string;
  observedAt: string;
  policy: "on_request";
}

/** Truthful freshness classification (never checkout-HEAD-derived):
 *  the serde externally-tagged shape — `"unknown"` (unit variant) or
 *  `{ observed: { matchesIndexed } }`. */
export type CodeFreshnessState =
  | "unknown"
  | { observed: { matchesIndexed: boolean | null } };

/** Binding-keyed freshness facts. */
export interface CodeRefFreshness {
  branchRef: string;
  policy: "on_request";
  observedOid: string | null;
  observedAt: string | null;
  state: CodeFreshnessState;
}

/** The immutable generation facts a checkpoint points at. */
export interface CheckpointGeneration {
  id: string;
  commitOid: string;
  snapshotDigest: string;
  parserFingerprint: string;
  filterFingerprint: string;
}

/** The binding-keyed checkpoint pointer (RepoId-keyed, never basename-keyed). */
export interface CodeBindingCheckpoint {
  repoId: string;
  branchRef: string;
  bindingVersion: number;
  codeEpoch: number;
  generation: CheckpointGeneration | null;
}

/** Serialized `CodeBindingStatus`. */
export interface CodeBindingStatusResult {
  binding: CodeBindingRecord;
  attempt: unknown;
  codeEpoch: number;
  indexedCommitOid: string | null;
  freshness: CodeRefFreshness;
  checkpoint: CodeBindingCheckpoint;
}

/** Bind request (`{ scope, repo, branchRef }`). */
export interface CodeBindRequest {
  scope: unknown;
  repo: string;
  branchRef: string;
}

/** Status request (`{ scope, repo }`). */
export interface CodeStatusRequest {
  scope: unknown;
  repo: string;
}

/** Observe request (`{ scope, repo, observedOid }`). */
export interface CodeObserveRefRequest {
  scope: unknown;
  repo: string;
  observedOid: string;
}

/** Bound code-scan request (persisted binding mandatory). */
export interface CodeScanRequest {
  path: string;
  scope: unknown;
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
