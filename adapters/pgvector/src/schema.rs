//! DDL for the pgvector backend — mirrors the SQLite schema with Postgres-native types.
//!
//! - JSONB for record_json (instead of SQLite TEXT) — queryable + indexed.
//! - pgvector's `vector(dimensions)` type for the embedding column + HNSW index.
//! - Scope columns (tenant/subject/workspace/session/environment) on every scoped table.
//! - Every table carries `created_at` + `last_updated_at` (TIMESTAMPTZ, default now()).

/// Creates the pgvector extension + every table the backend needs.
/// Idempotent (IF NOT EXISTS on every statement). Run once on first connect.
pub const SCHEMA_SQL: &str = r#"
-- === Extension ===
CREATE EXTENSION IF NOT EXISTS vector;

-- === Memory ===
CREATE TABLE IF NOT EXISTS memories (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories (tenant, subject, workspace);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories (created_at DESC);

-- === Knowledge: sources → documents → chunks ===
CREATE TABLE IF NOT EXISTS knowledge_sources (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_sources_scope ON knowledge_sources (tenant, subject, workspace);

CREATE TABLE IF NOT EXISTS knowledge_documents (
    id                TEXT PRIMARY KEY,
    source_id         TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE CASCADE,
    record_json       JSONB NOT NULL,
    stable_source_key TEXT,
    path              TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_documents_source_path ON knowledge_documents (stable_source_key, path);

CREATE TABLE IF NOT EXISTS knowledge_chunks (
    id             TEXT PRIMARY KEY,
    document_id    TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
    source_id      TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE CASCADE,
    record_json    JSONB NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_chunks_document ON knowledge_chunks (document_id);

-- === Knowledge: graph entities + relationships + graphs ===
CREATE TABLE IF NOT EXISTS knowledge_entities (
    id             TEXT PRIMARY KEY,
    graph_id       TEXT,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    record_json    JSONB NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_entities_scope ON knowledge_entities (tenant, subject, workspace);
CREATE INDEX IF NOT EXISTS idx_entities_graph ON knowledge_entities (graph_id);

CREATE TABLE IF NOT EXISTS knowledge_relationships (
    id             TEXT PRIMARY KEY,
    graph_id       TEXT,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    record_json    JSONB NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_rels_scope ON knowledge_relationships (tenant, subject, workspace);

CREATE TABLE IF NOT EXISTS knowledge_graphs (
    id                TEXT PRIMARY KEY,
    tenant            TEXT NOT NULL,
    subject           TEXT,
    workspace         TEXT,
    session           TEXT,
    environment       TEXT,
    stable_source_key TEXT,
    path              TEXT,
    record_json       JSONB NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_graphs_source ON knowledge_graphs (stable_source_key);
CREATE INDEX IF NOT EXISTS idx_graphs_scope ON knowledge_graphs (tenant, subject, workspace);

-- === Vector index (pgvector) ===
CREATE TABLE IF NOT EXISTS vectors (
    id             TEXT PRIMARY KEY,
    embedding      vector({dimensions}),
    target_type    TEXT,
    target_id      TEXT,
    model          TEXT,
    dimensions     INTEGER,
    content_hash   TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- pgvector HNSW vector indexes support up to 2000 dimensions. Wider
-- supported vectors retain exact cosine search, without an approximate index.
-- Inspect the actual column too: IF NOT EXISTS can retain an older dimension.
DO $vector_index$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'vectors'::regclass
               AND attname = 'embedding' AND atttypmod BETWEEN 1 AND 2000) THEN
        CREATE INDEX IF NOT EXISTS idx_vectors_embedding
            ON vectors USING hnsw (embedding vector_cosine_ops);
    END IF;
END $vector_index$;

-- === Belief + Contradiction ===
CREATE TABLE IF NOT EXISTS beliefs (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_beliefs_scope ON beliefs (tenant, subject, workspace);

CREATE TABLE IF NOT EXISTS contradictions (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- === Hierarchy ===
CREATE TABLE IF NOT EXISTS hierarchy_nodes (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_hierarchy_scope ON hierarchy_nodes (tenant, subject, workspace);

CREATE TABLE IF NOT EXISTS hierarchy_relations (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- === Procedures ===
CREATE TABLE IF NOT EXISTS procedures (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_procedures_scope ON procedures (tenant, subject, workspace);

-- === Memory events ===
CREATE TABLE IF NOT EXISTS memory_events (
    id             TEXT PRIMARY KEY,
    record_json    JSONB NOT NULL,
    tenant         TEXT NOT NULL,
    subject        TEXT,
    workspace      TEXT,
    session        TEXT,
    environment    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_events_scope ON memory_events (tenant, subject, workspace);
"#;

/// Substitutes the vector dimensions into the schema SQL.
pub fn schema_sql(dimensions: u32) -> String {
    let mut sql = SCHEMA_SQL.replace("{dimensions}", &dimensions.to_string());
    stamp_schema_version(&mut sql);
    sql
}

/// Base record schema for embeddings-off startup. Do not preselect a vector
/// dimension or touch an existing vector index before a provider is enabled.
/// Section markers delimit the optional vector DDL in the shared template.
pub fn schema_sql_without_vectors() -> String {
    let (before, vectors_and_after) = SCHEMA_SQL
        .split_once("-- === Vector index (pgvector) ===")
        .expect("vector section marker in schema template");
    let (_, after) = vectors_and_after
        .split_once("-- === Belief + Contradiction ===")
        .expect("post-vector section marker in schema template");
    let mut sql = format!("{before}\n-- === Belief + Contradiction ==={after}");
    stamp_schema_version(&mut sql);
    sql
}

/// The pgvector schema version — bumped on any non-additive DDL change.
/// Stamped at open (below) so an operator can query
/// `SELECT * FROM schema_meta` to see which schema a database carries and
/// detect version skew before it becomes a mystery.
pub const SCHEMA_VERSION: &str = "1";

/// Stamps the schema version into `schema_meta` (idempotent upsert).
pub fn stamp_schema_version(sql: &mut String) {
    sql.push_str(&format!(
        "\nCREATE TABLE IF NOT EXISTS schema_meta (\
            key TEXT PRIMARY KEY, value TEXT NOT NULL);\
         INSERT INTO schema_meta (key, value) VALUES ('schema_version', '{SCHEMA_VERSION}')\
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value;\n"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_schema_does_not_create_vector_table_or_index() {
        let sql = schema_sql_without_vectors();
        assert!(!sql.contains("CREATE TABLE IF NOT EXISTS vectors"));
        assert!(!sql.contains("idx_vectors_embedding"));
        assert!(!sql.contains("{dimensions}"));
        for table in [
            "memories",
            "knowledge_chunks",
            "beliefs",
            "procedures",
            "hierarchy_nodes",
            "schema_meta",
        ] {
            assert!(
                sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
                "missing {table}"
            );
        }
        assert!(schema_sql(768).contains("vector(768)"));
    }
}
