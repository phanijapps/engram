//! SQLite-backed hierarchy repository and path navigation.
//!
//! Storage persists nodes and relations as contract JSON with scope indexing.
//! `path_for` loads the in-scope graph and runs the same parent-chain traversal
//! as the in-memory adapter, so the durable backend is behaviorally identical.

use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use engram_domain::*;
use engram_hierarchy::HierarchyRepository;
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
        let all_nodes = load_nodes(&connection)?;
        let visible_nodes: Vec<&HierarchyNode> = all_nodes
            .iter()
            .filter(|node| scope_allows(&node.scope, scope))
            .collect();
        let mut chains = Vec::new();
        for seed_id in seed_ids {
            if let Some(seed) = find_seed_node(&visible_nodes, seed_id, max_layer) {
                chains.push(parent_chain(&visible_nodes, seed, max_layer));
            }
        }

        let lca_id = common_ancestor(&chains);
        let mut included_ids = BTreeSet::new();
        let mut nodes = Vec::new();
        for chain in chains {
            for node in chain {
                if included_ids.insert(node.id.to_string()) {
                    nodes.push(node.clone());
                }
            }
        }
        nodes.sort_by(|left, right| {
            left.layer
                .cmp(&right.layer)
                .then_with(|| left.id.cmp(&right.id))
        });

        let relations = load_relations(&connection)?
            .into_iter()
            .filter(|relation| {
                scope_allows(&relation.scope, scope)
                    && included_ids.contains(relation.source_id.as_str())
                    && included_ids.contains(relation.target_id.as_str())
            })
            .collect();

        Ok(HierarchyPath {
            seed_ids: seed_ids.to_vec(),
            lca_id,
            nodes,
            relations,
            max_layer,
        })
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

fn find_seed_node<'a>(
    nodes: &'a [&'a HierarchyNode],
    seed_id: &str,
    max_layer: Option<u32>,
) -> Option<&'a HierarchyNode> {
    nodes
        .iter()
        .copied()
        .filter(|node| max_layer.is_none_or(|limit| node.layer <= limit))
        .find(|node| {
            node.id.as_str() == seed_id || node.source_target_id.as_deref() == Some(seed_id)
        })
}

fn parent_chain<'a>(
    nodes: &'a [&'a HierarchyNode],
    seed: &'a HierarchyNode,
    max_layer: Option<u32>,
) -> Vec<&'a HierarchyNode> {
    let mut chain = Vec::new();
    let mut current = Some(seed);
    while let Some(node) = current {
        if max_layer.is_none_or(|limit| node.layer <= limit) {
            chain.push(node);
        }
        current = node.parent_id.as_ref().and_then(|parent_id| {
            nodes
                .iter()
                .copied()
                .find(|candidate| candidate.id == *parent_id)
        });
    }
    chain
}

fn common_ancestor(chains: &[Vec<&HierarchyNode>]) -> Option<HierarchyNodeId> {
    let first = chains.first()?;
    first.iter().find_map(|candidate| {
        chains
            .iter()
            .all(|chain| chain.iter().any(|node| node.id == candidate.id))
            .then(|| candidate.id.clone())
    })
}
