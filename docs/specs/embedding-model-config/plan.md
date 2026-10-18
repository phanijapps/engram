# Embedding model configuration plan

Implement the approved configuration options on feat/embedding-model-config from main fbab016. Preserve BGE defaults and existing domain schemas. Declining automatic vector migrations and model-quality tuning: each needs separate evidence and shared-store authorization.

## Design

EmbeddingProviderConfig retains its five existing fields plus flattened defaultable EmbeddingOptions. A shared pure override resolver handles presets and partial inputs; dimensions=0 internally denotes automatic resolution, while explicit nonpositive overrides are rejected. Integration bootstrap resolves positive dimensions before opening any vector index. A focused HTTP embedding adapter owns protocol transport and vector validation; existing FastEmbed adapter exposes catalogue-backed selection and dynamic labels. The facade bridge implements EmbeddingProvider. Semantic options fingerprint the embedding space; transport-only settings and credentials do not.

## Tasks

### T1 — shared configuration and identity
Depends on: none. Mode: TDD. Maps to defaults, presets, validation, precedence and compatibility ACs.
Tests: default/legacy JSON, partial profile inference, all presets, CLI/env precedence, malformed dimensions, provider/endpoint constraints and semantic fingerprint changes.
Approach: core/integration/src/embedding_config.rs plus narrow config/lib exports; update struct literals with defaults and document the additive serialized contract before code changes.

### T2 — provider adapters
Depends on: T1 contract shape. Mode: TDD.
Tests: catalogue selection without model loads; invalid dimensions; loopback HTTP request/response protocols, output order, malformed vectors, auth redaction and timeout behavior.
Approach: adapters/sqlite/src/vector FastEmbed selection, adapters/embedding/http for served models; no model/network implementation in domain.

### T3 — backend wiring and transport parity
Depends on: T1, T2. Mode: TDD + live QA.
Tests: SQLite same-size/different-size model switches preserve stored vectors and disable incompatible lane; pgvector persisted space fencing; MCP flag/env parsing; Node resolver and actual embeddings.
Approach: integration provider factory and existing recipe wiring, MCP configuration flags/profile resolution, Node resolver entry point and TS wrapper. Public capability names remain unchanged.

### T4 — verification and docs
Depends on: T3. Mode: goal-based + live QA.
Tests: fmt, workspace check/test, package build/typecheck/test, contracts/docs/neutrality/parity and real MCP model selection in disposable stores. Record baseline failures already present on main separately.
Approach: document presets and overrides, run focused review and live smoke; no commit/push requested yet.

## Progress

- [x] T1 configuration and identity
- [x] T2 provider adapters
- [x] T3 wiring and parity
- [x] T4 verification and documentation

Design-review resolutions: enforce dimensions <=8192 (sqlite-vec limit); explicitly reject Surreal automatic dimensions or extended provider options until inference is wired there.
