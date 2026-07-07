# RFC-0011: Rust crate integration contract for external embedding

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** TBD
- **Date opened:** 2026-07-06
- **Decision weight:** heavy
- **Related:** ADR-0009 (retrieval composition seam), ADR-0010 (decompose god modules), ADR-0014 (memory record embedding surface); spec [`rust-crate-integration`](../specs/rust-crate-integration/spec.md)

## Reviewer brief

- **Decision:** adopt a stable Rust crate integration contract that enables external applications to embed Engram without adopting storage layout, runtime policy, or UI assumptions.
- **Recommended outcome:** accept with feedback.
- **Change if accepted:**
  1. New `core/integration` crate for provider facade, capability reporting, and embedding provider abstraction
  2. New `adapters/integration` crate for migration service and conformance harness
  3. Extension of existing repository traits and error types to support capability discovery
  4. Public `EngramProvider` facade with typed repository handles and structured capability reporting
  5. Embedding provider abstraction supporting FastEmbed and Ollama with extendable design
  6. Migration/import API with dry-run validation and manifest fingerprinting
  7. Conformance harness tied to capability reporting
  8. External Rust application example demonstrating integration contract
- **Affected surface:** `core/` (new integration crate, extended domain types), `adapters/` (new integration crate, extended retrieval adapter), `bindings/node` (updated to use integration facade internally), `docs/` (RFC, spec, examples)
- **Stakes:** costly-to-reverse (this becomes the public integration contract) but not one-way — adapters are additive, and backward compatibility with existing N-API bindings is maintained
- **Review focus:** (1) the integration contract maintains separation between storage-neutral domain and adapter-specific persistence; (2) fail-closed behavior for unsupported capabilities; (3) embedding-space identity validation beyond dimensions; (4) capability reporting is stable and machine-readable
- **Not in scope:** non-SQLite storage backends (Postgres, Neo4j) — those are documented as future adapter additions, not built in this RFC. Also out: distributed deployment, clustering, advanced reranking beyond existing RRF.

## The ask

**Recommendation (BLUF):** adopt a stable Rust crate integration contract with a provider facade that bundles repository handles, exposes capability reporting, supports multiple embedding providers, and provides migration/import APIs. This makes Engram embeddable as a Rust library while maintaining adapter substitutability and fail-closed behavior. **This RFC implements the integration contract foundation only**; actual external application integrations (AgentZero, others) are follow-on work.

**Why now (SCQA):** An external integrating product came with a requirement to embed Engram as a Rust crate and manage the memory layer independently. Currently, Engram has well-defined repository traits internally but no stable public facade for external applications. The N-API bindings work but are Node-specific. Embedding providers are hard-coded to FastEmbed in the sqlite-vec adapter. No capability discovery exists — applications must call operations to discover what's unsupported. Migration requires understanding private adapter internals. The question: **what is the minimal, stable contract that makes Engram embeddable while preserving adapter substitutability?**

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Create `core/integration` crate for provider facade and capability reporting | Accept | Centralizes integration contract without affecting storage-neutral domain; provides single bootstrap surface | Acceptance | Confirm crate location and responsibility |
| D2 | Provider facade returns typed repository handles + CapabilityReport | Accept | Fail-closed capability discovery before operations; typed handles prevent manual adapter stitching | Acceptance | Confirm handle types and capability structure |
| D3 | Extend existing `CoreError` rather than create parallel error type | Accept | Maintains single error surface across all repository traits; redaction added to existing error types | Acceptance | Confirm error extension approach |
| D4 | Introduce `VectorIndex` trait alongside existing `RetrievalIndex` | Accept | Current `RetrievalIndex` is for candidate retrieval; `VectorIndex` is for raw vector operations with embedding-space validation | Acceptance | Confirm trait separation and relationship |
| D5 | Embedding provider abstraction supports FastEmbed + Ollama with extendable design | Accept | Meets integrating product's requirement for multiple providers; trait-based design allows future additions | Acceptance | Confirm provider interface and extensibility |
| D6 | Embedding-space identity beyond dimensions (provider + model + profile + normalization) | Accept | Prevents silent incompatibility between different embedding providers with same dimensions | Acceptance | Confirm embedding-space structure and validation |
| D7 | Migration/import API with dry-run validation and manifest fingerprinting | Accept | Enables safe data migration without requiring adapter internals; SHA-256 fingerprinting prevents stale writes | Acceptance | Confirm migration API surface and fingerprint scope |
| D8 | Capability reporting tied to conformance fixture execution | Accept | Ensures supported capabilities are mechanically proven before being marked Supported | Acceptance | Confirm capability-fixture wiring |
| D9 | Maintain backward compatibility with existing N-API bindings | Accept | N-API bindings use integration facade internally; public TypeScript API unchanged | Acceptance | Confirm backward compatibility approach |
| D10 | Constrain changes by existing ADR-0009, ADR-0010, ADR-0014 | Accept | Retention composition seam, god module decomposition, and memory embedding surface decisions must be respected | Acceptance | Confirm ADR compliance |

