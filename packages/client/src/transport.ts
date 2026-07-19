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
}
