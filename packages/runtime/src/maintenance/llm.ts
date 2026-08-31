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
/** Tool schema shape for LLM tool-use (name + JSON-schema parameters). */
export type { Tool };

/** One tool invocation the model requested (name + JSON arguments). */
export interface LlmToolCall {
  name: string;
  arguments: Record<string, unknown>;
}

/** Simple completion result: joined text + requested tool calls. */
export interface LlmCompleteResult {
  toolCalls: LlmToolCall[];
  text: string;
}

/** Options for the simple complete() surface. */
export interface LlmCompleteOptions {
  systemPrompt?: string;
  userText: string;
  tools?: Tool[];
}

/** Agent-loop message (user / assistant / toolResult) with content blocks. */
export interface LlmAgentMessage {
  role: "user" | "assistant" | "toolResult";
  content?: unknown;
  toolCallId?: string;
  toolName?: string;
  isError?: boolean;
  timestamp: number;
}

/** One content block: text or toolCall. */
export interface LlmAgentContent {
  type: string;
  text?: string;
  name?: string;
  arguments?: unknown;
  id?: string;
}

/** Agent round-trip result: raw blocks + joined text + parsed tool calls. */
export interface LlmAgentResult {
  content: LlmAgentContent[];
  text: string;
  toolCalls: LlmToolCall[];
}

/** The LLM abstraction: simple complete + agentic completeAgent surfaces. */
export interface LlmProvider {
  readonly provider: string;
  readonly model: string;
  complete(opts: LlmCompleteOptions): Promise<LlmCompleteResult>;
  /** Agentic round-trip: takes a full message history + tools, returns the
   *  raw model response (content blocks: text + toolCall). The caller
   *  executes tool calls + pushes toolResult messages for the next round. */
  completeAgent(opts: {
    systemPrompt?: string;
    messages: LlmAgentMessage[];
    tools?: Tool[];
  }): Promise<LlmAgentResult>;
}

/** Resolved LLM provider configuration (provider/model + optional overrides). */
export interface LlmProviderConfig {
  provider: string;
  model: string;
  apiKey?: string;
  /** Ollama base URL (default http://localhost:11434). Only used when provider=ollama. */
  ollamaBaseUrl?: string;
  /** Test/manual override — bypasses pi-mono entirely. */
  completeOverride?: (opts: LlmCompleteOptions) => Promise<LlmCompleteResult>;
}

/** Resolves the provider config from PI_PROVIDER/PI_MODEL env (defaults anthropic). */
export function llmConfigFromEnv(env: NodeJS.ProcessEnv = process.env): LlmProviderConfig {
  const apiKey = env.ANTHROPIC_API_KEY ?? env.OPENAI_API_KEY;
  return {
    provider: env.PI_PROVIDER ?? "anthropic",
    model: env.PI_MODEL ?? "claude-haiku-4-5",
    ...(apiKey !== undefined ? { apiKey } : {}),
    ...(env.OLLAMA_BASE_URL !== undefined ? { ollamaBaseUrl: env.OLLAMA_BASE_URL } : {}),
  };
}

/** Creates the LLM provider: env config by default, or an injected override
 *  (tests) / PI_DRY_RUN fixture. Exposes complete + completeAgent surfaces. */
export function createLlmProvider(
  config: LlmProviderConfig = llmConfigFromEnv(),
): LlmProvider {
  if (config.completeOverride) {
    return {
      provider: config.provider,
      model: config.model,
      complete: config.completeOverride,
      completeAgent: async ({ systemPrompt, messages, tools }) => {
        // For overrides (tests), delegate to the simple complete in a loop.
        const lastUser = [...messages].reverse().find((m) => m.role === "user");
        const userText = typeof lastUser?.content === "string" ? lastUser.content : JSON.stringify(lastUser?.content ?? "");
        const r = await config.completeOverride!({ ...(systemPrompt !== undefined ? { systemPrompt } : {}), userText, ...(tools !== undefined ? { tools } : {}) });
        return { content: [{ type: "text", text: r.text }, ...r.toolCalls.map((tc) => ({ type: "toolCall", name: tc.name, arguments: tc.arguments }))], text: r.text, toolCalls: r.toolCalls };
      },
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
    completeAgent: async ({ systemPrompt, messages, tools }) => {
      const context: Context = {
        messages: messages as Context["messages"],
        ...(systemPrompt !== undefined ? { systemPrompt } : {}),
        ...(tools && tools.length > 0 ? { tools } : {}),
      };
      const resp = await models.complete(model, context, auth);
      const errMsg2 = (resp as { errorMessage?: unknown }).errorMessage;
      if (typeof errMsg2 === "string" && errMsg2.length > 0) {
        throw new Error(`LLM agent call failed (${config.provider}/${config.model}): ${errMsg2}`);
      }
      const ablocks = (resp.content ?? []) as LlmAgentContent[];
      return {
        content: ablocks,
        text: ablocks.filter((b) => b.type === "text").map((b) => b.text ?? "").join(""),
        toolCalls: ablocks
          .filter((b) => b.type === "toolCall" && typeof b.name === "string")
          .map((b) => ({
            name: b.name as string,
            arguments: (b.arguments as Record<string, unknown> | undefined) ?? {},
          })),
      };
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
      const toolCalls: LlmToolCall[] = (tools ?? []).map((t) => ({
        name: t.name,
        arguments: {},
      }));
      return { toolCalls, text: "" };
    },
    completeAgent: async ({ tools }) => {
      const toolCalls: LlmToolCall[] = (tools ?? []).map((t) => ({
        name: t.name,
        arguments: {},
      }));
      return {
        content: [{ type: "text", text: "" }, ...toolCalls.map((tc) => ({ type: "toolCall", name: tc.name, arguments: tc.arguments }))],
        text: "",
        toolCalls,
      };
    },
  };
}
