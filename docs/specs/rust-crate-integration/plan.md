# Plan: Rust Crate Integration Contract

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Implement the integration contract in priority order: provider facade and capability reporting first (bootstrap surface), then embedding provider abstraction, vector index embedding-space validation, public query ports, migration/import API, retrieval trace contract, and conformance harness. Each priority builds on the previous: capability reporting requires repository handles, embedding-space validation requires the provider trait, migration requires stable error types. The riskiest part is the provider facade design — it must balance ergonomic bundling with adapter substitutability while keeping fail-closed behavior for unsupported families. Testing follows the spec strategy: TDD for invariants (capability state transitions, embedding-space validation, error stability), goal-based for typecheck and conformance, manual QA for integration examples.

## Constraints

- Must maintain backward compatibility with existing N-API bindings and repository traits
- Cannot expose SQLite table details or adapter internals as public contract
- Must support both FastEmbed and Ollama embedding providers with extendable design
- Capability codes and error codes must remain stable once released
- Must preserve valid-time vs record-time distinction in belief queries
- Storage schema versions must be visible through diagnostics

## Construction tests

**Integration tests:**
- Provider bootstrap with full configuration returns all repository handles and capability report
- End-to-end embedding-space mismatch rejection across insert and search operations
- Migration dry-run produces deterministic manifest that apply mode consumes
- Full integration example: external Rust application opens provider, discovers capabilities, writes memory, performs retrieval

**Manual verification:**
- External Rust application example compiles and runs with provider facade
- Conformance fixtures pass for all supported capabilities
- Capability report matches actual supported features

## Design (LLD)

### Design decisions
- **Provider facade bundles existing traits, doesn't replace** — EngramProvider wraps existing repository traits (MemoryRepository, KnowledgeRepository, etc.) rather than reimplementing them. New VectorIndex trait added alongside existing RetrievalIndex. Traces to: AC for single provider facade with typed handles.
- **Capability state is fail-closed** — Unsupported families return None for handles rather than panics, capability_report carries Unsupported state with reason code. Applications check before calling rather than discovering at call time. Traces to: AC for explicit capability reporting with stable reason codes.
- **Embedding space identity, not dimensions** — Compatibility requires matching provider + model + dimensions + prompt_profile + normalization, not just vector length. Traces to: AC for embedding-space validation beyond dimensions.
- **Extend existing CoreError, don't duplicate** — Add integration-specific variants to existing CoreError rather than creating parallel EngramError. Add redaction layer to existing error types. Traces to: AC for typed redacted errors.
- **Wrap existing FastEmbed, don't duplicate** — Existing FastEmbedBgeSmallQueryProvider wrapped through new EmbeddingProvider trait rather than creating duplicate implementation. Traces to: AC for embedding provider abstraction.
- **Extend existing FusionTrace, don't duplicate** — Add missing fields (source, discard_reason) to existing FusionTrace rather than creating parallel RetrievalTrace type. Traces to: AC for retrieval trace contract.
- **Valid-time vs record-time explicit** — Belief queries accept BeliefQuery with optional recorded_at filter; repositories that can't answer return existing CoreError::InvalidRequest rather than guessing from current rows. Traces to: AC for valid-time/record-time distinction.
- **Migration manifest fingerprinting** — Dry-run produces SHA-256 fingerprint of row counts and content checksums (key fields only, not full content to avoid timestamp non-determinism); apply mode rejects if fingerprint mismatches. Traces to: AC for migration gating.

### Data & schema
- **CapabilityState enum** — Supported, Unsupported { reason: CapabilityReason }, Degraded { reason: CapabilityReason }, RequiresMigration { reason: CapabilityReason }, RequiresReindex { reason: CapabilityReason }, Misconfigured { reason: CapabilityReason }. Source: capability state contract. Traces to: ACs for capability reporting.
- **CapabilityReport struct** — Includes all 10 capability families: memory, knowledge, graph, ontology, taxonomy, beliefs, hierarchy, retrieval, vectors, migration. Source: integration facade contract. Traces to: AC for explicit capability reporting.
- **EmbeddingSpace struct** — provider: String, model: String, dimensions: u32, prompt_profile: String, normalization: Option<String>. Source: embedding provider contract. Traces to: ACs for embedding-space identity.
- **CapabilityReason codes** — Stable string constants: provider_unavailable, embedding_space_mismatch, dimension_mismatch, record_time_history_unsupported, migration_manifest_stale, storage_path_outside_trusted_root, unsupported_store_family. Source: extended CoreError contract. Traces to: ACs for stable error codes.
- **CoreError extensions** — New variants added to existing CoreError: CapabilityUnsupported, EmbeddingSpaceMismatch, MigrationManifestStale, plus redaction methods to_redacted() and with_diagnostic(). Source: existing error contract in core/runtime. Traces to: ACs for typed redacted errors.

