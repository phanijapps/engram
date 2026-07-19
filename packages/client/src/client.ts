import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import type { EngramTransport } from "./transport.js";

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
}

/** Creates a typed Engram client over an injected transport while keeping core behavior elsewhere. */
export function createEngramClient(options: EngramClientOptions): EngramClient {
  return new EngramClient(options);
}
