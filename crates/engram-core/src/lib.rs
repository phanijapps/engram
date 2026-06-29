//! Behavior ports for the engram engine.
//!
//! `engram-core` defines the contracts that service implementations and
//! adapters must satisfy. It deliberately owns behavior boundaries, not concrete
//! infrastructure: SQL stores, vector indexes, embedding providers, schedulers,
//! gateways, and TypeScript bindings should implement these traits elsewhere.

mod consolidation;

use async_trait::async_trait;
use engram_domain::*;
use thiserror::Error;

pub use consolidation::DryRunConsolidationService;

/// Stable error surface shared by core services and adapters.
///
/// Adapter implementations should translate infrastructure-specific failures
/// into these categories at the boundary. Detailed diagnostics can be logged by
/// the adapter, but callers should be able to make portable decisions from this
/// enum without knowing which store, index, or provider was used.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("record not found: {target_type}:{target_id}")]
    NotFound {
        target_type: &'static str,
        target_id: String,
    },
    #[error("policy denied: {reason}")]
    PolicyDenied { reason: String },
    #[error("invalid request: {reason}")]
    InvalidRequest { reason: String },
    #[error("adapter failed: {adapter}: {message}")]
    Adapter { adapter: String, message: String },
    #[error("conflict: {reason}")]
    Conflict { reason: String },
}

/// Result type used by core ports.
pub type CoreResult<T> = Result<T, CoreError>;

/// Supplies timestamps for services that must be deterministic in tests.
///
/// Production implementations normally delegate to system time. Tests and
/// replay tools should provide a fixed or scripted clock so event ordering,
/// retention behavior, and consolidation windows can be verified precisely.
pub trait Clock: Send + Sync {
    /// Returns the current UTC timestamp for newly created domain records.
    fn now(&self) -> Timestamp;
}

/// Generates opaque identifiers for domain entities.
///
/// Implementations may use UUIDs, ULIDs, content-addressed IDs, or another
/// strategy, but callers must treat the result as opaque. Tenant, timestamp,
/// authorization, and storage-location semantics belong in typed fields.
pub trait IdGenerator: Send + Sync {
    /// Creates a new opaque identifier for the named entity type.
    fn new_id(&self, entity_type: &'static str) -> Id;
}

/// Decides whether a record scope is eligible for a request scope.
///
/// This is a structural scope check, not a full authorization decision. Policy
/// and caller permissions are handled by `PolicyAuthorizer` after scope
/// eligibility is established.
pub trait ScopeMatcher: Send + Sync {
    /// Returns true when `record_scope` may be considered for `request_scope`.
    fn is_visible_scope(&self, request_scope: &Scope, record_scope: &Scope) -> bool;
}

/// Enforces policy before durable mutations or retrieval composition.
///
/// Adapters may do additional physical isolation, but they must not bypass this
/// logical authorization layer. Denials should return `CoreError::PolicyDenied`
/// with a stable reason suitable for evaluation and audit records.
pub trait PolicyAuthorizer: Send + Sync {
    /// Checks whether `requester` may create or update a record in `scope`.
    fn can_write(&self, requester: &Requester, scope: &Scope, policy: &Policy) -> CoreResult<()>;

    /// Checks whether `requester` may retrieve a record governed by `policy`.
    fn can_retrieve(&self, requester: &Requester, scope: &Scope, policy: &Policy)
    -> CoreResult<()>;

    /// Checks whether `requester` may apply the requested deletion behavior.
    fn can_forget(&self, requester: &Requester, scope: &Scope, policy: &Policy) -> CoreResult<()>;
}

/// Persistence port for memory records and append-only lifecycle events.
///
/// Implementations must preserve the portable `MemoryRecord` shape losslessly.
/// Indexes, tables, or event streams may be adapter-specific, but status
/// changes and write events must remain auditable through the domain model.
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Stores a memory record and returns the persisted representation.
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord>;

    /// Looks up a memory by ID inside the caller-provided scope boundary.
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>>;

    /// Appends a lifecycle event without rewriting prior events.
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent>;

    /// Updates lifecycle status while preserving policy and provenance history.
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord>;
}

