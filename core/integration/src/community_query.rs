//! Engine-neutral community-query port.
//!
//! Computes the Louvain community structure + inter-community meta-edges + a
//! member index from the knowledge graph. Implementations cache the expensive
//! Louvain pass per store-version (the aggregate is ~2.6s on 227k edges; a
//! stateless re-run per call would be too slow for drill clicks).

use std::collections::HashMap;

use async_trait::async_trait;
use engram_domain::{CommunityOverview, Scope};
use engram_runtime::{CoreError, CoreResult};

/// Read port for the community aggregate (Louvain + meta-edges + member index).
/// Default methods return `CapabilityUnsupported` — the SQLite adapter overrides.
#[async_trait]
pub trait CommunityQuery: Send + Sync {
    /// The top-N communities + inter-community meta-edges + total count.
    async fn overview(&self, _scope: &Scope, _limit: usize) -> CoreResult<CommunityOverview> {
        Err(CoreError::CapabilityUnsupported {
            capability: "community_query".to_string(),
            reason: "this adapter does not implement community aggregation".to_string(),
        })
    }

    /// The full member index: community label → entity-id strings (capped per
    /// label). Consumers page + hydrate; the port computes the index once (cached).
    async fn member_index(&self, _scope: &Scope) -> CoreResult<HashMap<u32, Vec<String>>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "community_query".to_string(),
            reason: "this adapter does not implement community aggregation".to_string(),
        })
    }

    /// The community label for an entity id (None if not in a drillable community).
    async fn community_of(&self, _scope: &Scope, _entity_id: &str) -> CoreResult<Option<u32>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "community_query".to_string(),
            reason: "this adapter does not implement community aggregation".to_string(),
        })
    }
}
