import { describe, expect, it, vi } from "vitest";
import { createNativeEmbeddingProviderTransport } from "../src/embedding-provider.js";
import { createNativeProviderTransport } from "../src/provider.js";
import type { NativeProviderBinding } from "../src/binding.js";

function fixture() {
  const handle = {
    embeddingSpace: vi.fn(() => '{"model_id":"fixture","dimensions":2}'),
    providerId: vi.fn(() => "fixture"),
    embedQuery: vi.fn(() => [0.25, 0.75]),
    embedPassage: vi.fn(() => [0.5, 0.5]),
    embedBatch: vi.fn(() => [[0.5, 0.5], [0.75, 0.25]]),
  };
  const provider = { requireEmbeddingProviderApi: vi.fn(() => handle) } as unknown as NativeProviderBinding;
  return { handle, provider };
}

describe("embedding inference transport", () => {
  it("keeps query, passage and ordered batch calls on the same Rust handle", async () => {
    const { handle, provider } = fixture();
    const transport = createNativeEmbeddingProviderTransport(provider);
    expect(await transport.embedQuery("query")).toEqual([0.25, 0.75]);
    expect(await transport.embedPassage("passage")).toEqual([0.5, 0.5]);
    expect(await transport.embedBatch(["first", "second"])).toEqual([[0.5, 0.5], [0.75, 0.25]]);
    expect(handle.embedQuery).toHaveBeenCalledWith("query");
    expect(handle.embedPassage).toHaveBeenCalledWith("passage");
    expect(handle.embedBatch).toHaveBeenCalledWith(["first", "second"]);
    expect(await transport.embeddingSpace()).toEqual({ model_id: "fixture", dimensions: 2 });
    expect(await transport.providerId()).toBe("fixture");
  });

  it("exposes the handle through the held provider transport", async () => {
    const { handle, provider } = fixture();
    await createNativeProviderTransport({ provider }).embedding().embedQuery("unchanged");
    expect(handle.embedQuery).toHaveBeenCalledWith("unchanged");
  });

  it("preserves capability and inference failures", async () => {
    const { handle, provider } = fixture();
    handle.embedQuery.mockImplementation(() => { throw new Error("output dimension mismatch"); });
    await expect(createNativeEmbeddingProviderTransport(provider).embedQuery("query")).rejects.toThrow("output dimension mismatch");
    expect(() => createNativeEmbeddingProviderTransport({
      requireEmbeddingProviderApi() { throw new Error("provider unavailable"); },
    } as unknown as NativeProviderBinding)).toThrow("provider unavailable");
    expect(() => createNativeEmbeddingProviderTransport({} as NativeProviderBinding)).toThrow("Rebuild @engram/node");
    expect(() => createNativeEmbeddingProviderTransport({
      requireEmbeddingProviderApi() { return { embeddingSpace() { return "{}"; } }; },
    } as unknown as NativeProviderBinding)).toThrow("Rebuild @engram/node");
  });
});
