//! SQLite-backed [`KnowledgeQuery`] over [`SqlKnowledgeStore`]'s inherent list
//! methods.
//!
//! Engine-specific (names `SqlKnowledgeStore`, gated behind the `sqlite`
//! feature). The [`KnowledgeQuery`] trait itself (in the parent crate's
//! [`knowledge_query`](crate::knowledge_query) module) stays engine-neutral.

use async_trait::async_trait;
use engram_domain::Scope;
use engram_runtime::CoreResult;
use engram_store_sqlite::SqlKnowledgeStore;

use crate::knowledge_query::KnowledgeQuery;

#[async_trait]
impl KnowledgeQuery for SqlKnowledgeStore {
    async fn list_entities(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_domain::KnowledgeEntity>> {
        SqlKnowledgeStore::list_entities(self, scope).await
    }

    async fn list_relationships(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<engram_domain::KnowledgeRelationship>> {
        SqlKnowledgeStore::list_relationships(self, scope).await
    }
}
