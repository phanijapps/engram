import type { ContextPayload } from "@engram/contracts";

/** Engram client operations that runtime adapters can observe without owning behavior. */
export type AdapterOperation = "writeMemory" | "retrieve" | "forget";

/** Stable error classes emitted by adapter wrappers. */
export type AdapterErrorKind = "policy_denial" | "transport_error";

/** Event emitted immediately before a wrapped Engram operation starts. */
export interface OperationStartedEvent {
  kind: "operation_started";
  operation: AdapterOperation;
  at: string;
}

/** Event emitted after a wrapped Engram operation completes successfully. */
export interface OperationSucceededEvent {
  kind: "operation_succeeded";
  operation: AdapterOperation;
  at: string;
  durationMs: number;
}

/** Event emitted after a wrapped Engram operation throws or rejects. */
export interface OperationFailedEvent {
  kind: "operation_failed";
  operation: AdapterOperation;
  at: string;
  durationMs: number;
  errorKind: AdapterErrorKind;
  message: string;
}

/** Retrieval-specific summary emitted after a successful retrieve operation. */
export interface RetrievalTraceEvent {
  kind: "retrieval_trace";
  operation: "retrieve";
  at: string;
  itemCount: number;
  omittedCount: number;
  sourceFailureCount: number;
}

/** Event union emitted by framework-neutral adapter utilities. */
export type AdapterEvent =
  | OperationStartedEvent
  | OperationSucceededEvent
  | OperationFailedEvent
  | RetrievalTraceEvent;

/** Receives adapter events and forwards them to application telemetry, logs, or tests. */
export interface AdapterObserver {
  emit(event: AdapterEvent): void | Promise<void>;
}

/** Classifies thrown transport errors into stable adapter error buckets. */
export function classifyError(error: unknown): AdapterErrorKind {
  const message = errorMessage(error).toLowerCase();
  if (message.includes("policy denied") || message.includes("policy_denied")) {
    return "policy_denial";
  }
  return "transport_error";
}

/** Extracts a safe human-readable message from an unknown thrown value. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "unknown transport error";
}

/** Builds a retrieval trace from the accepted `ContextPayload` shape. */
export function retrievalTrace(payload: ContextPayload, at: string): RetrievalTraceEvent {
  return {
    kind: "retrieval_trace",
    operation: "retrieve",
    at,
    itemCount: payload.items.length,
    omittedCount: payload.omitted?.length ?? 0,
    sourceFailureCount: payload.sourceFailures?.length ?? 0
  };
}
