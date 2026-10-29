import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import type { EngramTransport } from "./transport.js";
import {
  EngramCodeBindingUnsupportedError,
  type CodeBindRequest,
  type CodeBindingRecord,
  type CodeBindingStatusResult,
  type CodeObserveRefRequest,
  type CodeRefObservation,
  type CodeScanRequest
} from "./code-binding.js";

/** Construction options for an `EngramClient`, currently limited to the required transport. */
export interface EngramClientOptions {
  transport: EngramTransport;
}

/** Typed facade for Engram v1 memory operations over an injected transport implementation. */
export class EngramClient {
  readonly #transport: EngramTransport;

  constructor(options: EngramClientOptions) {
    this.#transport = options.transport;
  }

  /** Writes one memory through the configured transport and returns the accepted v1 response shape. */
  writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return this.#transport.writeMemory(request);
  }

  /** Retrieves policy-checked context through the configured transport using a v1 retrieval request. */
  retrieve(request: RetrievalRequest): Promise<ContextPayload> {
    return this.#transport.retrieve(request);
  }

  /** Applies a v1 forget operation through the configured transport and returns the visible outcome. */
  forget(request: ForgetRequest): Promise<ForgetResult> {
    return this.#transport.forget(request);
  }

  /** The bound code-binding facade over the same transport (TASK-020); throws
   *  typed `EngramCodeBindingUnsupportedError` when the transport lacks the
   *  surface. */
  codeBinding(): EngramCodeBindingClient {
    return new EngramCodeBindingClient(this.#transport);
  }
}

/** Creates a typed Engram client over an injected transport while keeping core behavior elsewhere. */
export function createEngramClient(options: EngramClientOptions): EngramClient {
  return new EngramClient(options);
}

// ---- TASK-020: bound code-binding facade (optional transport surface) ----

/** Extracts the optional code-binding surface from the transport, if present. */
function codeTransport(
  transport: EngramTransport
): {
  codeBind(request: CodeBindRequest): Promise<CodeBindingRecord>;
  codeStatus(request: { scope: unknown; repo: string }): Promise<CodeBindingStatusResult>;
  codeObserveRef(request: CodeObserveRefRequest): Promise<CodeRefObservation>;
  scanCode(request: CodeScanRequest): Promise<unknown>;
} {
  const candidate = transport as {
    codeBind?: (request: CodeBindRequest) => Promise<CodeBindingRecord>;
    codeStatus?: (request: { scope: unknown; repo: string }) => Promise<CodeBindingStatusResult>;
    codeObserveRef?: (request: CodeObserveRefRequest) => Promise<CodeRefObservation>;
    scanCode?: (request: CodeScanRequest) => Promise<unknown>;
  };
  if (
    !candidate.codeBind ||
    !candidate.codeStatus ||
    !candidate.codeObserveRef ||
    !candidate.scanCode
  ) {
    throw new EngramCodeBindingUnsupportedError();
  }
  return candidate as unknown as ReturnType<typeof codeTransport>;
}

/** Bound code operations reachable on the injected transport (TASK-020).
 *  Every method fails typed when the transport lacks the surface. */
export class EngramCodeBindingClient {
  readonly #transport: ReturnType<typeof codeTransport>;

  constructor(transport: EngramTransport) {
    this.#transport = codeTransport(transport);
  }

  /** Binds a repository branch (same-ref idempotent; other branch
   *  `BindingConflict`). */
  codeBind(request: CodeBindRequest): Promise<CodeBindingRecord> {
    return this.#transport.codeBind(request);
  }

  /** Binding-keyed status: indexed OID, on-request freshness, checkpoint. */
  codeStatus(request: { scope: unknown; repo: string }): Promise<CodeBindingStatusResult> {
    return this.#transport.codeStatus(request);
  }

  /** Records one on-request ref observation. */
  codeObserveRef(request: CodeObserveRefRequest): Promise<CodeRefObservation> {
    return this.#transport.codeObserveRef(request);
  }

  /** Scans a BOUND repository (persisted binding mandatory — no direct-ingest
   *  bypass; unbound scopes deny with `BindingRequired`). */
  scanCode(request: CodeScanRequest): Promise<unknown> {
    return this.#transport.scanCode(request);
  }
}
