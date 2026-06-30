//! SQLite schema management for the knowledge adapter.
//!
//! Each knowledge record is stored losslessly as contract JSON, with scope and
//! lookup columns indexed for repository reads. Scope columns live only on
//! records that carry their own scope (sources, entities, relationships, graphs,
//! concept schemes); chunks, documents, concepts, and relations inherit
//! visibility from their owner and are filtered by joining to it.

use engram_runtime::{CoreError, CoreResult};
use rusqlite::Connection;

/// Creates the SQLite tables required by the knowledge adapter.
pub(crate) fn initialize_schema(connection: &Connection) -> CoreResult<()> {
    connection
        .execute_batch(
            r#"
            -- WAL allows concurrent readers + one writer (rayon scan workers write
            -- while the UI polls reads). busy_timeout makes a contended connection
            -- wait instead of failing immediately with "database is locked".
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS knowledge_sources (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS knowledge_documents (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS knowledge_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS knowledge_entities (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS knowledge_relationships (
                id TEXT PRIMARY KEY,
                graph_id TEXT,
                subject_id TEXT,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS knowledge_graphs (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS concept_schemes (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS concepts (
                id TEXT PRIMARY KEY,
                scheme_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS concept_relations (
                id TEXT PRIMARY KEY,
                scheme_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ontologies (
                id TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                subject TEXT,
                workspace TEXT,
                session TEXT,
                environment TEXT,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ontology_classes (
                id TEXT PRIMARY KEY,
                ontology_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ontology_properties (
                id TEXT PRIMARY KEY,
                ontology_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ontology_axioms (
                id TEXT PRIMARY KEY,
                ontology_id TEXT NOT NULL,
                record_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_document ON knowledge_chunks(document_id);
            CREATE INDEX IF NOT EXISTS idx_chunks_source ON knowledge_chunks(source_id);
            CREATE INDEX IF NOT EXISTS idx_documents_source ON knowledge_documents(source_id);
            CREATE INDEX IF NOT EXISTS idx_relationships_graph_subject
                ON knowledge_relationships(graph_id, subject_id);
            CREATE INDEX IF NOT EXISTS idx_concepts_scheme ON concepts(scheme_id);
            CREATE INDEX IF NOT EXISTS idx_concept_relations_scheme ON concept_relations(scheme_id);
            CREATE INDEX IF NOT EXISTS idx_ontology_classes ON ontology_classes(ontology_id);
            CREATE INDEX IF NOT EXISTS idx_ontology_properties ON ontology_properties(ontology_id);
            CREATE INDEX IF NOT EXISTS idx_ontology_axioms ON ontology_axioms(ontology_id);
            "#,
        )
        .map_err(sql_error)
}

/// Converts SQLite errors into the stable core adapter error surface.
pub(crate) fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-knowledge-sqlite".to_owned(),
        message: error.to_string(),
    }
}

/// Converts contract JSON serialization errors into a core adapter failure.
pub(crate) fn json_error(error: serde_json::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-knowledge-sqlite".to_owned(),
        message: error.to_string(),
    }
}
