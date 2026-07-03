//! Belief and contradiction behavior ports for the engram engine.
//!
//! This crate owns storage-neutral belief behavior that adapters must preserve:
//! valid-time filtering, lifecycle transitions, contradiction idempotency keys,
//! and embedding-scoring helpers. Domain records live in `engram-domain`; raw
//! storage, scheduler ownership, HTTP APIs, and product-specific compatibility
//! layers stay outside this crate.

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;

pub mod contradiction;
pub mod embedding;
pub mod lifecycle;
pub mod query;
pub mod reconcile;
pub mod temporal;

pub use contradiction::{CanonicalContradictionPair, canonical_pair_key, canonicalize_pair};
pub use embedding::{
    BeliefEmbeddingCandidate, BeliefEmbeddingScore, cosine_similarity, decode_f32_le,
    rank_embedding_candidates,
};
pub use lifecycle::{
    belief_references_source, clear_stale_state, is_live_belief, mark_stale, retract_belief,
    supersede_belief,
};
pub use query::{BeliefQuery, BeliefQueryOrder, BeliefReferenceQuery};
pub use reconcile::{AuthorityPolicy, Reconciled, TieRule, reconcile};
pub use temporal::{interval_contains, live_at};

/// Persistence port for derived beliefs and contradiction records.
///
/// Beliefs should be recomputable from evidence or explicitly marked as manual.
/// Contradictions are review records; writing one must not silently mutate the
/// targets in conflict.
#[async_trait]
pub trait BeliefRepository: Send + Sync {
    /// Stores a derived or manually asserted belief.
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief>;

    /// Upserts a belief by its compatibility key.
    ///
    /// Implementations must treat `(scope, subject.key, valid_from)` as the
    /// idempotency key so compatibility adapters can preserve legacy "one stance
    /// for this subject at this valid time" behavior without parsing opaque IDs.
    async fn upsert_belief(&self, belief: Belief) -> CoreResult<Belief>;

    /// Returns the best belief matching the query, including valid-time filters.
    ///
    /// Repositories that cannot answer `recorded_at` history must return
    /// `CoreError::InvalidRequest` instead of pretending current rows are a
    /// bitemporal audit log.
    async fn get_belief(&self, query: BeliefQuery) -> CoreResult<Option<Belief>>;

    /// Looks up one belief by opaque identifier inside the supplied scope.
    async fn get_belief_by_id(&self, id: &BeliefId, scope: &Scope) -> CoreResult<Option<Belief>>;

    /// Marks a belief stale without changing its evidence or content.
    async fn mark_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief>;

    /// Clears a belief's stale state without changing its evidence or content.
    async fn clear_stale(&self, id: &BeliefId, scope: &Scope, at: Timestamp) -> CoreResult<Belief>;

    /// Supersedes a belief by closing its valid interval and linking the
    /// replacement belief.
    async fn supersede_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        replacement_id: BeliefId,
        at: Timestamp,
    ) -> CoreResult<Belief>;

    /// Retracts a belief by closing its valid interval without selecting a
    /// replacement.
    async fn retract_belief(
        &self,
        id: &BeliefId,
        scope: &Scope,
        at: Timestamp,
    ) -> CoreResult<Belief>;

    /// Lists currently stale beliefs visible to the supplied scope.
    async fn list_stale(&self, scope: &Scope) -> CoreResult<Vec<Belief>>;

    /// Lists live beliefs that cite the requested evidence source.
    async fn beliefs_referencing_source(
        &self,
        query: BeliefReferenceQuery,
    ) -> CoreResult<Vec<Belief>>;

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
