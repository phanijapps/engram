import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import type { EngramTransport } from "./transport.js";
import type { CodeStatusRequest } from "./code-binding.js";
import type { CodeBinding, CodeResetJournal, CodeResetPreview } from "@engram/contracts";
import type { CodeBindingStatus } from "@engram/node";
import {
  EngramCodeBindingUnsupportedError,
  type CodeBindRequest,
  type CodeRefreshRequest,
  type CodeRegisterRequest,
  type CodeRefObservation,
  type CodeScanRequest,
  type CodeScanResult,
  type CodeResetPreviewRequest,
  type CodeResetApplyRequest,
  type CodeResetResumeRequest
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
  codeBind(request: CodeBindRequest): Promise<CodeBinding>;
  codeRegister(request: CodeRegisterRequest): Promise<unknown>;
  codeStatus(request: CodeStatusRequest): Promise<CodeBindingStatus>;
  codeRefresh(request: CodeRefreshRequest): Promise<CodeRefObservation>;
  scanCode(request: CodeScanRequest): Promise<CodeScanResult>;
  codeResetPreview(request: CodeResetPreviewRequest): Promise<CodeResetPreview>;
  codeResetApply(request: CodeResetApplyRequest): Promise<CodeResetJournal>;
  codeResetResume(request: CodeResetResumeRequest): Promise<CodeResetJournal>;
} {
  const candidate = transport as {
    codeBind?: (request: CodeBindRequest) => Promise<CodeBinding>;
    codeRegister?: (request: CodeRegisterRequest) => Promise<unknown>;
    codeStatus?: (request: CodeStatusRequest) => Promise<CodeBindingStatus>;
    codeRefresh?: (request: CodeRefreshRequest) => Promise<CodeRefObservation>;
    scanCode?: (request: CodeScanRequest) => Promise<CodeScanResult>;
    codeResetPreview?: (request: CodeResetPreviewRequest) => Promise<CodeResetPreview>;
    codeResetApply?: (request: CodeResetApplyRequest) => Promise<CodeResetJournal>;
    codeResetResume?: (request: CodeResetResumeRequest) => Promise<CodeResetJournal>;
  };
  if (
    !candidate.codeBind ||
    !candidate.codeRegister ||
    !candidate.codeStatus ||
    !candidate.codeRefresh ||
    !candidate.scanCode ||
    !candidate.codeResetPreview ||
    !candidate.codeResetApply ||
    !candidate.codeResetResume
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
   *  `BindingConflict`). Returns the GENERATED `CodeBinding` contract. */
  codeBind(request: CodeBindRequest): Promise<CodeBinding> {
    return this.#transport.codeBind(request);
  }

  /** Binding-keyed status: indexed OID, on-request freshness, checkpoint. */
  codeStatus(request: CodeStatusRequest): Promise<CodeBindingStatus> {
    return this.#transport.codeStatus(request);
  }

  /** Creates an exact reset preview through the authorized transport. */
  codeResetPreview(request: CodeResetPreviewRequest): Promise<CodeResetPreview> { return this.#transport.codeResetPreview(request); }
  /** Persists an owner-approved reset journal and applies it. */
  codeResetApply(request: CodeResetApplyRequest): Promise<CodeResetJournal> { return this.#transport.codeResetApply(request); }
  /** Resumes the immutable approved journal after an interruption. */
  codeResetResume(request: CodeResetResumeRequest): Promise<CodeResetJournal> { return this.#transport.codeResetResume(request); }

  /** Registers a repository's acquisition locator (r6 finding 5). */
  codeRegister(request: CodeRegisterRequest): Promise<unknown> {
    return this.#transport.codeRegister(request);
  }

  /** ONE on-request refresh of the bound branch ref, resolved from the
   *  REGISTERED locator (r6 finding 5) — never a caller-supplied OID. */
  codeRefresh(request: CodeRefreshRequest): Promise<CodeRefObservation> {
    return this.#transport.codeRefresh(request);
  }

  /** Scans a BOUND repository (persisted binding mandatory — no direct-ingest
   *  bypass; unbound scopes deny with `BindingRequired`). */
  scanCode(request: CodeScanRequest): Promise<CodeScanResult> {
    return this.#transport.scanCode(request);
  }
}
