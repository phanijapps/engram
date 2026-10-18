# Spec: Configurable embedding models

- **Status:** Implemented
- **Owner:** phanijapps
- **Plan:** [plan.md](plan.md)
- **Constrained by:** ADR-0003, ADR-0022
- **Contract:** Rust `EmbeddingProviderConfig`, additive JSON/TOML SDK configuration; portable v1 domain schemas unchanged
- **Shape:** integration
- **Mode:** focused implementation with bounded review, per owner preference

## Objective

Operators select local FastEmbed or explicitly configured Ollama/OpenAI-compatible embedding models and output dimensions through configuration. BGE Small/384 remains the default. Rust SDK, Node SDK and MCP hosts use the same resolved embedding identity and reject incompatible vectors without deleting stored data.

## Assumptions

- Product scope and defaults are approved in the user's “start wit hit” instruction, 2026-10-16.
- Latest main is fbab016; earlier code-intelligence and CI-repair branches are not included (git ancestry check). This change stays based on the requested main branch.
- Existing EmbeddingProviderConfig has model/dimension fields but SQLite and pgvector wiring construct BGE Small unconditionally (bootstrap source reads).
- FastEmbed 5.17.2 exposes model enumeration, model dimensions and model-code metadata (installed crate source). Served model APIs are validated against their official documentation and loopback fixtures.
- User prefers fewer review rounds; use one focused independent design/security pass and one final diff pass, fixing concrete findings.

## Acceptance Criteria

- Default configuration selects FastEmbed BGE Small EN v1.5 with 384 dimensions and preserves existing default query behavior.
- Presets: default, local-alternative (BGE Base/768), code-evaluation (served Qwen3-Embedding-0.6B/1024), multilingual (BGE-M3/1024). Custom model identifiers are unrestricted for served providers and validated against FastEmbed's catalogue for local models.
- JSON and TOML accept partial embedding sections, presets and optional dimensions. Missing dimensions resolve to the selected local model's native size or a served model's observed output. Explicit positive dimensions are validated; reduced dimensions are requested only through an adapter/API that supports that request, never silently padded or sliced.
- Shared override resolver implements explicit CLI/SDK overrides > environment > profile > defaults. Explicit Rust/JSON configs are not secretly overridden by ambient environment; hosts opt into the resolver.
- Environment supports preset, provider, model, dimensions, endpoint, key-variable name and query/passage instructions. CLI overrides are available on MCP. Existing --no-vector still avoids model initialization and HTTP requests.
- Provider options include endpoint, api_key_env, query_instruction, passage_instruction, request_timeout_secs (default 30), batch_size (default 32). Credentials are read only from the named environment variable at construction and are absent from serialized identity and diagnostics.
- Explicit and inferred dimensions are bounded to 1–8192 before index construction (the active local vector adapter maximum).
- Surreal remains a vector-only backend: automatic dimensions and extended inference options fail explicitly rather than reporting an unwired provider. Legacy fixed-dimension configurations retain their behavior.
- HTTP adapters use bounded requests/responses, validate finite nonempty equal-sized vectors and batch cardinality/order, reject invalid protocols/URL credentials, disable redirects and redact transport/server details. HTTP is permitted only for loopback endpoints; remote endpoints require HTTPS.
- Invalid or unsupported model configurations fail explicitly before vector use; no fallback to a differently named model. Model-load unavailability may degrade with an explicit diagnostic, never relabel BGE as another model.
- Model/dimension/instruction/provider-endpoint identity changes fence existing vector indexes. SQLite reports RequiresReindex; pgvector rejects incompatible persisted identity or dimensions with a reindex/new-store instruction. No automatic wipe or schema dimension alteration.
- Configurable models work through SQLite/pgvector recipes and Rust/Node/MCP entry points. MCP capability output includes the actual active embedding-space identity without credentials. Reranker configuration and historical evaluation artifacts are unchanged.

## Boundaries

Always preserve model-space integrity, policy checks, typed errors and backend boundaries. External HTTP integration lives in an adapter; transport hosts forward Rust behavior. Query and passage formatting are distinct.

Ask first for destructive reindexing of shared stores or calls using paid remote credentials; neither is required for implementation verification.

Never truncate arbitrary fixed-size vectors, mix different model spaces, add remote model loading to domain crates, or claim all models run in every runtime. No automatic fallback, store wipe, unrelated CI/compiler/dependency cleanup or accuracy claims.

## Compatibility

Serialized configuration fields are additive and valid existing settings retain their meaning. Validation is tightened: `normalization="cosine"` is rejected (use `l2` for normalized cosine retrieval), and local FastEmbed rejects `normalization="none"` because that runtime produces normalized vectors (omit it or use `l2`). Rust struct literals gain a defaultable options field and may need `..Default::default()` (source compatibility change). Portable domain contracts and generated domain types remain unchanged. Existing default embedding identity remains stable.

## Testing Strategy

TDD: defaults/presets/precedence, dimension rejection/inference, model-space fingerprints and index compatibility. Adapter HTTP integration tests use real loopback requests with deterministic fixture responses including malformed/oversized/error payloads; no credentials or paid services. Goal-based checks cover compilation, generated contracts, docs, neutrality and parity. Manual/live QA exercises the rebuilt MCP over a disposable store with BGE and a second local model when cached, plus a served fixture through the real MCP. Tests distinguish fixture transport verification from model-quality measurements.

## Verification receipt

See [verification.md](verification.md) for functional evidence, real-model smoke results and independently reproduced baseline limitations.
