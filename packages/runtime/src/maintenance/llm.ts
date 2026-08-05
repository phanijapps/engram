//! LLM provider wrapper over the pi-mono SDK (`@earendil-works/pi-ai`). The single
//! seam through which `engram-maintain`'s LLM ops (reflection, contradiction) call a
//! model. Provider-agnostic: default **Anthropic Claude**, switchable to
//! **Ollama**/**OpenAI** via env (`PI_PROVIDER`/`PI_MODEL` + provider key /
//! `OLLAMA_BASE_URL`). **Rust stays LLM-free** — this is TS-only (RFC-0017).
//!
//! Testing: inject `completeOverride` to bypass pi-mono entirely (no tokens, no
//! network). `PI_DRY_RUN=1` is the manual/E2E fallback (one toolCall per tool).

import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import { Type, type Context, type Tool } from "@earendil-works/pi-ai";

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
  /** Test/manual override — bypasses pi-mono entirely. */
  completeOverride?: (opts: LlmCompleteOptions) => Promise<LlmCompleteResult>;
}

export function llmConfigFromEnv(env: NodeJS.ProcessEnv = process.env): LlmProviderConfig {
  const apiKey = env.ANTHROPIC_API_KEY ?? env.OPENAI_API_KEY;
  return {
    provider: env.PI_PROVIDER ?? "anthropic",
    model: env.PI_MODEL ?? "claude-haiku-4-5",
    ...(apiKey !== undefined ? { apiKey } : {}),
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
  const model = models.getModel(config.provider, config.model);
  if (!model) {
    throw new Error(
      `pi-mono: model not found: ${config.provider}/${config.model} — set PI_PROVIDER/PI_MODEL to a built-in model (PI_DRY_RUN=1 to skip)`,
    );
  }
  const auth = config.apiKey !== undefined ? { apiKey: config.apiKey } : undefined;
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
