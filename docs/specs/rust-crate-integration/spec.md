# Spec: Rust Crate Integration Contract

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0009 (retrieval composition seam), ADR-0010 (decompose god modules), ADR-0014 (memory record embedding surface), RFC-0011 (rust crate integration)
- **Brief:** none
- **Contract:** core/integration (CapabilityState, CapabilityReason, EmbeddingSpace, EngramConfig, CapabilityReport), core/domain (extended error types with stable codes), core/retrieval (VectorIndex trait, RetrievalTrace)
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram provides a stable Rust crate integration contract that enables external applications to embed the memory engine without adopting Engram's storage layout, runtime policy, or UI assumptions. The integration boundary introduces a provider facade that bundles existing repository traits (MemoryRepository, KnowledgeRepository, KnowledgeGraphRepository, OntologyRepository, TaxonomyRepository, BeliefRepository, HierarchyRepository, RetrievalIndex) with explicit capability reporting. A new VectorIndex trait provides raw vector operations with embedding-space validation beyond dimensions. The contract extends existing CoreError types with redaction and stable codes rather than creating parallel error types. Provider-neutral embedding generation supports the existing FastEmbed provider and adds Ollama compatibility through an extensible EmbeddingProvider trait. Migration/import APIs with dry-run/apply gating enable safe data import without requiring adapter internals. Conformance fixtures mechanically verify capability claims before features are marked as supported.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Maintain separation between storage-neutral domain types and adapter-specific persistence
- Keep vector compatibility based on embedding space identity (provider + model + dimensions), not dimensions alone
- Return typed unsupported errors for unimplemented operations instead of silent empty results
- Preserve valid-time vs record-time distinction in belief queries
- Apply scope and policy checks on write, retrieve, ingest, consolidate, and forget paths
- Return redacted errors that omit private internals (SQL, embeddings, absolute paths, record contents)
- Make capability reporting explicit and machine-readable with stable reason codes
- Keep provider traits extendable for future embedding providers beyond FastEmbed and Ollama

### Ask first

- Adding new public surface beyond what the integration contract specifies
- Changing capability state codes or error codes once released
- Modifying the embedding space identity structure
- Adding new repository traits to the provider facade
- Changing the bootstrap/configuration contract

### Never do

- Expose SQLite table details as the public integration contract
- Require host applications to preserve prior database layouts
- Make vector dimensions the only compatibility guard between embeddings
- Hide unsupported behavior behind fallback behavior that changes semantics
- Make applications depend on private adapter internals to migrate data
- Answer record-time queries from current-row timestamps (must return existing CoreError::InvalidRequest)
- Return raw SQL internals, absolute private paths, raw embeddings, or private record contents in public errors
- Create parallel error types when extending existing CoreError is sufficient
- Duplicate existing FastEmbed provider implementation instead of wrapping it through new trait

## Testing Strategy

- **Provider facade and capability reporting**: TDD — core invariants around capability state transitions, fail-closed behavior, and handle availability. Verified by integration tests that bootstrap provider with various configurations and check capability report accuracy.
- **Embedding provider trait**: TDD — embedding space identity validation, dimension checking, provider interchangeability. Verified by unit tests for EmbeddingSpace equality/hashing and integration tests with actual FastEmbed/Ollama providers.
- **Vector index embedding-space validation**: TDD — rejection of mismatched embedding spaces, dimension validation, RequiresReindex capability state. Verified by integration tests that attempt mismatched insert/search operations and verify proper errors and capability state transitions.
- **Public query ports**: Goal-based check — typecheck confirms trait methods are accessible, integration tests verify scope isolation. Verified by `cargo check --workspace` and integration tests for each repository family.
- **Migration/import API**: TDD — dry-run validation, manifest fingerprinting, stale rejection, gated apply mode. Verified by unit tests for fingerprint computation and integration tests for full migration workflows.
- **Retrieval trace contract**: TDD — trace output stability, source/score/rank/fusion/discard reason fields. Verified by unit tests for RetrievalTrace serialization and integration tests for trace generation accuracy.
- **Conformance harness**: Goal-based check — fixture execution passes, capability reporting tied to fixture existence. Verified by conformance test suite execution and capability detection logic tests.
- **Configuration contract**: TDD — path confinement, symlink rejection, redacted error messages. Verified by unit tests for path validation and error redaction.
- **Error contract**: Goal-based check — error code stability, safe message formatting, context redaction. Verified by `cargo check --workspace` for error type stability and unit tests for redaction logic.
- **External Rust application example**: Manual QA — example compiles and runs successfully. Verified by `cargo build --examples` and manual execution walkthrough.
- **N-API binding compatibility**: Goal-based check — TypeScript examples continue to work. Verified by existing N-API test suite and `cargo test` in bindings/node.

## Acceptance Criteria

