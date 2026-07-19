//! Injected read access to the in-scope knowledge graph for associative
//! retrieval.
//!
//! Mirrors the two-method shape of
//! `adapters::knowledge::sqlite::retrieval::GraphCandidateSource`:
//! - [`GraphRelationshipSource::entities`] supplies the entities lexical seed
//!   resolution matches against, and whose `provenance` / content the emitted
//!   `RetrievalResult`s clone.
//! - [`GraphRelationshipSource::relationships`] supplies the directed edge set
//!   the Personalized PageRank walk traverses.
//!
//! The trait keeps [`crate::AssociativeGraphIndex`] (added in a later task)
//! testable without a concrete store — a stub source stands in — and lets a
//! future backend plug in by implementing the same two reads. Implementations
//! MUST scope-filter (`scope_allows`) before returning, so the PPR walk over the
//! returned edge set cannot cross scope boundaries. Scope isolation is enforced
//! at this read boundary, never inside the walk.

use async_trait::async_trait;
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, Scope};
use engram_runtime::CoreResult;

/// Read access to the in-scope knowledge graph for associative retrieval.
///
/// Implementations return only the entities and relationships the request
/// `scope` is allowed to see. Because the Personalized PageRank walk runs only
/// over edges this trait returns, out-of-scope nodes are unreachable by
/// construction — there is no second scope gate inside the walk.
#[async_trait]
pub trait GraphRelationshipSource: Send + Sync {
    /// All knowledge-graph entities visible to `scope`.
    async fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>>;

    /// All knowledge-graph relationships visible to `scope`, as the directed
    /// edge set the walk traverses.
    async fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>>;
}
