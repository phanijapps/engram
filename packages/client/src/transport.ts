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
  codeBind?(request: CodeBindRequest): Promise<CodeBinding>;
  codeRegister?(request: CodeRegisterRequest): Promise<unknown>;
  codeStatus?(request: CodeStatusRequest): Promise<CodeBindingStatus>;
  codeRefresh?(request: CodeRefreshRequest): Promise<CodeRefObservation>;
  codeResetPreview?(request: CodeResetPreviewRequest): Promise<CodeResetPreview>;
  codeResetApply?(request: CodeResetApplyRequest): Promise<CodeResetJournal>;
  codeResetResume?(request: CodeResetResumeRequest): Promise<CodeResetJournal>;
  scanCode?(request: CodeScanRequest): Promise<CodeScanResult>;
}

// TASK-020: the optional code-binding surface's types (declared on
// EngramTransport above) re-exported for transports that implement it.
import type { CodeBinding, CodeResetJournal, CodeResetPreview } from "@engram/contracts";
import type { CodeBindingStatus } from "@engram/node";
import type {
  CodeBindRequest,
  CodeRefreshRequest,
  CodeRegisterRequest,
  CodeRefObservation,
  CodeScanRequest,
  CodeScanResult,
  CodeStatusRequest,
  CodeResetPreviewRequest,
  CodeResetApplyRequest,
  CodeResetResumeRequest
} from "./code-binding.js";

/** The required code-binding surface (what `EngramCodeBindingClient` uses). */
export interface CodeBindingTransport {
  codeBind(request: CodeBindRequest): Promise<CodeBinding>;
  codeRegister(request: CodeRegisterRequest): Promise<unknown>;
  codeStatus(request: CodeStatusRequest): Promise<CodeBindingStatus>;
  codeRefresh(request: CodeRefreshRequest): Promise<CodeRefObservation>;
  scanCode(request: CodeScanRequest): Promise<CodeScanResult>;
  codeResetPreview(request: CodeResetPreviewRequest): Promise<CodeResetPreview>;
  codeResetApply(request: CodeResetApplyRequest): Promise<CodeResetJournal>;
  codeResetResume(request: CodeResetResumeRequest): Promise<CodeResetJournal>;
}
