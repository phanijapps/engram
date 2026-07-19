//! Retrieval extension ports.
//!
//! These traits are the storage-neutral retrieval boundary. Candidate
//! producers implement source-specific policy and scope checks before returning
//! results; fusion and context composition then happen without knowing which
//! store, index, graph, or provider produced each candidate.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

/// Candidate retrieval port for one source or strategy.
///
/// A `RetrievalIndex` is defined by **what it returns** — candidates of some
/// `RetrievalTargetType` (Memory, Chunk, Entity, Relationship, …) with
/// provenance and policy attached — never by **how** it retrieves. Traversal,
/// vector similarity, lexical match, or any hybrid are the adapter's private
/// choice; the trait stays mechanism-agnostic so backends are interchangeable
/// (a graph index may be backed by Neo4j traversal or pgvector semantics).
/// Final fusion and context-budget decisions are handled later in the pipeline.
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

/// Reranks fused retrieval candidates by query-aware relevance (cross-encoder).
/// Applied between fusion and budget in `compose_context`. Implementations stay
/// behind the injected scorer; model lifecycle is never in core.
pub trait RetrievalReranker: Send + Sync {
    /// Reranks candidates by the query, best-first. Stamps `FusionTrace` with
    /// `rerank_strategy = CrossEncoder` + the new score. Returns the reranked
    /// list (may be shorter if candidates are filtered).
    fn rerank(
        &self,
        request: &RetrievalRequest,
        candidates: Vec<RetrievalResult>,
    ) -> CoreResult<Vec<RetrievalResult>>;
}
