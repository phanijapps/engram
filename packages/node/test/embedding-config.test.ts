import { describe, expect, it, vi } from "vitest";
import { resolveEmbeddingConfig } from "../src/embedding-config.js";
import type { NativeBinding } from "../src/binding.js";

function fixture() {
  const resolve = vi.fn(() => JSON.stringify({
    provider_type: "fastembed", model: "BAAI/bge-small-en-v1.5", dimensions: 384,
    prompt_profile: "query", request_timeout_secs: 30, batch_size: 32,
  }));
  const bindingLoader = () => ({ resolveEmbeddingConfig: resolve }) as unknown as NativeBinding;
  return { resolve, bindingLoader };
}

describe("embedding configuration transport", () => {
  it("keeps environment opt-in and leaves defaults to Rust", () => {
    const { resolve, bindingLoader } = fixture();
    const config = resolveEmbeddingConfig({ bindingLoader });
    expect(resolve).toHaveBeenCalledWith(null, null, false);
    expect(config.dimensions).toBe(384);
  });

  it("forwards profile and explicit inputs independently", () => {
    const { resolve, bindingLoader } = fixture();
    const profile = { preset: "local-alternative" as const };
    const overrides = { model: "custom-model", dimensions: 512 };
    resolveEmbeddingConfig({ profile, overrides, useEnvironment: true, bindingLoader });
    expect(resolve).toHaveBeenCalledWith(JSON.stringify(profile), JSON.stringify(overrides), true);
  });

  it("preserves validation errors from Rust", () => {
    const bindingLoader = () => ({ resolveEmbeddingConfig: () => { throw new Error("invalid dimensions"); } }) as unknown as NativeBinding;
    expect(() => resolveEmbeddingConfig({ overrides: { dimensions: -1 }, bindingLoader })).toThrow("invalid dimensions");
  });

  it("reports an outdated addon without applying a fallback", () => {
    expect(() => resolveEmbeddingConfig({ bindingLoader: () => ({}) as NativeBinding })).toThrow("Rebuild @engram/node");
  });
});