### Interfaces & contracts
- **EngramProvider facade** — One public struct with Arc<dyn Repository> handles for each supported family plus CapabilityReport. Bootstrap accepts EngramConfig. Source: provider facade contract. Traces to: ACs for single provider facade.
- **EmbeddingProvider trait** — provider_id(), model_id(), dimensions(), embedding_space(), embed_query(), embed_passage(), embed_batch() methods. Implementations: FastEmbedProvider, OllamaProvider. Source: embedding provider contract. Traces to: ACs for provider-neutral embeddings.
- **VectorIndex trait** — embedding_space(), insert(), search(), delete_target(), clear() methods with embedding-space validation. Source: vector index contract. Traces to: ACs for embedding-space enforcement.
- **Migration API** — MigrationService trait with dry_run_import(), apply_import() methods, manifest fingerprinting, ValidationReport response. Source: migration contract. Traces to: ACs for dry-run/apply gating.
- **Configuration contract** — EngramConfig struct with storage_path, trusted_root, scope_policy, embedding_provider, migration_mode, capability_policy fields. Source: configuration contract. Traces to: ACs for explicit configuration.

### Component / module decomposition
- **core/integration** (new crate) — Owns EngramProvider, CapabilityReport, EmbeddingProvider trait, FastEmbed provider wrapper, Ollama provider implementation, configuration types, error extensions to CoreError. No storage dependencies, only domain types.
- **core/domain** (existing - extended) — Extended with CapabilityState, CapabilityReason, EmbeddingSpace types. Storage-neutral.
- **core/retrieval** (existing - extended) — New VectorIndex trait added alongside existing RetrievalIndex. Embedding-space validation contracts. Extended FusionTrace with source and discard_reason fields.
- **core/runtime** (existing - extended) — CoreError enum extended with integration-specific variants and redaction methods (to_redacted, with_diagnostic).
- **adapters/retrieval/sqlite-vec** (existing - extended) — Existing FastEmbedBgeSmallQueryProvider wrapped through new EmbeddingProvider trait. Embedding_space metadata storage and validation logic added to SqliteVectorIndex. Implements new VectorIndex trait.
- **adapters/integration** (new crate) — MigrationService implementation, conformance harness fixtures, validation and reporting logic. Depends on domain types and existing repository traits.
- **examples/rust-embedding** (new) — External Rust application example demonstrating provider bootstrap, capability discovery, and basic operations.

### State & control flow
- **Bootstrap flow** — Application creates EngramConfig → calls EngramProvider::bootstrap() → provider constructs repositories, runs capability checks, returns EngramProvider with handles + CapabilityReport → application checks capabilities before using handles.
- **Embedding-space validation flow** — Insert: check embedding_space match → reject on mismatch with RequiresReindex. Search: check query embedding_space against index embedding_space → reject on mismatch.
- **Migration flow** — Host reads source data → maps to import DTOs → calls dry_run_import() → returns ValidationReport + manifest fingerprint → host reviews → calls apply_import() with fingerprint → validates fingerprint still matches → writes or rejects stale manifest.

### Behavior & rules
- **Fail-closed capability checks** — Unsupported repository methods return None for handles, CapabilityReport shows Unsupported state. Application must check capability state before calling; panics on unsupported call are bugs.
- **Embedding-space strict matching** — Provider identity, model identity, dimensions, prompt profile, and normalization must all match. No fallback or compatibility modes.
- **Path confinement** — storage_path resolved against trusted_root, rejects traversal escapes and symlinks outside trusted root. Errors show redacted paths only.
- **Error redaction** — Public errors omit SQL internals, absolute private paths, raw embeddings, private record contents. Local diagnostic context (developer-mode only) carries full detail.

