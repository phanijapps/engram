//! LLM provider wrapper over the pi-mono SDK (`@earendil-works/pi-ai`). The single
//! seam through which `engram-maintain`'s LLM ops (reflection, contradiction) call a
//! model. Provider-agnostic: default **Anthropic Claude**, switchable to
//! **Ollama**/**OpenAI** via env (`PI_PROVIDER`/`PI_MODEL` + provider key /
//! `OLLAMA_BASE_URL`). **Rust stays LLM-free** — this is TS-only (RFC-0017).
//!
//! Testing: inject `completeOverride` to bypass pi-mono entirely (no tokens, no
//! network). `PI_DRY_RUN=1` is the manual/E2E fallback (one toolCall per tool).

import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import { Type, type Context, type Model, type Tool } from "@earendil-works/pi-ai";

export { Type };
export type { Tool };

export interface LlmToolCall {
  name: string;
  arguments: Record<string, unknown>;
}

export interface LlmCompleteResult {
  toolCalls: LlmToolCall[];
  text: string;
}

export interface LlmCompleteOptions {
  systemPrompt?: string;
  userText: string;
  tools?: Tool[];
}

export interface LlmProvider {
  readonly provider: string;
  readonly model: string;
  complete(opts: LlmCompleteOptions): Promise<LlmCompleteResult>;
}

export interface LlmProviderConfig {
  provider: string;
  model: string;
  apiKey?: string;
  /** Ollama base URL (default http://localhost:11434). Only used when provider=ollama. */
  ollamaBaseUrl?: string;
  /** Test/manual override — bypasses pi-mono entirely. */
  completeOverride?: (opts: LlmCompleteOptions) => Promise<LlmCompleteResult>;
}

export function llmConfigFromEnv(env: NodeJS.ProcessEnv = process.env): LlmProviderConfig {
  const apiKey = env.ANTHROPIC_API_KEY ?? env.OPENAI_API_KEY;
  return {
    provider: env.PI_PROVIDER ?? "anthropic",
    model: env.PI_MODEL ?? "claude-haiku-4-5",
    ...(apiKey !== undefined ? { apiKey } : {}),
    ...(env.OLLAMA_BASE_URL !== undefined ? { ollamaBaseUrl: env.OLLAMA_BASE_URL } : {}),
  };
}

export function createLlmProvider(
  config: LlmProviderConfig = llmConfigFromEnv(),
): LlmProvider {
  if (config.completeOverride) {
    return {
      provider: config.provider,
      model: config.model,
      complete: config.completeOverride,
    };
  }
  if (process.env.PI_DRY_RUN === "1") {
    return dryRunProvider(config);
  }
  return piMonoProvider(config);
}

function piMonoProvider(config: LlmProviderConfig): LlmProvider {
  const models = builtinModels();
  const isOllama = config.provider === "ollama";
  // pi-mono's builtin list has no Ollama models, so for Ollama construct an
  // OpenAI-compatible Model pointing at the local Ollama /v1 endpoint. Other
  // providers resolve via the builtin list.
  const model = isOllama
    ? ollamaModel(config.model, config.ollamaBaseUrl)
    : models.getModel(config.provider, config.model);
  if (!model) {
    throw new Error(
      `pi-mono: model not found: ${config.provider}/${config.model} — set PI_PROVIDER/PI_MODEL to a built-in model (PI_DRY_RUN=1 to skip)`,
    );
  }
  // Ollama needs no key; the OpenAI client requires one → pass a dummy.
  const auth = isOllama
    ? { apiKey: "ollama" }
    : config.apiKey !== undefined
      ? { apiKey: config.apiKey }
      : undefined;
  return {
    provider: config.provider,
    model: config.model,
    complete: async ({ systemPrompt, userText, tools }) => {
      const context: Context = {
        messages: [{ role: "user", content: userText, timestamp: Date.now() }],
        ...(systemPrompt !== undefined ? { systemPrompt } : {}),
        ...(tools && tools.length > 0 ? { tools } : {}),
      };
      const resp = await models.complete(model, context, auth);
      // pi-mono returns errors in `errorMessage` (not as a throw) — surface them
      // instead of silently returning an empty result.
      const errMsg = (resp as { errorMessage?: unknown }).errorMessage;
      if (typeof errMsg === "string" && errMsg.length > 0) {
        throw new Error(
          `LLM call failed (${config.provider}/${config.model}): ${errMsg}`,
        );
      }
      const blocks = (resp.content ?? []) as Array<{
        type: string;
        text?: string;
        name?: string;
        arguments?: unknown;
      }>;
      const toolCalls: LlmToolCall[] = blocks
        .filter((b) => b.type === "toolCall" && typeof b.name === "string")
        .map((b) => ({
          name: b.name as string,
          arguments: (b.arguments as Record<string, unknown> | undefined) ?? {},
        }));
      const text = blocks
        .filter((b) => b.type === "text")
        .map((b) => b.text ?? "")
        .join("");
      return { toolCalls, text };
    },
  };
}

/** Builds an OpenAI-compatible Model for a local Ollama instance. Ollama exposes
 *  an OpenAI-compatible `/v1/chat/completions`. pi-mono rejects `provider:"ollama"`
 *  ("Unknown provider") and its openai client appends `/chat/completions` (not
 *  `/v1/...`), so we use `provider:"openai"` + append `/v1` to the base URL. */
function ollamaModel(id: string, baseUrl?: string | undefined): Model<"openai-completions"> {
  const root = baseUrl ?? "http://localhost:11434";
  const openaiBaseUrl = root.endsWith("/v1") ? root : `${root}/v1`;
  return {
    id,
    name: id,
    api: "openai-completions",
    provider: "openai",
    baseUrl: openaiBaseUrl,
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 8192,
    maxTokens: 4096,
    compat: { supportsStrictTools: false },
  } as unknown as Model<"openai-completions">;
}

function dryRunProvider(config: LlmProviderConfig): LlmProvider {
  return {
    provider: config.provider,
    model: config.model,
    complete: async ({ tools }) => {
      // One toolCall per provided tool with empty args — exercises the maintenance
      // op wiring (the op writes a record per toolCall) without tokens/network.
      const toolCalls: LlmToolCall[] = (tools ?? []).map((t) => ({
        name: t.name,
        arguments: {},
      }));
      return { toolCalls, text: "" };
    },
  };
}
