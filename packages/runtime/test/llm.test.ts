//! LLM provider wrapper — DI + dry-run + env config. Does NOT exercise the real
//! pi-mono path (that needs a key + network; smoke-only, manual).

import { describe, it, expect } from "vitest";

import {
  createLlmProvider,
  llmConfigFromEnv,
  type LlmCompleteResult,
} from "../src/maintenance/llm.js";

describe("llm provider wrapper", () => {
  it("completeOverride bypasses pi-mono + passes through the result", async () => {
    const fixture: LlmCompleteResult = {
      toolCalls: [{ name: "record_belief", arguments: { subject: "x" } }],
      text: "ok",
    };
    const p = createLlmProvider({
      provider: "stub",
      model: "stub",
      completeOverride: async () => fixture,
    });
    const r = await p.complete({ userText: "hi", systemPrompt: "s" });
    expect(r).toEqual(fixture);
    expect(p.provider).toBe("stub");
    expect(p.model).toBe("stub");
  });

  it("PI_DRY_RUN emits one toolCall per provided tool (empty args)", async () => {
    const prev = process.env.PI_DRY_RUN;
    process.env.PI_DRY_RUN = "1";
    try {
      const p = createLlmProvider({ provider: "anthropic", model: "claude-haiku-4-5" });
      const tools = [
        { name: "record_belief", description: "d", parameters: {} },
        { name: "find_contradiction", description: "d", parameters: {} },
      ];
      const r = await p.complete({ userText: "hi", tools: tools as never });
      expect(r.toolCalls).toHaveLength(2);
      expect(r.toolCalls[0]!.name).toBe("record_belief");
      expect(r.toolCalls[1]!.name).toBe("find_contradiction");
      expect(r.toolCalls[0]!.arguments).toEqual({});
    } finally {
      if (prev === undefined) delete process.env.PI_DRY_RUN;
      else process.env.PI_DRY_RUN = prev;
    }
  });

  it("constructing the real (non-dry-run) provider resolves the default model without throwing", () => {
    // No PI_DRY_RUN, no override, no key → piMonoProvider; getModel must succeed for
    // the default anthropic/claude-haiku-4-5 (no network until complete() is called).
    const prev = process.env.PI_DRY_RUN;
    delete process.env.PI_DRY_RUN;
    try {
      const p = createLlmProvider({ provider: "anthropic", model: "claude-haiku-4-5" });
      expect(p.provider).toBe("anthropic");
      expect(typeof p.complete).toBe("function");
    } finally {
      if (prev !== undefined) process.env.PI_DRY_RUN = prev;
    }
  });

  it("llmConfigFromEnv reads overrides + defaults to anthropic/claude-haiku-4-5", () => {
    const ollama = llmConfigFromEnv({
      PI_PROVIDER: "ollama",
      PI_MODEL: "llama3.1",
      OLLAMA_BASE_URL: "http://localhost:11434",
    });
    expect(ollama.provider).toBe("ollama");
    expect(ollama.model).toBe("llama3.1");

    const def = llmConfigFromEnv({});
    expect(def.provider).toBe("anthropic");
    expect(def.model).toBe("claude-haiku-4-5");

    const keyed = llmConfigFromEnv({ ANTHROPIC_API_KEY: "sk-test" });
    expect(keyed.apiKey).toBe("sk-test");
  });
});