### Failure, edge cases & resilience
- **Unsupported operation graceful degradation** — Methods return typed CoreError::Unsupported rather than panicking or returning empty results. Capability state reflects this.
- **Embedding-space mismatch handling** — Insert rejects with error, capability state transitions to RequiresReindex. Search rejects immediately — no fallback to dimension-only check.
- **Migration manifest staleness** — apply_import() validates fingerprint against current state, rejects with stale_manifest error if mismatch detected. No silent writes.
- **Missing embedding provider** — Capability report shows Degraded state with provider_unavailable reason. Vector operations fail with typed error.
- **Path traversal attacks** — Bootstrap resolves and confines paths before opening storage, rejects symlinks and escapes early. Safe context shows redacted paths only.

### Quality attributes (NFRs)
- **Type safety** — All public surfaces use Rust type system; no string-based API surface. Verified by typecheck gate.
- **Error stability** — Error codes and capability reasons remain stable across releases. New codes added, old codes never changed. Verified by goal-based check.
- **Documentation completeness** — All public traits, structs, and functions have rustdoc examples. Verified by cargo doc gate.
- **Conformance coverage** — Every supported capability has corresponding fixture. Verified by conformance test gate.
- **Schema version visibility** — Storage schema versions visible through provider diagnostic methods (schema_version(), adapter_version()). Verified by integration tests.
- **Migration guidance** — Breaking changes include migration guidance in MIGRATION.md and version-specific documentation. Verified by manual documentation review.

### Dependencies & integration
- **FastEmbed integration** — Depends on fastembed-rs crate. Added as optional dependency; enabled via feature flag.
- **Ollama integration** — Depends on reqwest and tokio for HTTP client. Uses OpenAI-compatible API surface.
- **SQLite vector storage** — Depends on existing rusqlite and sqlite-vec. No new dependencies for storage layer.
- **SHA-256 for manifest fingerprinting** — Uses sha2 crate (already in workspace dependencies).
- **serde for configuration** — Uses existing serde and serde_json for config serialization.

## Tasks

### T1: Add domain types for capability reporting and embedding space

**Depends on:** none

**Tests:**
- CapabilityState enum variants serialize/deserialize correctly
- EmbeddingSpace struct equality and hashing work as expected
- CapabilityReason string constants are stable and compile-time checkable
- EngramErrorCode enum covers all error categories from the contract

**Approach:**
- Add CapabilityState enum to core/domain/src/capability.rs with variants: Supported, Unsupported { reason: CapabilityReason }, Degraded { reason: CapabilityReason }, RequiresMigration { reason: CapabilityReason }, RequiresReindex { reason: CapabilityReason }, Misconfigured { reason: CapabilityReason }
- Add CapabilityReason string constants: provider_unavailable, embedding_space_mismatch, dimension_mismatch, record_time_history_unsupported, migration_manifest_stale, storage_path_outside_trusted_root, unsupported_store_family
- Add EmbeddingSpace struct to core/domain/src/embedding.rs with fields: provider (String), model (String), dimensions (u32), prompt_profile (String), normalization (Option<String>), derive PartialEq, Eq, Hash, Serialize, Deserialize
- Add EngramErrorCode enum to core/domain/src/error.rs with variants covering all error categories from the contract
- Update core/domain/src/lib.rs to re-export new types

**Done when:** Typecheck passes, all new types have serde derives and rustdoc, example code in rustdoc compiles

**Touches:** core/domain/src/capability.rs (new), core/domain/src/embedding.rs (new), core/domain/src/error.rs (modified), core/domain/src/lib.rs (modified)

### T2: Create integration crate structure and provider configuration

**Depends on:** T1

**Mode:** Goal-based check

**Tests:**
- EngramConfig struct serializes/deserializes correctly
- EngramConfig validation rejects empty storage_path and missing trusted_root
- CapabilityPolicy enum handles fail_closed vs omit modes correctly
- MigrationMode enum enforces dry_run before apply requirement

**Approach:**
- Create core/integration crate with Cargo.toml depending on engram-domain and engram-runtime
- Add core/integration/src/config.rs with EngramConfig struct containing: storage_path (PathBuf), trusted_root (PathBuf), scope_policy (ScopeMappingStrategy), embedding_provider (EmbeddingProviderConfig), migration_mode (MigrationMode), capability_policy (CapabilityPolicy)
- Add CapabilityPolicy enum: FailClosed, OmitUnsupported
- Add MigrationMode enum: DryRun, Apply
- Add configuration validation logic in EngramConfig::validate() method
- Add core/integration/src/lib.rs re-exports