- [ ] A host application opens one provider facade from one configuration object (storage_path, trusted_root, scope_policy, embedding_provider, migration_mode, capability_policy)
- [ ] The provider returns a structured CapabilityReport before any worker or route needs to use features, with each feature family (memory, knowledge, graph, ontology, taxonomy, beliefs, hierarchy, retrieval, vectors, migration) in Supported/Unsupported/Degraded/RequiresMigration/RequiresReindex/Misconfigured state with stable reason codes
- [ ] The provider exposes typed repository handles (Arc<dyn MemoryRepository>, Arc<dyn KnowledgeRepository>, Arc<dyn KnowledgeGraphRepository>, Arc<dyn OntologyRepository>, Arc<dyn TaxonomyRepository>, Arc<dyn BeliefRepository>, Arc<dyn HierarchyRepository>, Arc<dyn RetrievalIndex>) for supported families
- [ ] New VectorIndex trait provides raw vector operations (embedding_space, insert, search, delete_target, clear) with embedding-space validation; implementations reject mismatched embedding spaces and transition to RequiresReindex capability state when configured space changes
- [ ] Unsupported operations return typed CoreError variants rather than silent empty results
- [ ] EmbeddingProvider trait supports provider-neutral embedding generation with provider_id, model_id, dimensions, and embedding_space identity
- [ ] EmbeddingProvider implementations include existing FastEmbed provider (wrapped through new trait) and new Ollama-compatible provider
- [ ] Existing FastEmbedBgeSmallQueryProvider is wrapped/exposed through EmbeddingProvider trait without duplication
- [ ] Vector insert operations validate embedding space (provider + model + dimensions + prompt_profile + normalization) and reject mismatched spaces
- [ ] Vector search operations validate query embedding space and reject queries with mismatched spaces  
- [ ] Vector indexes support explicit creation of separate indexes per embedding space
- [ ] Memory repository exposes existing public read/write ports (write_memory, retrieve, forget) with extended error redaction
- [ ] Knowledge repository exposes existing public read/write ports (put_source, put_document, put_chunk, get_chunk, put_entity, put_relationship, get_entity, get_relationship) without leaking adapter schema details
- [ ] KnowledgeGraphRepository exposes existing graph traversal (neighbors, list_graphs) without exposing private internals
- [ ] OntologyRepository and TaxonomyRepository are included in capability reporting and provider handles
- [ ] Belief repository maintains existing valid-time vs record-time distinction, returning existing CoreError::InvalidRequest for unimplemented temporal queries  
- [ ] CoreError is extended with new integration-specific variants (CapabilityUnsupported, EmbeddingSpaceMismatch, MigrationManifestStale) while maintaining existing error surface
- [ ] CoreError includes redaction layer (to_redacted()) that removes SQL internals, absolute paths, raw embeddings, and private record contents from public errors
- [ ] CoreError includes local diagnostic mode (with_diagnostic()) for development with full detail
- [ ] Hierarchy repository exposes existing first-class generic concept (create/update nodes, attach children, query ancestors/descendants, compute paths, summarize subtrees) separate from graph side-effects
- [ ] Retrieval composes multiple candidate sources (semantic vector, keyword/text, graph-neighborhood, belief, hierarchy) with extended trace output
- [ ] RetrievalTrace extends existing FusionTrace with source, score, rank, fusion, and discard_reason fields for observability
- [ ] Retrieval trace is stable enough for observability but does not include private row contents or raw embeddings unless explicitly requested through safe local diagnostic mode
- [ ] Migration/import API provides dry-run validation, deterministic validation report, unsupported mapping report, row counts by family, scope translation report, embedding-space validation, and target DB path validation
- [ ] Migration apply mode is gated by dry-run manifest fingerprinting (SHA-256 of row counts and content hashes) with stale manifest rejection
- [ ] Migration writes no destructive changes unless explicitly requested
- [ ] Errors expose extended CoreError with stable codes, safe human messages, optional redacted context, and optional local-only diagnostic context
- [ ] Errors omit raw SQL internals, absolute private paths, raw embeddings, and private record contents from public messages
- [ ] Conformance fixtures exist for scope isolation, memory lifecycle, knowledge operations, graph operations, ontology/taxonomy operations, beliefs, hierarchy, vector validation, retrieval trace determinism, and migration gating
- [ ] Each supported capability has a corresponding fixture that proves it before capability reporting marks it as supported
- [ ] Capability detection calls conformance fixtures during bootstrap so Supported state requires passing fixtures
- [ ] Public Rust traits, DTOs, and capability codes follow semver versioning
- [ ] Error codes and capability reasons remain stable across releases
- [ ] Storage schema versions are visible through provider diagnostics methods
- [ ] Breaking changes include migration guidance and version compatibility documentation

## Assumptions

- Technical: Runtime is Rust workspace edition 2024 (source: Cargo.toml)
- Technical: Storage-neutral domain types exist in core/domain (source: core/domain/src/lib.rs)
- Technical: 9+ repository traits already defined: MemoryRepository, MemoryEventRepository, KnowledgeRepository, KnowledgeGraphRepository, OntologyRepository, TaxonomyRepository, BeliefRepository, HierarchyRepository, RetrievalIndex (source: grep of core/*/src/lib.rs)
- Technical: SQLite adapters exist for all major subsystems (source: adapters/memory/sqlite, adapters/knowledge/sqlite, adapters/hierarchy/sqlite)
- Technical: N-API bindings exist with Native*Engine pattern (source: bindings/node/src/lib.rs)
- Technical: Vector index exists with sqlite-vec and existing FastEmbedBgeSmallQueryProvider (source: adapters/retrieval/sqlite-vec)
- Technical: No existing capability discovery system (source: grep confirmed no capability reporting patterns exist)
- Technical: Existing FusionTrace defined in core/domain/src/retrieval.rs (source: confirmed by code inspection)
- Technical: Existing EmbeddingRef defined in core/domain/src/knowledge.rs (source: confirmed by code inspection)
- Technical: Existing CoreError defined in core/runtime/src/lib.rs (source: confirmed by code inspection)
- Product: Integration requirement came from external application needing to embed Engram (source: user confirmation 2026-07-06)
- Product: Embedding provider support is FastEmbed and Ollama, must be extendable for future providers (source: user confirmation 2026-07-06)
- Product: Host applications need structured CapabilityReport at bootstrap time for feature discovery (source: user confirmation 2026-07-06)
- Process: RFC process governs convention changes (source: docs/CONVENTIONS.md §3)
- Process: Spec status lifecycle is Draft → Implementing → Shipped (source: docs/CONVENTIONS.md §4)
- Process: User is ultimate authority for Boundaries sign-off (source: user confirmation 2026-07-06)