## Problem & goals

**Problem.** External applications need to embed Engram as a Rust crate but currently lack a stable public integration contract. Existing challenges:

1. **No provider facade** — Applications must manually stitch together SQLite adapters, understand internal configuration, and manage repository construction
2. **No capability discovery** — Unsupported behavior is discovered at call time through errors, not before starting workers/routes
3. **Hard-coded embedding** — FastEmbed is the only embedding provider; switching providers requires adapter changes
4. **Vector dimension-only compatibility** — Current vector indexes check only dimensions, not full embedding space identity
5. **No migration contract** — Importing data requires understanding private SQLite table structures
6. **No conformance verification** — No way to mechanically verify that an adapter supports claimed capabilities

**Goals.**
- Stable public Rust crate integration contract for external embedding
- Single provider facade with typed repository handles and capability reporting
- Provider-neutral embedding generation with extendable provider design
- Embedding-space validation beyond dimensions (provider + model + profile + normalization)
- Migration/import API with dry-run validation and manifest fingerprinting
- Conformance harness tied to capability reporting
- Backward compatibility with existing N-API bindings
- Fail-closed behavior for unsupported capabilities

## Alternatives considered

### 1. Status quo (N-API bindings only)
**Rejected.** N-API bindings are Node-specific and don't address Rust crate embedding requirements. External Rust applications can't use Node bindings.

### 2. Expose SQLite adapters directly as public API
**Rejected.** Violates storage-neutral domain principle. Locks Engram into SQLite-specific storage layout. Makes future Postgres/Neo4j adapters break the public contract.

### 3. Capability discovery at call time only
**Rejected.** Forces applications to wrap every operation in try/catch for unsupported operations. Makes it impossible to fail-closed at startup when incompatible configuration is detected.

### 4. Dimension-only vector compatibility
**Rejected.** Allows silent incompatibility between different embedding providers with same dimensions. 384-dimensional FastEmbed vectors and 384-dimensional OpenAI vectors are not interchangeable.

## Design proposal

### Crate structure

**New `core/integration` crate** (storage-neutral):
- `EngramProvider` facade with typed repository handles
- `CapabilityReport` with structured capability states
- `EmbeddingProvider` trait with FastEmbed and Ollama implementations
- `EngramConfig` for provider configuration
- Extended domain types: `CapabilityState`, `EmbeddingSpace`, `CapabilityReason`
- Error extensions to existing `CoreError` with redaction layer

**New `adapters/integration` crate** (adapter-specific):
- `MigrationService` implementation with dry-run/apply gating
- `ConformanceHarness` with fixture groups per capability family
- Validation and reporting logic

**Extended `core/domain` crate**:
- `CapabilityState` enum (Supported, Unsupported, Degraded, RequiresMigration, RequiresReindex, Misconfigured)
- `EmbeddingSpace` struct (provider, model, dimensions, prompt_profile, normalization)
- `CapabilityReason` string constants (stable codes)
- Extended error types with redaction support

**Extended `core/retrieval` crate**:
- New `VectorIndex` trait alongside existing `RetrievalIndex`
- Embedding-space validation on insert and search operations
- Extended `RetrievalTrace` with source/score/rank/fusion/discard_reason fields

**Extended `adapters/retrieval/sqlite-vec` crate**:
- `EmbeddingSpace` metadata storage and validation
- Capability state tracking for embedding-space changes
- Implementation of new `VectorIndex` trait

### Provider facade contract