**Done when:** Typecheck passes, configuration validation rejects invalid configs, rustdoc examples compile

**Touches:** core/integration/Cargo.toml (new), core/integration/src/config.rs (new), core/integration/src/lib.rs (new)

### T3: Implement capability reporting infrastructure

**Depends on:** T2

**Mode:** Goal-based check

**Tests:**
- CapabilityReport struct construction with all 10 capability states
- CapabilityReport includes ontology and taxonomy families
- CapabilityReport serialization/deserialization preserves state
- is_supported() helper method returns correct boolean for each capability family
- CapabilityReport::all_supported() returns true only when all families are Supported

**Approach:**
- Add core/integration/src/capability.rs with CapabilityReport struct containing: memory (CapabilityState), knowledge (CapabilityState), graph (CapabilityState), ontology (CapabilityState), taxonomy (CapabilityState), beliefs (CapabilityState), hierarchy (CapabilityState), retrieval (CapabilityState), vectors (CapabilityState), migration (CapabilityState)
- Add CapabilityReport::all_supported() helper method
- Add is_supported() helper methods for each capability family
- Add capability state transition logic with validation

**Done when:** Typecheck passes, capability report includes all 10 families, helpers work correctly, rustdoc examples compile

**Touches:** core/integration/src/capability.rs (new), core/integration/src/lib.rs (modified)

### T4: Define EmbeddingProvider trait and wrap existing FastEmbed provider

**Depends on:** T1, T2

**Mode:** TDD

**Tests:**
- EmbeddingProvider trait methods have correct signatures
- FastEmbedProvider wrapper implements EmbeddingProvider trait correctly
- Existing FastEmbedBgeSmallQueryProvider is wrapped, not duplicated
- EmbeddingSpace identity generation is consistent and unique per model
- Wrapper returns correct dimensions and embedding space
- Wrapper handles batch embedding with proper error handling
- No duplication of existing FastEmbed functionality

**Approach:**
- Add core/integration/src/embedding.rs with EmbeddingProvider trait containing: provider_id(), model_id(), dimensions(), embedding_space(), embed_query(), embed_passage(), embed_batch() methods
- Add fastembed_provider.rs module that wraps existing FastEmbedBgeSmallQueryProvider from adapters/retrieval/sqlite-vec/src/fastembed_provider.rs
- Implement EmbeddingProvider trait for the wrapper, delegating to existing provider
- Add EmbeddingError enum for embedding-specific failures
- Add embedding_space() method that constructs EmbeddingSpace from provider configuration
- Add batch embedding logic with proper error handling and result aggregation
- Document relationship between new trait and existing VectorQueryProvider

**Done when:** Typecheck passes, FastEmbed wrapper works with actual fastembed-rs, embedding space identity is consistent, no duplication of existing provider

**Touches:** core/integration/src/embedding.rs (new), core/integration/src/fastembed_provider.rs (new), core/integration/Cargo.toml (modified for fastembed dependency), adapters/retrieval/sqlite-vec/src/fastembed_provider.rs (read for wrapper implementation)

### T5: Implement Ollama-compatible embedding provider

**Depends on:** T4

**Mode:** Manual QA (requires external Ollama instance)

**Tests:**
- OllamaProvider implements EmbeddingProvider trait correctly
- OllamaProvider handles HTTP client errors gracefully
- OllamaProvider generates correct embedding space identity
- OllamaProvider batch embedding works with rate limiting and retry logic
- Retry policy: 3 attempts with exponential backoff (base delay 100ms, max delay 1s)
- Rate limiting: max 5 concurrent batch requests

**Approach:**
- Add core/integration/src/ollama_provider.rs with OllamaProvider implementation
- Use reqwest for HTTP client with OpenAI-compatible API surface
- Add retry logic with 3 attempts, exponential backoff (100ms base, 1s max)
- Add rate limiting for batch requests (max 5 concurrent)
- Implement proper error handling for network failures
- Document retry policy and rate limits in rustdoc

**Done when:** Typecheck passes, Ollama provider works with actual Ollama instance, embedding space identity is consistent, retry and rate limiting documented

**Touches:** core/integration/src/ollama_provider.rs (new), core/integration/Cargo.toml (modified for reqwest dependency)

