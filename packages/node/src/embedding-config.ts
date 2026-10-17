import { loadNativeBinding, type NativeBindingLoader } from "./binding.js";

/** Optional inputs interpreted and validated by the shared Rust resolver. */
export interface EmbeddingConfigOverrides {
  preset?: "default" | "local-alternative" | "code-evaluation" | "multilingual";
  provider_type?: string;
  model?: string;
  dimensions?: number;
  endpoint?: string;
  /** Name of a credential environment variable; never place a secret here. */
  api_key_env?: string;
  query_instruction?: string;
  passage_instruction?: string;
  request_timeout_secs?: number;
  batch_size?: number;
}

/** Serialized Rust configuration, ready for EngramConfig.embedding_provider. */
export interface ResolvedEmbeddingConfig {
  provider_type: string;
  model: string;
  /** Omitted means resolve the native/observed size during provider construction. */
  dimensions?: number;
  prompt_profile: string;
  normalization?: string | null;
  endpoint?: string | null;
  api_key_env?: string | null;
  query_instruction?: string | null;
  passage_instruction?: string | null;
  request_timeout_secs: number;
  batch_size: number;
}

/** Shared Rust resolver inputs, with optional ambient environment overrides. */
export interface ResolveEmbeddingConfigOptions {
  profile?: EmbeddingConfigOverrides & { prompt_profile?: string; normalization?: string | null };
  overrides?: EmbeddingConfigOverrides;
  /** Opt into ENGRAM_EMBEDDING_* overrides. Default false; explicit overrides win. */
  useEnvironment?: boolean;
  bindingLoader?: NativeBindingLoader;
}

/** Resolve defaults, presets and overrides through Rust, without model loading. */
export function resolveEmbeddingConfig(
  options: ResolveEmbeddingConfigOptions = {},
): ResolvedEmbeddingConfig {
  const binding = loadNativeBinding(options.bindingLoader);
  if (!binding.resolveEmbeddingConfig) {
    throw new Error("Native addon does not support embedding configuration. Rebuild @engram/node.");
  }
  return JSON.parse(binding.resolveEmbeddingConfig(
    options.profile === undefined ? null : JSON.stringify(options.profile),
    options.overrides === undefined ? null : JSON.stringify(options.overrides),
    options.useEnvironment ?? false,
  )) as ResolvedEmbeddingConfig;
}
