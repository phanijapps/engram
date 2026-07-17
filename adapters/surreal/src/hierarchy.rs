//! Surreal hierarchy cell — `HierarchyRepository` over embedded SurrealKV.
//!
//! Mirrors `engram-store-sqlite::hierarchy`: persists nodes + relations (DTO
//! under a `data` field, scope-indexed) and delegates path navigation to the
//! shared `engram_hierarchy::navigation::navigate` (same traversal the SQLite
//! adapter uses), so behavior is identical across backends.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{HierarchyNode, HierarchyPath, HierarchyRelation, Scope};
use engram_hierarchy::{HierarchyRepository, navigation};
use engram_runtime::CoreResult;

use crate::SurrealConnection;
use crate::util::{DataWrapper, scope_allows, surreal_err};

const NODE_TABLE: &str = "hierarchy_node";
const RELATION_TABLE: &str = "hierarchy_relation";

/// `HierarchyRepository` backed by embedded SurrealKV.
pub struct SurrealHierarchyStore {
    conn: Arc<SurrealConnection>,
}

impl SurrealHierarchyStore {
    /// Creates a hierarchy store over a shared Surreal connection.
    pub fn new(conn: Arc<SurrealConnection>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl HierarchyRepository for SurrealHierarchyStore {
    async fn put_node(&self, node: HierarchyNode) -> CoreResult<HierarchyNode> {
        let db = self.conn.db().await?;
        let key = node.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{NODE_TABLE}', $key) SET data = $node"
        ))
        .bind(("key", key))
        .bind(("node", node.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(node)
    }

    async fn put_relation(&self, relation: HierarchyRelation) -> CoreResult<HierarchyRelation> {
        let db = self.conn.db().await?;
        let key = relation.id.to_string();
        db.query(&format!(
            "UPSERT type::thing('{RELATION_TABLE}', $key) SET data = $relation"
        ))
        .bind(("key", key))
        .bind(("relation", relation.clone()))
        .await
        .map_err(surreal_err)?;
        Ok(relation)
    }

    async fn path_for(
        &self,
        seed_ids: &[String],
        scope: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath> {
        let db = self.conn.db().await?;
        let mut node_res = db
            .query(&format!("SELECT data FROM {NODE_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let node_rows: Vec<DataWrapper<HierarchyNode>> = node_res.take(0).map_err(surreal_err)?;
        let mut rel_res = db
            .query(&format!("SELECT data FROM {RELATION_TABLE}"))
            .await
            .map_err(surreal_err)?;
        let rel_rows: Vec<DataWrapper<HierarchyRelation>> = rel_res.take(0).map_err(surreal_err)?;

        let nodes: Vec<_> = node_rows
            .into_iter()
            .map(|w| w.data)
            .filter(|n| scope_allows(&n.scope, scope))
            .collect();
        let relations: Vec<_> = rel_rows
            .into_iter()
            .map(|w| w.data)
            .filter(|r| scope_allows(&r.scope, scope))
            .collect();
        Ok(navigation::navigate(
            &nodes, &relations, seed_ids, max_layer,
        ))
    }
}