### T6: Introduce VectorIndex trait with embedding-space validation

**Depends on:** T1, T4

**Mode:** TDD

**Tests:**
- VectorIndex trait has embedding_space() method returning EmbeddingSpace
- VectorIndex::insert() validates embedding space before insertion
- VectorIndex::search() validates query embedding space before search  
- Mismatched embedding spaces return proper errors with capability state transition
- VectorIndex trait is distinct from existing RetrievalIndex trait
- Trait clearly documents relationship to existing VectorQueryProvider/VectorRetrievalIndex

**Approach:**
- Add new VectorIndex trait to core/retrieval/src/vector_index.rs (new file)
- Define VectorIndex with embedding_space(), insert(), search(), delete_target(), clear() methods
- Add embedding-space validation contracts to insert() and search() methods
- Document relationship to existing RetrievalIndex and adapter-local VectorQueryProvider
- Add embedding-space mismatch error variant to extended CoreError
- Update capability state transition logic for embedding-space mismatches

**Done when:** Typecheck passes, VectorIndex trait defined as separate from RetrievalIndex, embedding-space validation contract clearly defined, relationship to existing adapter seams documented

**Touches:** core/retrieval/src/vector_index.rs (new), core/retrieval/src/lib.rs (modified)

### T7: Implement embedding-space validation in sqlite-vec adapter

**Depends on:** T6

**Tests:**
- SqliteVectorIndex stores embedding_space metadata
- SqliteVectorIndex::insert() rejects mismatched embedding spaces
- SqliteVectorIndex::search() rejects queries with mismatched embedding spaces
- Capability state transitions to RequiresReindex when embedding space changes

**Approach:**
- Update adapters/retrieval/sqlite-vec/src/index.rs to add embedding_space column to vectors table
- Add embedding_space storage and retrieval logic
- Add embedding-space validation in insert() method
- Add embedding-space validation in search() method
- Add capability state tracking for embedding-space changes
- Update table creation logic to include embedding_space field

**Done when:** Typecheck passes, embedding-space validation works in integration tests, capability state transitions correctly

**Touches:** adapters/retrieval/sqlite-vec/src/index.rs (modified)

### T8: Implement EngramProvider facade

**Depends on:** T2, T3, T6, T7, T12

**Mode:** Goal-based check

**Tests:**
- EngramProvider::bootstrap() constructs all repository handles
- EngramProvider returns None for unsupported repository handles
- EngramProvider::capabilities() returns correct CapabilityReport
- EngramProvider rejects invalid configuration with proper errors
- EngramProvider path confinement works correctly
- Capability detection calls conformance fixtures before marking capabilities as Supported

**Approach:**
- Add core/integration/src/provider.rs with EngramProvider struct containing Arc<dyn Repository> handles for each supported family plus CapabilityReport
- Add EngramProvider::bootstrap() method accepting EngramConfig
- Add repository construction logic for each supported family
- Add capability detection logic that calls conformance fixtures from T12
- Add fail-closed behavior for unsupported families
- Add path confinement logic in bootstrap
- Wire capability detection to fixture execution so Supported requires passing fixtures

**Done when:** Typecheck passes, bootstrap creates all supported handles, capability report accurate and fixture-gated, rustdoc examples compile

**Touches:** core/integration/src/provider.rs (new), core/integration/src/lib.rs (modified)

### T9: Extend CoreError with integration variants and redaction

**Depends on:** T1, T2

**Mode:** Goal-based check

**Tests:**
- CoreError enum extended with integration-specific variants
- CoreError::to_redacted() removes SQL internals, absolute paths, raw embeddings
- CoreError::with_diagnostic() preserves full detail for local development
- Existing CoreError variants remain unchanged (backward compatibility)
- Error code stability maintained across releases
- Error conversion from existing CoreError uses work as expected

**Approach:**
- Extend core/runtime/src/lib.rs CoreError enum with new variants: CapabilityUnsupported { capability: String, reason: String }, EmbeddingSpaceMismatch { expected: String, actual: String }, MigrationManifestStale { expected: String, actual: String }
- Add to_redacted() method to CoreError that removes SQL internals, absolute paths, raw embeddings
- Add with_diagnostic() method to CoreError that preserves full detail for local development
- Add conversion logic from existing CoreError variants for integration-specific errors
- Document error code stability guarantees in rustdoc
- Ensure all new error variants follow existing CoreError patterns

