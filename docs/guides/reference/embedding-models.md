# Embedding model configuration

Embedding configuration selects the model, inference provider, output dimensions and query/passage instructions used by Engram. The default remains FastEmbed `BAAI/bge-small-en-v1.5` with 384 dimensions and the existing BGE query formatting.

## Presets

| Preset | Provider | Model | Dimensions |
| --- | --- | --- | --- |
| `default` | `fastembed` | `BAAI/bge-small-en-v1.5` | 384 |
| `local-alternative` | `fastembed` | `BAAI/bge-base-en-v1.5` | 768 |
| `multilingual` | `fastembed` | `BAAI/bge-m3` | 1024 |
| `code-evaluation` | `openai-compatible` | `Qwen/Qwen3-Embedding-0.6B` | 1024 |

Presets select evaluation candidates; they do not guarantee retrieval accuracy. `code-evaluation` also sets a code-retrieval query instruction. It requires an explicit endpoint and a separately provisioned model server. Engram does not start or install served models.

FastEmbed accepts models in the installed FastEmbed catalogue. Served providers accept the model identifier understood by their server. Selecting another model does not fall back to BGE Small if that model is unavailable. FastEmbed and HTTP inference require their corresponding compiled features.

## Configuration fields

Fields belong in the `embedding_provider` object in JSON or the `[embedding_provider]` TOML section. Provider options are flattened into that section.

| Field | Default | Meaning |
| --- | --- | --- |
| `preset` | `default` | Starting configuration; supplied fields override the preset. |
| `provider_type` | `fastembed` | `fastembed`, `ollama`, `openai-compatible`; `none` disables provider construction. |
| `model` | `BAAI/bge-small-en-v1.5` | Provider model identifier. |
| `dimensions` | 384 for the default | Explicit output size, 1–8192. Omit to infer when changing models. |
| `endpoint` | Absent | Required for served models. Base URL or embedding endpoint. |
| `api_key_env` | Absent | Name of the environment variable containing the API credential. |
| `query_instruction` | Model default | Prefix applied to queries; an empty string explicitly disables the prefix. |
| `passage_instruction` | Model default | Prefix applied to indexed passages; an empty string explicitly disables the prefix. |
| `request_timeout_secs` | 30 | Request timeout, 1–300 seconds. |
| `batch_size` | 32 | Maximum inference batch size, 1–256. |
| `prompt_profile` | `query` | Embedding-space profile label; available in JSON/TOML. |
| `normalization` | Absent | `none` or `l2`; available in JSON/TOML. |

Explicit dimensions must match the output the adapter receives. Engram never pads or slices fixed-size vectors to satisfy configuration. Native FastEmbed dimensions come from its catalogue. Served providers verify dimensions using one fixed passage during construction, including when dimensions are configured explicitly. Automatic dimensions are omitted in serialized resolved settings until inference determines them.

Reduced dimensions are usable only when the serving API/model honors the requested size. A model configured as 384 dimensions cannot become a 768-dimensional model through configuration.

```toml
[embedding_provider]
preset = "local-alternative"
batch_size = 16
```

A custom Ollama model with inferred dimensions:

```toml
[embedding_provider]
provider_type = "ollama"
model = "my-embedding-model"
endpoint = "http://localhost:11434"
```

A separately hosted Qwen model:

```toml
[embedding_provider]
preset = "code-evaluation"
endpoint = "http://localhost:8000/v1"
# api_key_env = "ENGRAM_MODEL_TOKEN"
```

Endpoint URLs require HTTPS for remote hosts; HTTP is accepted only for loopback hosts. URL credentials, query strings and fragments are rejected. Redirects are disabled. `api_key_env` stores the variable name; credential values are read during provider construction and are excluded from embedding identity and diagnostics.

Ollama base URLs use `/api/embed`; OpenAI-compatible base URLs use `/v1/embeddings`. A complete embedding endpoint is also accepted.

## MCP flags and environment

Resolution order is **explicit CLI overrides > environment > TOML profile > defaults**. An explicit `--embedding-config` path takes precedence over `ENGRAM_EMBEDDING_CONFIG`.

| CLI flag | Environment variable | Value |
| --- | --- | --- |
| `--embedding-config` | `ENGRAM_EMBEDDING_CONFIG` | TOML profile path |
| `--embedding-preset` | `ENGRAM_EMBEDDING_PRESET` | Preset name |
| `--embedding-provider` | `ENGRAM_EMBEDDING_PROVIDER` | Provider identifier |
| `--embedding-model` | `ENGRAM_EMBEDDING_MODEL` | Model identifier |
| `--embedding-dimensions` | `ENGRAM_EMBEDDING_DIMENSIONS` | Positive integer, at most 8192 |
| `--embedding-endpoint` | `ENGRAM_EMBEDDING_ENDPOINT` | Endpoint URL |
| `--embedding-api-key-env` | `ENGRAM_EMBEDDING_API_KEY_ENV` | Credential variable name |
| `--embedding-query-instruction` | `ENGRAM_EMBEDDING_QUERY_INSTRUCTION` | Query prefix |
| `--embedding-passage-instruction` | `ENGRAM_EMBEDDING_PASSAGE_INSTRUCTION` | Passage prefix |
| `--embedding-timeout-secs` | `ENGRAM_EMBEDDING_TIMEOUT_SECS` | Timeout seconds |
| `--embedding-batch-size` | `ENGRAM_EMBEDDING_BATCH_SIZE` | Batch size |

