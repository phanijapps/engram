//! Narrow read port for scoped active memories.
//!
//! The consolidation crate deliberately has no repository dependency, so the
//! synthesizer injects this focused trait (mirrors `GraphRelationshipSource` in
//! the associative-graph adapter). Production wiring adapts
//! `MemoryEventRepository::list_events_for_scope` → `MemoryRepository::
//! get_memory` → filter `Active` → texts; tests stub it directly.

use async_trait::async_trait;
use engram_domain::{Belief, Scope};
use engram_runtime::CoreResult;

/// Read access to a scope's active memory texts for reflection.
#[async_trait]
pub trait ActiveMemorySource: Send + Sync {
    /// Returns the text content of all `MemoryStatus::Active` memories visible
    /// to `scope`, in stable order.
    async fn active_memory_texts(&self, scope: &Scope) -> CoreResult<Vec<String>>;
}

/// Write access for persisting reflection-derived beliefs (mirrors
/// `BeliefRepository::put_belief`). Narrow so tests stub one method instead of
/// 13; production wiring adapts `BeliefRepository` to this trait.
#[async_trait]
pub trait BeliefSink: Send + Sync {
    /// Persists a derived belief, returning the stored record.
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief>;
}