**Done when:** Typecheck passes, error redaction works correctly, local diagnostic mode preserves detail, rustdoc examples compile, existing CoreError usage unchanged

**Touches:** core/runtime/src/lib.rs (modified), core/integration/src/lib.rs (re-exports)

### T10: Implement migration/import API with dry-run and apply gating

**Depends on:** T1, T8, T9

**Mode:** TDD

**Tests:**
- MigrationService trait has dry_run_import() and apply_import() methods
- Dry-run produces ValidationReport with row counts and manifest fingerprint
- Fingerprint computed from row counts + content hashes (key fields only, not full content to avoid timestamp non-determinism)
- Apply mode validates manifest fingerprint before writing
- Stale manifest rejection works correctly
- Scope translation report is accurate
- Provider exposes schema_version() and adapter_version() diagnostic methods

**Approach:**
- Add core/integration/src/migration.rs with MigrationService trait containing: dry_run_import(), apply_import() methods
- Add ValidationReport struct with row counts, unsupported mappings, scope translation, embedding-space validation results
- Add manifest fingerprinting using SHA-256 of row counts + content hashes (key fields: IDs, scopes, timestamps)
- Add stale manifest rejection logic
- Add import DTO types for migration data
- Add schema_version() and adapter_version() methods to provider facade
- Create adapters/integration crate with SqlMigrationService implementation

**Done when:** Typecheck passes, dry-run produces deterministic reports, apply mode gates correctly, schema version methods work, rustdoc examples compile

**Touches:** core/integration/src/migration.rs (new), adapters/integration/src/lib.rs (new), adapters/integration/Cargo.toml (new), core/integration/src/provider.rs (modified for diagnostic methods)

### T11: Extend FusionTrace with retrieval trace fields

**Depends on:** T1, T8

**Mode:** TDD

**Tests:**
- FusionTrace extended with source, score, rank, fusion, discard_reason fields
- RetrievalResponse contains extended trace alongside results
- Extended trace output is stable and deterministic
- Trace omits private internals by default
- Local diagnostic mode includes full detail
- Existing FusionTrace fields remain unchanged (backward compatibility)

**Approach:**
- Extend core/domain/src/retrieval.rs FusionTrace struct with new fields: source (String), score (f32), rank (u32), discard_reason (Option<String>)
- Add RetrievalResponse struct containing results (Vec<RetrievalResult>), trace (FusionTrace), capabilities_used (Vec<String>), degraded (Vec<CapabilityReason>)
- Add trace generation logic in retrieval fusion to populate new fields
- Add privacy logic for trace output (omit private internals by default)
- Add local diagnostic mode for full detail traces
- Document relationship between new fields and existing fusion field

**Done when:** Typecheck passes, extended trace output is deterministic, privacy logic works correctly, existing FusionTrace usage unchanged, rustdoc examples compile

**Touches:** core/domain/src/retrieval.rs (modified), core/retrieval/src/composer.rs (modified)

### T12: Create conformance harness for capability verification

**Depends on:** T8, T10, T11

**Mode:** Goal-based check

**Tests:**
- Conformance harness fixture groups exist for all 10 capability families (including ontology/taxonomy)
- Fixtures pass for all supported capabilities
- Capability reporting ties to fixture existence and execution results
- Unsupported capabilities lack corresponding fixtures
- Fixtures are deterministic and repeatable
- Harness exposes capability probing entry point for provider bootstrap

**Approach:**
- Add adapters/integration/src/conformance.rs with ConformanceHarness struct
- Add fixture methods for: scope_isolation, memory_lifecycle, knowledge_operations, graph_operations, ontology_taxonomy_operations, beliefs, hierarchy, vector_validation, retrieval_trace_determinism, migration_gating
- Add capability-to-fixture mapping logic with all 10 families
- Add fixture execution harness with deterministic setup
- Add fixture result reporting
- Add probe_capabilities() entry point that provider bootstrap calls
- Create integration tests in adapters/integration/tests/

**Done when:** Typecheck passes, all 10 capability families have fixtures, harness exposes probe_capabilities(), conformance tests are deterministic

**Touches:** adapters/integration/src/conformance.rs (new), adapters/integration/tests/ (new)

### T13: Create external Rust application example

**Depends on:** T7, T8, T11

