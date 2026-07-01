//! SQLite-backed hierarchy repository and path navigation.
//!
//! Storage persists nodes and relations as contract JSON with scope indexing.
//! `path_for` loads the in-scope graph and runs the same parent-chain traversal
//! as the in-memory adapter, so the durable backend is behaviorally identical.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use engram_domain::*;
use engram_hierarchy::{HierarchyRepository, navigation};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, params};

use crate::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};

/// SQLite-backed hierarchy repository.
///
/// Preserves hierarchy nodes and relations as contract JSON while indexing
/// identifiers, scope columns, and (for nodes) `layer` for repository reads.
#[derive(Clone)]
pub struct SqlHierarchyStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlHierarchyStore {
    /// Opens an in-memory hierarchy store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(sql_error)?)
    }

    /// Opens a file-backed hierarchy store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::from_connection(Connection::open(path).map_err(sql_error)?)
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-hierarchy-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }
}

#[async_trait]
impl HierarchyRepository for SqlHierarchyStore {
    async fn put_node(&self, node: HierarchyNode) -> CoreResult<HierarchyNode> {
        let json = serde_json::to_string(&node).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO hierarchy_nodes
                    (id, tenant, subject, workspace, session, environment, layer, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    layer = excluded.layer,
                    record_json = excluded.record_json
                "#,
                params![
                    node.id.to_string(),
                    node.scope.tenant,
                    node.scope.subject,
                    node.scope.workspace,
                    node.scope.session,
                    node.scope.environment,
                    node.layer as i64,
                    json,
                ],
            )
            .map_err(sql_error)?;
        Ok(node)
    }

    async fn put_relation(&self, relation: HierarchyRelation) -> CoreResult<HierarchyRelation> {
        let json = serde_json::to_string(&relation).map_err(json_error)?;
        let connection = self.lock()?;
        connection
            .execute(
                r#"
                INSERT INTO hierarchy_relations
                    (id, tenant, subject, workspace, session, environment, record_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    tenant = excluded.tenant,
                    subject = excluded.subject,
                    workspace = excluded.workspace,
                    session = excluded.session,
                    environment = excluded.environment,
                    record_json = excluded.record_json
                "#,
                params![
                    relation.id,
                    relation.scope.tenant,
                    relation.scope.subject,
                    relation.scope.workspace,
                    relation.scope.session,
                    relation.scope.environment,
                    json,
                ],
            )
            .map_err(sql_error)?;
        Ok(relation)
    }

    async fn path_for(
        &self,
        seed_ids: &[String],
        scope: &Scope,
        max_layer: Option<u32>,
    ) -> CoreResult<HierarchyPath> {
        let connection = self.lock()?;
        let nodes = load_nodes(&connection)?
            .into_iter()
            .filter(|node| scope_allows(&node.scope, scope))
            .collect::<Vec<_>>();
        let relations = load_relations(&connection)?
            .into_iter()
            .filter(|relation| scope_allows(&relation.scope, scope))
            .collect::<Vec<_>>();
        Ok(navigation::navigate(
            &nodes, &relations, seed_ids, max_layer,
        ))
    }
}

fn load_nodes(connection: &Connection) -> CoreResult<Vec<HierarchyNode>> {
    let mut statement = connection
        .prepare("SELECT record_json FROM hierarchy_nodes ORDER BY id")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut nodes = Vec::new();
    for row in rows {
        let json = row.map_err(sql_error)?;
        nodes.push(serde_json::from_str::<HierarchyNode>(&json).map_err(json_error)?);
    }
    Ok(nodes)
}

fn load_relations(connection: &Connection) -> CoreResult<Vec<HierarchyRelation>> {
    let mut statement = connection
        .prepare("SELECT record_json FROM hierarchy_relations ORDER BY id")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut relations = Vec::new();
    for row in rows {
        let json = row.map_err(sql_error)?;
        relations.push(serde_json::from_str::<HierarchyRelation>(&json).map_err(json_error)?);
    }
    Ok(relations)
}

// Path navigation (seed resolution, parent-chain walk, LCA) lives in
// `engram_hierarchy::navigation` and is shared with the in-memory adapter.
