import type { NativeProviderBinding } from "./binding.js";

/** Direct transport over Rust embedding inference, without query formatting in JS. */
export interface NativeEmbeddingProviderTransport {
  embeddingSpace(): Promise<unknown>;
  providerId(): Promise<string>;
  embedQuery(text: string): Promise<number[]>;
  embedPassage(text: string): Promise<number[]>;
  embedBatch(texts: string[]): Promise<number[][]>;
}

/** Require a provider handle, preserving native capability and inference errors. */
export function createNativeEmbeddingProviderTransport(
  provider: NativeProviderBinding,
): NativeEmbeddingProviderTransport {
  if (!provider.requireEmbeddingProviderApi) {
    throw new Error("Native addon does not expose embedding inference. Rebuild @engram/node.");
  }
  const handle = provider.requireEmbeddingProviderApi();
  if (typeof handle.embedQuery !== "function" || typeof handle.embedPassage !== "function" || typeof handle.embedBatch !== "function") {
    throw new Error("Native addon does not expose embedding inference. Rebuild @engram/node.");
  }
  return {
    async embeddingSpace() { return JSON.parse(handle.embeddingSpace()) as unknown; },
    async providerId() { return handle.providerId(); },
    async embedQuery(text) { return handle.embedQuery(text); },
    async embedPassage(text) { return handle.embedPassage(text); },
    async embedBatch(texts) { return handle.embedBatch(texts); },
  };
}