**Mode:** Manual QA

**Tests:**
- Example Rust application compiles and runs
- Example demonstrates provider bootstrap correctly
- Example shows capability discovery before usage
- Example demonstrates memory write and retrieval
- Example demonstrates embedding-space validation (T7)
- Example demonstrates retrieval with trace output (T11)
- Example shows all 10 capability families in CapabilityReport

**Approach:**
- Create examples/rust-embedding/Cargo.toml depending on engram-integration
- Create examples/rust-embedding/src/main.rs demonstrating:
  - EngramConfig construction
  - EngramProvider bootstrap
  - CapabilityReport discovery and checking for all families
  - Memory write operation
  - Retrieval operation with trace inspection
  - Embedding-space validation
- Add comprehensive rustdoc comments explaining usage
- Add README with setup and run instructions

**Done when:** Example compiles and runs successfully, demonstrates embedding-space validation and retrieval traces, shows all 10 capability families, rustdoc is comprehensive

**Touches:** examples/rust-embedding/Cargo.toml (new), examples/rust-embedding/src/main.rs (new), examples/rust-embedding/README.md (new)

### T14: Update N-API bindings to use integration facade

**Depends on:** T8

**Mode:** Goal-based check

**Tests:**
- N-API bindings compile and work with new integration facade
- Native*Engine structs use EngramProvider internally
- Existing N-API TypeScript test suite passes (`cargo test --package bindings/node`)
- TypeScript examples continue to work
- Public TypeScript API surface unchanged
- Backward compatibility is maintained

**Approach:**
- Update bindings/node/src/memory.rs to use EngramProvider for service construction
- Update bindings/node/src/knowledge.rs to use EngramProvider
- Update bindings/node/src/belief.rs to use EngramProvider
- Update bindings/node/src/hierarchy.rs to use EngramProvider
- Update other engine files as needed
- Maintain public API compatibility for TypeScript
- Add internal capability checking logic
- Run existing N-API test suite to verify no regressions

**Done when:** N-API bindings compile and work, `cargo test --package bindings/node` passes, TypeScript examples work, public API unchanged

**Touches:** bindings/node/src/memory.rs (modified), bindings/node/src/knowledge.rs (modified), bindings/node/src/belief.rs (modified), bindings/node/src/hierarchy.rs (modified)

## Rollout

- **Delivery:** Big bang for the integration contract — all components ship together in a coordinated release. External applications adopt the new facade in a single version upgrade. The integration facade is additive; existing N-API bindings remain compatible.
- **Infrastructure:** No new infrastructure required. This is a library contract, not a deployed service. External applications manage their own storage and runtime.
- **External-system integration:** FastEmbed requires model downloads on first use (cached locally). Ollama requires external Ollama instance for embedding provider. Both are optional — applications choose their embedding strategy.
- **Deployment sequencing:** No deployment sequencing required. Library contract ships as a single coordinated release. Applications adopt at their own pace. N-API binding updates maintain backward compatibility.

## Risks

- **Capability state explosion** — If capability states become too granular, the CapabilityReport becomes complex to use. Mitigated by keeping capability families coarse-grained (8 families) and reason codes stable.
- **Embedding-space validation strictness** — Strict embedding-space matching may break existing workflows that rely on dimension-only compatibility. Mitigated by clear error messages and RequiresReindex capability state with migration guidance.
- **Performance overhead** — Capability checking on every operation could impact performance. Mitigated by caching capability state in provider handles and using compile-time type checking where possible.
- **N-API binding compatibility** — Changes to internal service construction could break TypeScript bindings. Mitigated by maintaining public API compatibility and using integration facade internally only.

## Changelog

- 2026-07-06: initial plan following integration contract practical priority order
- 2026-07-06: fixed based on adversarial review findings — added RFC-0011, fixed Constrained by section to cite ADRs, fixed Contract section, reconciled with existing codebase (CoreError extension instead of parallel EngramError, VectorIndex as new trait not update to existing, FastEmbed wrapper not duplication, FusionTrace extension not parallel type), added ontology/taxonomy to CapabilityReport, added Mode fields to tasks, fixed T8 dependency to include T12, added verification modes to all tasks, specified Ollama retry policy, added schema version diagnostics, clarified migration fingerprint scope, fixed T13 dependencies, added N-API test verification, updated design decisions section
