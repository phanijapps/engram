import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

/** Transport boundary that lets `EngramClient` call an implementation without owning runtime behavior. */
export interface EngramTransport {
  writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse>;
  retrieve(request: RetrievalRequest): Promise<ContextPayload>;
  forget(request: ForgetRequest): Promise<ForgetResult>;
  // TASK-020: the code-binding surface is OPTIONAL — a transport without it
  // fails typed (`EngramCodeBindingUnsupportedError`), never silently.
  codeBind?(request: CodeBindRequest): Promise<CodeBindingRecord>;
  codeRegister?(request: CodeRegisterRequest): Promise<unknown>;
  codeStatus?(request: { scope: unknown; repo: string }): Promise<CodeBindingStatusResult>;
  codeRefresh?(request: CodeRefreshRequest): Promise<CodeRefObservation>;
  scanCode?(request: CodeScanRequest): Promise<unknown>;
}

// TASK-020: the optional code-binding surface's types (declared on
// EngramTransport above) re-exported for transports that implement it.
import type {
  CodeBindRequest,
  CodeBindingRecord,
  CodeBindingStatusResult,
  CodeRefreshRequest,
  CodeRegisterRequest,
  CodeRefObservation,
  CodeScanRequest
} from "./code-binding.js";

/** The required code-binding surface (what `EngramCodeBindingClient` uses). */
export interface CodeBindingTransport {
  codeBind(request: CodeBindRequest): Promise<CodeBindingRecord>;
  codeRegister(request: CodeRegisterRequest): Promise<unknown>;
  codeStatus(request: { scope: unknown; repo: string }): Promise<CodeBindingStatusResult>;
  codeRefresh(request: CodeRefreshRequest): Promise<CodeRefObservation>;
  scanCode(request: CodeScanRequest): Promise<unknown>;
}