```sh
engram-mcp --storage /tmp/engram-example \
  --embedding-config embedding.toml \
  --embedding-batch-size 8
```

`--no-vector` skips embedding model construction and HTTP calls. Configuration parsing still validates supplied options. The BFF uses the shared environment resolver when `ENGRAM_ENABLE_VECTOR=true`; its existing default `false` skips inference.

## Rust SDK

`EmbeddingProviderConfig::resolve` applies precedence without loading a model. Environment is opt-in for SDK callers; direct explicit configs do not read embedding override variables.

```rust
use engram_integration::{EmbeddingOverrides, EmbeddingProviderConfig};

let profile = EmbeddingProviderConfig::from_toml_file("embedding.toml")?;
let embedding = EmbeddingProviderConfig::resolve(
    Some(profile),
    EmbeddingOverrides::from_env()?,
    EmbeddingOverrides {
        batch_size: Some(8),
        ..Default::default()
    },
)?;
// Pass `embedding` as EngramConfig.embedding_provider.
```

Rust stores the flattened provider fields under `EmbeddingProviderConfig.options` (`EmbeddingOptions`). SDK overrides use `EmbeddingOverrides`. An omitted dimension is represented internally as zero until provider construction; an explicit zero is rejected.

## Node SDK

`resolveEmbeddingConfig` delegates to the Rust resolver through the native addon. Environment overrides are disabled by default and enabled with `useEnvironment: true`. It does not load models or read credential values.

```ts
import { resolveEmbeddingConfig } from "@engram/node";

const embedding = resolveEmbeddingConfig({
  profile: { preset: "local-alternative" },
  useEnvironment: true,
  overrides: { batch_size: 8 },
});
// Include `embedding_provider: embedding` in the native provider's config JSON.
```

An opened provider transport exposes actual inference through its configured Rust provider:

```ts
import { createNativeProviderTransport } from "@engram/node";

// `configJson` is the complete EngramConfig JSON, including embedding_provider.
const provider = createNativeProviderTransport({ configJson });
const inference = provider.embedding();
const space = await inference.embeddingSpace();
const queryVector = await inference.embedQuery("Find the user lookup function");
const passageVector = await inference.embedPassage("function getUser(id) { return db.find(id); }");
const batchVectors = await inference.embedBatch(["first passage", "second passage"]);
```

The helper `createNativeEmbeddingProviderTransport(nativeProvider)` exposes the same operations for callers holding a raw native provider. Both forward query formatting, batch execution and validation to Rust. Missing provider capabilities and model errors propagate to the caller.

The exported types are `EmbeddingConfigOverrides`, `ResolvedEmbeddingConfig` and `ResolveEmbeddingConfigOptions`. Resolved `dimensions` is optional for automatic sizing. The native addon must include `resolveEmbeddingConfig`; an older addon produces a rebuild error.

## Existing stores and errors

Changing the model, dimensions, instructions or served endpoint changes embedding-space identity. Existing vectors remain stored; Engram does not erase them or automatically alter index dimensions. SQLite fences an incompatible vector lane with `RequiresReindex`. Pgvector rejects incompatible persisted space/dimensions with a reindex/new-store diagnostic. Use a separate store or an explicitly planned reindex when changing spaces.

Surreal remains a vector-only backend: it supports legacy fixed-dimension settings but rejects automatic dimensions and extended inference options. Selecting a provider does not add an unwired inference capability to that backend.

Invalid dimensions, unknown presets, unsupported local models, invalid endpoints and mismatched provider output fail explicitly. Model loading may report provider unavailability; it never relabels another model as the requested model. Reranker configuration is independent.

## See also

- [Expanded comparison: 105 questions across six repositories](../../perf/embedding-expanded-comparison-2026-10-16.md)
- [EmbeddingGemma 2 reference comparison and native-runtime status](../../perf/embeddinggemma2-comparison-2026-10-16.md)
- [Measured BGE Small/Base retrieval comparison](../../perf/embedding-model-comparison-2026-10-16.md)
- [Repository indexing](../explanation/how-repos-get-indexed.md)
- [Embedding configuration acceptance criteria](../../specs/embedding-model-config/spec.md)

Normalization accepts omitted/provider-default, `none`, or `l2`. FastEmbed produces normalized vectors and rejects `none`; use its default or `l2`. The historical `cosine` normalization label is rejected: cosine is a similarity metric; choose `l2` for normalization. Model IDs on served endpoints should name immutable model versions; retagging a model under the same ID requires explicit reindexing.