/// Read port for append-only memory lifecycle events.
///
/// Event reads are separate from memory record writes because audit, evaluation,
/// consolidation, and debugging need to inspect history without granting direct
/// mutation access. Implementations must preserve event ordering as recorded by
/// the adapter and must apply the supplied scope boundary before returning
/// events.
#[async_trait]
pub trait MemoryEventRepository: Send + Sync {
    /// Looks up a lifecycle event by ID inside the caller-provided scope.
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>>;

    /// Lists lifecycle events for one memory inside the caller-provided scope.
    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>>;

    /// Lists lifecycle events visible to the supplied scope.
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>>;
}

/// Persistence port for source-grounded knowledge records.
///
/// Knowledge sources, documents, and chunks are separate from memory records so
/// code repositories and unstructured documents can be ingested without turning
/// source facts into agent memories prematurely.
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Stores or updates a registered knowledge source.
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource>;

    /// Stores a versioned document extracted from a source.
    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument>;

    /// Stores the smallest retrievable source-grounded unit.
    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk>;

    /// Looks up a chunk by ID inside the caller-provided scope boundary.
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>>;
}

/// Persistence port for derived beliefs and contradiction records.
///
/// Beliefs should be recomputable from evidence or explicitly marked as manual.
/// Contradictions are review records; writing one must not silently mutate the
/// targets in conflict.
#[async_trait]
pub trait BeliefRepository: Send + Sync {
    /// Stores a derived or manually asserted belief.
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief>;

    /// Stores a reviewable contradiction between memories, beliefs, or knowledge.
    async fn put_contradiction(&self, contradiction: Contradiction) -> CoreResult<Contradiction>;
}

/// Persistence and navigation port for hierarchy structures.
///
/// Hierarchy adapters may materialize trees, DAG-like relation sets, or cached
/// paths internally. The public results must still expose explainable nodes,
/// relations, and provenance for navigation and context compression.
#[async_trait]
pub trait HierarchyRepository: Send + Sync {
    /// Stores a hierarchy node from a build or manual curation step.
    async fn put_node(&self, node: HierarchyNode) -> CoreResult<HierarchyNode>;

    /// Stores an explainable relation between hierarchy nodes.
    async fn put_relation(&self, relation: HierarchyRelation) -> CoreResult<HierarchyRelation>;

    /// Finds a navigation path for seed objects without crossing scope boundaries.
    async fn path_for(
        &self,
        seed_ids: &[String],
        scope: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath>;
}

/// Candidate retrieval port for one source or strategy.
///
/// A `RetrievalIndex` might be lexical, vector, graph, temporal, hierarchical,
/// or hybrid. It returns candidates with provenance and policy attached; final
/// fusion and context-budget decisions are handled later in the pipeline.
#[async_trait]
pub trait RetrievalIndex: Send + Sync {
    /// Retrieves candidates for the request without composing the final context.
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>>;
}

/// Merges and reranks candidates from multiple retrieval sources.
///
/// Implementations should preserve or populate `FusionTrace` so callers can see
/// which source contributed a result, how scores changed, and which duplicates
/// were collapsed.
pub trait RetrievalFusion: Send + Sync {
    /// Returns a ranked candidate list after fusion and optional reranking.
    fn fuse(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>>;
}

/// Builds the final context payload returned to callers.
///
/// Composition is where budgets, omitted-result explanations, and non-fatal
/// source failures become visible. Implementations must not hide policy denials
/// or degraded retrieval sources when the contract allows reporting them.
pub trait ContextComposer: Send + Sync {
    /// Applies final budget and explanation rules to produce caller context.
    fn compose(
        &self,
        request: &RetrievalRequest,
        results: Vec<RetrievalResult>,
        failures: Vec<RetrievalSourceFailure>,
    ) -> CoreResult<ContextPayload>;
}

/// Public service contract for memory workflows.
///
/// A concrete engine should enforce scope and policy across all three methods.
/// Repositories and retrieval indexes are lower-level ports; this service is
/// the stable orchestration boundary for application bindings.
#[async_trait]
pub trait MemoryService: Send + Sync {
    /// Writes a memory and records the corresponding lifecycle event.
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse>;

    /// Retrieves policy-checked context for the request.
    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload>;

