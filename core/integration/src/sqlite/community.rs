//! SQLite storage counts and compatibility delegates to canonical communities.
use crate::{CommunityQuery, community_composition::SharedCommunityQuery};
use async_trait::async_trait;
use engram_domain::{CommunityOverview, Scope, ScopeCounts};
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;
use std::{collections::HashMap, sync::Arc};
pub async fn overview(
    store: &SqlKnowledgeStore,
    scope: &Scope,
    limit: usize,
) -> CoreResult<CommunityOverview> {
    crate::community_composition::overview(store, scope, limit).await
}
pub async fn overview_eligible(
    store: &SqlKnowledgeStore,
    scope: &Scope,
    limit: usize,
    view: &engram_retrieval::BoundCodeView,
) -> CoreResult<CommunityOverview> {
    crate::community_composition::overview_eligible(store, scope, limit, view).await
}

#[async_trait]
impl CommunityQuery for SqlKnowledgeStore {
    async fn overview(&self, scope: &Scope, limit: usize) -> CoreResult<CommunityOverview> {
        overview(self, scope, limit).await
    }
    async fn member_index(&self, scope: &Scope) -> CoreResult<HashMap<u32, Vec<String>>> {
        SharedCommunityQuery::new(Arc::new(self.clone()))
            .member_index(scope)
            .await
    }
    async fn community_of(&self, scope: &Scope, entity_id: &str) -> CoreResult<Option<u32>> {
        SharedCommunityQuery::new(Arc::new(self.clone()))
            .community_of(scope, entity_id)
            .await
    }
    async fn scope_counts(&self, scope: &Scope) -> CoreResult<ScopeCounts> {
        SqlKnowledgeStore::scope_counts(self, scope).await
    }
}
