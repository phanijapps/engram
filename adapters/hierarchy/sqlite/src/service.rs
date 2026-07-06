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
use engram_runtime::{CoreError, CoreResult, SqliteOpenOptions, SqlitePath};
use rusqlite::{Connection, OptionalExtension, params};

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

    /// Opens a SQLite hierarchy store with explicit configuration options.
    ///
    /// This constructor allows hosts like AgentZero's adapter to control WAL mode,
    /// busy timeout, foreign keys, migrations, and directory creation explicitly.
    ///
    /// # Arguments
    ///
    /// * `options` - SQLite configuration options including path, journal mode, and
    ///   pragma settings
    ///
    /// # Returns
    ///
    /// Returns a configured `SqlHierarchyStore` instance with schema initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use engram_runtime::{SqliteOpenOptions, SqliteJournalMode, SqlitePath};
    /// use engram_store_hierarchy_sqlite::SqlHierarchyStore;
    ///
    /// let options = SqliteOpenOptions {
    ///     path: SqlitePath::File("/path/to/hierarchy.db".into()),
    ///     create_parent_dirs: true,
    ///     journal_mode: SqliteJournalMode::Wal,
    ///     busy_timeout_ms: Some(5000),
    ///     foreign_keys: true,
    ///     run_migrations: true,
    /// };
    ///
    /// let store = SqlHierarchyStore::open_with_options(options)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                            adapter: "engram-store-hierarchy-sqlite".to_owned(),
                            message: format!("failed to create parent directory: {}", e),
                        })?;
                    }
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(|e| CoreError::Adapter {
            adapter: "engram-store-hierarchy-sqlite".to_owned(),
            message: e.to_string(),
        })?;

        // Apply pragmas from options
        Self::apply_pragmas(&connection, &options)?;

        // Initialize schema
        initialize_schema(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn from_connection(connection: Connection) -> CoreResult<Self> {
        // Apply default pragmas for backward compatibility
        let options = SqliteOpenOptions::in_memory();
        Self::apply_pragmas(&connection, &options)?;

        initialize_schema(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Applies SQLite PRAGMAs based on configuration options.
    ///
    /// This centralizes pragma application so both `open_with_options` and legacy
    /// constructors can use the same logic.
    fn apply_pragmas(connection: &Connection, options: &SqliteOpenOptions) -> CoreResult<()> {
        // Journal mode (returns result set, so we use query_row and ignore the result)
        let journal_pragma = format!(
            "PRAGMA journal_mode = {}",
            options.journal_mode.as_pragma_value()
        );
        connection
            .query_row(&journal_pragma, [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-hierarchy-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Synchronous mode (NORMAL for WAL mode)
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-hierarchy-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Busy timeout
        if let Some(timeout_ms) = options.busy_timeout_ms {
            let timeout_pragma = format!("PRAGMA busy_timeout = {}", timeout_ms);
            connection
                .query_row(&timeout_pragma, [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-hierarchy-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Foreign keys
        if options.foreign_keys {
            connection
                .query_row("PRAGMA foreign_keys = ON", [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-hierarchy-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Cache size (64MB default for better performance)
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-hierarchy-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        Ok(())
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
