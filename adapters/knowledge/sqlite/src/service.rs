//! SQLite-backed knowledge, graph, and taxonomy repository.
//!
//! Storage-only: this module persists contract payloads as JSON with scope and
//! lookup indexing. Knowledge orchestration (ingestion, extraction, retrieval
//! fusion) lives elsewhere. Scope visibility mirrors the in-memory knowledge
//! adapter — records that carry scope are filtered directly; chunks, documents,
//! concepts, and relations inherit visibility from their owning source or scheme.

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::Utc;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult, SqliteOpenOptions, SqlitePath};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    schema::{initialize_schema, json_error, sql_error},
    scope::scope_allows,
};

/// SQLite-backed knowledge, graph, and taxonomy repository.
///
/// Preserves knowledge sources/documents/chunks, entities/relationships/graphs,
/// and concept schemes/concepts/relations as contract JSON while indexing
/// identifiers and scope columns for repository reads.
#[derive(Clone)]
pub struct SqlKnowledgeStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqlKnowledgeStore {
    /// Opens an in-memory SQLite knowledge store and initializes its schema.
    pub fn open_in_memory() -> CoreResult<Self> {
        let connection = Connection::open_in_memory().map_err(sql_error)?;
        Self::from_connection(connection)
    }

    /// Opens a file-backed SQLite knowledge store and initializes its schema.
    pub fn open_file(path: impl AsRef<Path>) -> CoreResult<Self> {
        let connection = Connection::open(path).map_err(sql_error)?;
        Self::from_connection(connection)
    }