    /// Applies delete, redact, tombstone, or archive behavior to a target.
    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult>;
}

/// Reads external sources without owning persistence.
///
/// Implementations translate filesystems, Git repositories, URLs, uploads, or
/// APIs into `SourceDocument` records. They should report adapter failures
/// explicitly instead of returning partial success as complete ingestion.
#[async_trait]
pub trait SourceReader: Send + Sync {
    /// Lists or discovers documents available from a registered source.
    async fn read_source(&self, source: &KnowledgeSource) -> CoreResult<Vec<SourceDocument>>;

    /// Reads extracted textual content for one source document.
    async fn read_document(&self, document: &SourceDocument) -> CoreResult<String>;
}

/// Splits source document content into source-grounded chunks.
///
/// Chunkers must preserve enough location and provenance information for later
/// retrieval explanations. Code-aware chunkers should emit symbol/file chunk
/// kinds instead of flattening everything into generic text.
pub trait Chunker: Send + Sync {
    /// Creates retrievable chunks from a document's extracted content.
    fn chunk_document(
        &self,
        source: &KnowledgeSource,
        document: &SourceDocument,
        content: &str,
    ) -> CoreResult<Vec<KnowledgeChunk>>;
}

/// Coordinates source reading, chunking, and knowledge persistence.
///
/// Ingestion services should be idempotent where source hashes allow it. Dry
/// runs should compute the plan without writing durable source, document, or
/// chunk records.
#[async_trait]
pub trait IngestionService: Send + Sync {
    /// Ingests a registered source and returns chunks written or planned.
    async fn ingest(&self, request: IngestRequest) -> CoreResult<Vec<KnowledgeChunk>>;
}

/// Runs auditable consolidation cycles over memory and knowledge state.
///
/// Consolidation may synthesize memories, beliefs, contradictions, hierarchy
/// nodes, or taxonomy changes. Any durable mutation should be represented in a
/// `ConsolidationRun` with task-level outcomes and recoverable errors.
#[async_trait]
pub trait ConsolidationService: Send + Sync {
    /// Executes one consolidation cycle for the requested scope and strategy.
    async fn consolidate(&self, request: ConsolidationRequest) -> CoreResult<ConsolidationRun>;
}

/// Derives belief records from current evidence.
///
/// Synthesizers should keep evidence links intact and mark beliefs stale or
/// superseded rather than destructively rewriting unsupported conclusions.
#[async_trait]
pub trait BeliefSynthesizer: Send + Sync {
    /// Produces belief candidates for a consolidation request.
    async fn synthesize_beliefs(&self, request: &ConsolidationRequest) -> CoreResult<Vec<Belief>>;
}

/// Detects reviewable contradictions across beliefs and their evidence.
///
/// Detection is advisory. Implementations should create contradiction records
/// with severity and reasoning, leaving resolution to a later explicit step.
#[async_trait]
pub trait ContradictionDetector: Send + Sync {
    /// Returns contradictions found in the supplied belief set.
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>>;
}

/// Builds hierarchy nodes for navigation and context compression.
///
/// Builders may use clustering, taxonomy, graph structure, or model-assisted
/// summaries internally. Outputs must preserve algorithm provenance and avoid
/// creating multiple parent pointers inside a single tree version.
#[async_trait]
pub trait HierarchyBuilder: Send + Sync {
    /// Builds hierarchy nodes for a scope using a recorded build configuration.
    async fn build_hierarchy(
        &self,
        config: &HierarchyBuildConfig,
        scope: &Scope,
    ) -> CoreResult<Vec<HierarchyNode>>;
}

/// Executes evaluation fixtures against a memory implementation.
///
/// Runners should report positive recall failures, forbidden recall leaks,
/// missing explanations, and score/ranking regressions separately so quality
/// failures are actionable.
#[async_trait]
pub trait EvaluationRunner: Send + Sync {
    /// Runs a fixture and returns per-case pass/fail details.
    async fn run_fixture(&self, fixture: EvaluationFixture) -> CoreResult<EvaluationReport>;
}

/// Result of running one evaluation fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationReport {
    pub fixture_id: EvaluationId,
    pub cases: Vec<EvaluationCaseReport>,
}

/// Result of one case inside an evaluation fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationCaseReport {
    pub case_id: String,
    pub passed: bool,
    pub failures: Vec<String>,
}
