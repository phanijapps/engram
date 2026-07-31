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
CREATE INDEX IF NOT EXISTS idx_vectors_embedding
    ON vectors USING hnsw (embedding vector_cosine_ops);

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
"#;

/// Substitutes the vector dimensions into the schema SQL.
pub fn schema_sql(dimensions: u32) -> String {
    SCHEMA_SQL.replace("{dimensions}", &dimensions.to_string())
}