    /// Opens a SQLite knowledge store with explicit configuration options.
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
    /// Returns a configured `SqlKnowledgeStore` instance with schema initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use engram_runtime::{SqliteOpenOptions, SqliteJournalMode, SqlitePath};
    /// use engram_store_knowledge_sqlite::SqlKnowledgeStore;
    ///
    /// let options = SqliteOpenOptions {
    ///     path: SqlitePath::File("/path/to/knowledge.db".into()),
    ///     create_parent_dirs: true,
    ///     journal_mode: SqliteJournalMode::Wal,
    ///     busy_timeout_ms: Some(5000),
    ///     foreign_keys: true,
    ///     run_migrations: true,
    /// };
    ///
    /// let store = SqlKnowledgeStore::open_with_options(options)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_with_options(options: SqliteOpenOptions) -> CoreResult<Self> {
        let connection = match &options.path {
            SqlitePath::File(path) => {
                if options.create_parent_dirs {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| CoreError::Adapter {
                            adapter: "engram-store-knowledge-sqlite".to_owned(),
                            message: format!("failed to create parent directory: {}", e),
                        })?;
                    }
                }
                Connection::open(path)
            }
            SqlitePath::InMemory => Connection::open_in_memory(),
        }
        .map_err(|e| CoreError::Adapter {
            adapter: "engram-store-knowledge-sqlite".to_owned(),
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
                adapter: "engram-store-knowledge-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Synchronous mode (NORMAL for WAL mode)
        connection
            .query_row("PRAGMA synchronous = NORMAL", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-knowledge-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        // Busy timeout
        if let Some(timeout_ms) = options.busy_timeout_ms {
            let timeout_pragma = format!("PRAGMA busy_timeout = {}", timeout_ms);
            connection
                .query_row(&timeout_pragma, [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-knowledge-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Foreign keys
        if options.foreign_keys {
            connection
                .query_row("PRAGMA foreign_keys = ON", [], |_row| Ok(()))
                .optional()
                .map_err(|e| CoreError::Adapter {
                    adapter: "engram-store-knowledge-sqlite".to_owned(),
                    message: e.to_string(),
                })?;
        }

        // Cache size (64MB default for better performance)
        connection
            .query_row("PRAGMA cache_size = 64000", [], |_row| Ok(()))
            .optional()
            .map_err(|e| CoreError::Adapter {
                adapter: "engram-store-knowledge-sqlite".to_owned(),
                message: e.to_string(),
            })?;

        Ok(())
    }

    /// Public lock method used by repository trait implementations.
    ///
    /// This method is public so that separate modules (knowledge, graph,
    /// taxonomy, ontology) can access the SQLite connection. Each module
    /// implements a specific repository trait and needs database access.
    pub fn lock(&self) -> CoreResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| CoreError::Adapter {
            adapter: "engram-store-knowledge-sqlite".to_owned(),
            message: "connection lock poisoned".to_owned(),
        })
    }

    /// Lists knowledge graphs visible to `scope` (store-specific; not on a port).
    /// Used by the whole-graph explorer to enumerate ingested sources/repos.
    pub async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_graphs ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut graphs = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let graph = serde_json::from_str::<KnowledgeGraph>(&json).map_err(json_error)?;
            if scope_allows(&graph.scope, scope) {
                graphs.push(graph);
            }
        }
        Ok(graphs)
    }

    /// Lists knowledge entities visible to `scope`. Each entity carries its
    /// `graph_id` so the explorer can cluster by source/repo.
    pub async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_entities ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let entity = serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error)?;
            if scope_allows(&entity.scope, scope) {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Lists knowledge chunks visible to `scope`. Chunks carry the actual
    /// document/code text so Q&A can explain what code does (not just its
    /// call-graph edges).
    pub async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_chunks ORDER BY document_id, id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut chunks = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let chunk = serde_json::from_str::<KnowledgeChunk>(&json).map_err(json_error)?;
            // Chunks inherit visibility from their source.
            let source = source_for_chunk(&connection, &chunk)?;
            if source
                .map(|s| scope_allows(&s.scope, scope))
                .unwrap_or(false)
            {
                chunks.push(chunk);
            }
        }
        Ok(chunks)
    }

    /// Lists knowledge sources (repos) visible to `scope`. One record per scan.
    /// Much cheaper than loading all entities to compute per-repo stats.
    pub async fn list_sources(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_sources ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut sources = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let source = serde_json::from_str::<KnowledgeSource>(&json).map_err(json_error)?;
            if scope_allows(&source.scope, scope) {
                sources.push(source);
            }
        }
        Ok(sources)
    }

    /// Lists `KnowledgeEntity` records belonging to a specific repository (via
    /// `graph_id IN (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?)`),
    /// visible to `scope`. The per-source `EntityKind::Repository` node has
    /// `graph_id = None` and is NOT included in this result set; reach it via its
    /// `belongs_to` edges returned by `list_relationships_by_source`.
    pub async fn list_entities_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_entities \
                 WHERE graph_id IN \
                     (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?1) \
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![stable_source_key], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut entities = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let entity = serde_json::from_str::<KnowledgeEntity>(&json).map_err(json_error)?;
            if scope_allows(&entity.scope, scope) {
                entities.push(entity);
            }
        }
        Ok(entities)
    }

    /// Lists `KnowledgeRelationship` records belonging to a specific repository
    /// (via `graph_id IN (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?)`),
    /// visible to `scope`. This includes the `belongs_to` edges that link document
    /// graphs to the per-source `EntityKind::Repository` node (those edges carry
    /// the document graph's `graph_id`).
    pub async fn list_relationships_by_source(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT record_json FROM knowledge_relationships \
                 WHERE graph_id IN \
                     (SELECT id FROM knowledge_graphs WHERE stable_source_key = ?1) \
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![stable_source_key], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut relationships = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let relationship =
                serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error)?;
            if scope_allows(&relationship.scope, scope) {
                relationships.push(relationship);
            }
        }
        Ok(relationships)
    }

    /// Lists knowledge relationships visible to `scope`.
    pub async fn list_relationships(
        &self,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT record_json FROM knowledge_relationships ORDER BY id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut relationships = Vec::new();
        for row in rows {
            let json = row.map_err(sql_error)?;
            let relationship =
                serde_json::from_str::<KnowledgeRelationship>(&json).map_err(json_error)?;
            if scope_allows(&relationship.scope, scope) {
                relationships.push(relationship);
            }
        }
        Ok(relationships)
    }
}

/// Caps the number of relationships `validate_graph` scans, so advisory
/// validation over an oversized graph stays bounded.
pub(crate) const VALIDATE_RELATIONSHIP_LIMIT: usize = 5_000;

/// Builds the advisory provenance stamped on every validation finding. Validation
/// is deterministic and carries no input evidence, so a fixed system actor is used.
pub(crate) fn validation_provenance(ontology_id: &OntologyId, now: chrono::DateTime<Utc>) -> Provenance {
    Provenance {
        source: format!("ontology:{ontology_id}"),
        actor: Actor {
            id: Id::from("engram-ontology-validator"),
            kind: ActorKind::System,
            display_name: Some("Ontology validator".to_owned()),
            metadata: None,
        },
        observed_at: now,
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("ontology_validation".to_owned()),
    }
}

/// Loads the `KnowledgeSource` that owns a chunk (chunk -> document -> source).
fn source_for_chunk(
    connection: &Connection,
    chunk: &KnowledgeChunk,
) -> CoreResult<Option<KnowledgeSource>> {
    let document = connection
        .query_row(
            "SELECT record_json FROM knowledge_documents WHERE id = ?1",
            params![chunk.document_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| serde_json::from_str::<SourceDocument>(&json).map_err(json_error))
        .transpose()?;
    let Some(document) = document else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT record_json FROM knowledge_sources WHERE id = ?1",
            params![document.source_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| serde_json::from_str::<KnowledgeSource>(&json).map_err(json_error))
        .transpose()
}
