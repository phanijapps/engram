import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import {
  loadNativeBinding,
  type NativeBinding,
  type NativeBindingLoader,
  type NativeMemoryEngineBinding
} from "./binding.js";

/** Transport interface implemented by the native Engram Node package. */
export interface NativeMemoryTransport {
  writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse>;
  retrieve(request: RetrievalRequest): Promise<ContextPayload>;
  forget(request: ForgetRequest): Promise<ForgetResult>;
}

/** Options for constructing a native memory transport. */
export interface NativeMemoryTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
}

/** Creates a transport that delegates memory behavior to the Rust native engine. */
export function createNativeMemoryTransport(
  options: NativeMemoryTransportOptions = {}
): NativeMemoryTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeMemoryTransport(new binding.NativeMemoryEngine());
}

class JsonNativeMemoryTransport implements NativeMemoryTransport {
  constructor(private readonly engine: NativeMemoryEngineBinding) {}

  async writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return decode<WriteMemoryResponse>(this.engine.writeMemoryJson(encode(request)));
  }

  async retrieve(request: RetrievalRequest): Promise<ContextPayload> {
    return decode<ContextPayload>(this.engine.retrieveJson(encode(request)));
  }

  async forget(request: ForgetRequest): Promise<ForgetResult> {
    return decode<ForgetResult>(this.engine.forgetJson(encode(request)));
  }
}

function encode(value: unknown): string {
  return JSON.stringify(value);
}

function decode<T>(json: string): T {
  return JSON.parse(json) as T;
}