```rust
pub struct EngramProvider {
    pub memory: Option<Arc<dyn MemoryRepository>>,
    pub knowledge: Option<Arc<dyn KnowledgeRepository>>,
    pub graph: Option<Arc<dyn KnowledgeGraphRepository>>,
    pub beliefs: Option<Arc<dyn BeliefRepository>>,
    pub hierarchy: Option<Arc<dyn HierarchyRepository>>,
    pub retrieval: Option<Arc<dyn RetrievalIndex>>,
    pub vectors: Option<Arc<dyn VectorIndex>>,
    pub capabilities: CapabilityReport,
}

impl EngramProvider {
    pub fn bootstrap(config: EngramConfig) -> Result<Self, CoreError>;
}
```

**Key design decisions:**
- Handles are `Option<T>` — `None` for unsupported families (fail-closed)
- Single bootstrap call constructs all supported repositories
- `CapabilityReport` available immediately after bootstrap
- Path confinement and validation happen during bootstrap

### Capability reporting contract

```rust
pub struct CapabilityReport {
    pub memory: CapabilityState,
    pub knowledge: CapabilityState,
    pub graph: CapabilityState,
    pub ontology: CapabilityState,
    pub taxonomy: CapabilityState,
    pub beliefs: CapabilityState,
    pub hierarchy: CapabilityState,
    pub retrieval: CapabilityState,
    pub vectors: CapabilityState,
    pub migration: CapabilityState,
}

pub enum CapabilityState {
    Supported,
    Unsupported { reason: CapabilityReason },
    Degraded { reason: CapabilityReason },
    RequiresMigration { reason: CapabilityReason },
    RequiresReindex { reason: CapabilityReason },
    Misconfigured { reason: CapabilityReason },
}
```

**Key design decisions:**
- All 10 capability families reported (including ontology/taxonomy)
- Stable `CapabilityReason` string codes
- `Unsupported` vs `Degraded` vs `RequiresMigration` distinction
- Applications can check capabilities before calling operations

### Embedding provider contract

```rust
pub trait EmbeddingProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn dimensions(&self) -> u32;
    fn embedding_space(&self) -> EmbeddingSpace;
    
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
    async fn embed_passage(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
    async fn embed_batch(&self, texts: &[&str], mode: EmbeddingMode) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

pub struct EmbeddingSpace {
    pub provider: String,
    pub model: String,
    pub dimensions: u32,
    pub prompt_profile: String,
    pub normalization: Option<String>,
}
```

**Key design decisions:**
- Provider/model identity included in embedding space (not just dimensions)
- Prompt profile and normalization affect compatibility
- Trait-based design allows future provider additions
- FastEmbed and Ollama implementations included initially

### Error extension contract

**Extend existing `CoreError`** rather than create parallel error type:

```rust
// Extend existing CoreError enum with new variants
pub enum CoreError {
    // Existing variants...
    
    // New integration-specific variants
    #[error("Capability {0} is not supported: {1}")]
    CapabilityUnsupported { capability: String, reason: String },
    
    #[error("Embedding space mismatch: expected {expected}, got {actual}")]
    EmbeddingSpaceMismatch { expected: String, actual: String },
    
    #[error("Migration manifest is stale (fingerprint {expected} != {actual})")]
    MigrationManifestStale { expected: String, actual: String },
    
    // ... other new variants with redacted messages
}

// Add redaction layer for public error exposure
impl CoreError {
    pub fn to_redacted(&self) -> RedactedError {
        // Remove SQL internals, absolute paths, raw embeddings
        // Preserve safe human-readable messages
    }
    
    pub fn with_diagnostic(&self) -> DiagnosticError {
        // Full detail for local development only
    }
}
```

**Key design decisions:**
- Single error surface across all repository traits
- Redaction layer removes private internals from public errors
- Local diagnostic mode preserves full detail for development
- Error codes remain stable once released

### Migration/import contract

```rust
pub trait MigrationService: Send + Sync {
    async fn dry_run_import(&self, request: ImportRequest) -> Result<ValidationReport, CoreError>;
    async fn apply_import(&self, request: ImportRequest, manifest_fingerprint: String) -> Result<MigrationResult, CoreError>;
}

pub struct ValidationReport {
    pub row_counts_by_family: BTreeMap<String, u64>,
    pub unsupported_mappings: Vec<UnsupportedMapping>,
    pub scope_translation: ScopeTranslationReport,
    pub embedding_space_validation: EmbeddingSpaceValidation,
    pub manifest_fingerprint: String,
}
```

