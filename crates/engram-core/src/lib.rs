//! Orchestration ports for the engram engine.
//!
//! `engram-core` is the compatibility facade and orchestration boundary above
//! dedicated behavior crates. Memory ports live in `engram-memory`; knowledge,
//! graph, ontology, source, and ingestion ports live in `engram-knowledge`.
//! Concrete infrastructure still belongs behind adapters.

mod consolidation;

use async_trait::async_trait;
use engram_domain::*;

pub use consolidation::{
    ConsolidationMutationExecutor, ConsolidationMutationOutcome, DryRunConsolidationService,
    GatedConsolidationService,
};
pub use engram_knowledge::*;
pub use engram_memory::*;
pub use engram_runtime::{CoreError, CoreResult};

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

    /// Looks up a contradiction review record inside the supplied scope.
    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>>;

    /// Applies an explicit reviewer resolution to a contradiction record.
    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction>;
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