**Key design decisions:**
- Dry-run produces deterministic validation report
- SHA-256 fingerprint of row counts + content checksums
- Apply mode validates fingerprint before writing
- Stale manifest rejection prevents accidental overwrites

## Impact analysis

### Benefits
- External applications can embed Engram without understanding adapter internals
- Capability discovery prevents runtime failures for unsupported operations
- Provider-neutral embeddings support multiple embedding backends
- Embedding-space validation prevents silent incompatibility
- Migration API enables safe data import without private schema knowledge
- Conformance harness mechanically verifies capability claims
- Backward compatibility maintained for existing N-API bindings

### Costs
- Two new crates to maintain (`core/integration`, `adapters/integration`)
- Additional trait (`VectorIndex`) alongside existing `RetrievalIndex`
- Extended domain types increase surface area
- Migration and conformance infrastructure adds complexity

### Risks
- **Capability state explosion** — Too many capability states could make reporting complex. Mitigated by keeping families coarse-grained (10 families).
- **Embedding-space strictness** — May break workflows relying on dimension-only compatibility. Mitigated by clear error messages and RequiresReindex capability state.
- **Performance overhead** — Capability checking on every operation could impact performance. Mitigated by caching capability state in handles.
- **N-API binding compatibility** — Internal changes could break TypeScript bindings. Mitigated by maintaining public API compatibility.

### Migration path
- Existing N-API bindings continue to work (use integration facade internally)
- Current repository traits unchanged (only extended)
- New capabilities added incrementally by task
- External applications adopt new facade at their own pace

## Success criteria

**Functional:**
- External Rust application can open provider facade from single configuration
- Capability report accurately reflects supported/unsupported features
- Embedding providers are interchangeable behind common trait
- Vector indexes reject embedding-space mismatches
- Migration dry-run produces deterministic validation report
- Apply mode rejects stale manifests
- Conformance fixtures pass for all supported capabilities

**Non-functional:**
- Provider facade typechecked with stable public API
- Error codes and capability reasons remain stable across releases
- Capability discovery completes within 100ms (in-memory) or 500ms (file-backed)
- Rustdoc examples compile and demonstrate usage

## Implementation phases

1. **Foundation** (T1-T3): Domain types, integration crate structure, capability reporting
2. **Embedding abstraction** (T4-T5): EmbeddingProvider trait with FastEmbed and Ollama implementations  
3. **Vector integration** (T6-T7): VectorIndex trait and sqlite-vec embedding-space validation
4. **Provider facade** (T8): EngramProvider with repository handles and bootstrap logic
5. **Error handling** (T9): Extended CoreError with redaction layer
6. **Migration API** (T10): MigrationService with dry-run/apply gating
7. **Retrieval tracing** (T11): Extended retrieval trace with stable output
8. **Conformance** (T12): Harness with fixture groups per capability
9. **Documentation** (T13): External Rust application example
10. **Integration** (T14): N-API bindings updated to use facade internally

## Open questions

1. **Should migration fingerprint include full row content or IDs only?** Recommendation: IDs + content hashes for key fields to balance determinism with performance.
2. **Should capability detection be dynamic or static?** Recommendation: Static at bootstrap time for simplicity; dynamic detection adds complexity.
3. **Should Ollama retry logic be configurable or fixed policy?** Recommendation: Start with fixed policy (3 attempts, exponential backoff), make configurable in future if needed.
4. **Relationship between VectorIndex and RetrievalIndex?** Recommendation: VectorIndex is for raw vector operations, RetrievalIndex is for candidate retrieval composition. They are siblings, not hierarchical.

## References

- Integration contract document: `/home/videogamer/Documents/engram-rust-crate-integration-contract.md`
- ADR-0009: Retrieval composition seam
- ADR-0010: Decompose god modules  
- ADR-0014: Memory record embedding surface
- Spec: `docs/specs/rust-crate-integration/spec.md`
- Plan: `docs/specs/rust-crate-integration/plan.md`
