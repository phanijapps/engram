//! SQLite vector index implementation.
//!
//! This module owns the first sqlite-vec adapter slice: table creation,
//! dimension validation, insertion, and nearest-neighbor reads. It does not own
//! embeddings, hybrid ranking, policy decisions, or portable retrieval
//! contracts.

use engram_domain::EmbeddingTargetType;
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, params};

use crate::{
    entry::{VectorEntry, VectorSearchResult},
    extension::register_sqlite_vec,
    vector::serialize_f32_vector,
};

/// SQLite-backed vector index using the sqlite-vec extension.
pub struct SqliteVectorIndex {
    connection: Connection,
    dimensions: u32,
}

impl SqliteVectorIndex {
    /// Opens an in-memory sqlite-vec index for local tests and fixtures.
    ///
    /// The dimensions are fixed at table creation so insert and query paths can
    /// reject mismatched vectors before sqlite-vec sees them.
    pub fn open_in_memory(dimensions: u32) -> CoreResult<Self> {
        if dimensions == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "dimensions must be greater than zero".to_owned(),
            });
        }
        register_sqlite_vec();
        let connection = Connection::open_in_memory().map_err(sql_error)?;
        connection
            .execute(
                &format!(
                    r#"
                    CREATE VIRTUAL TABLE vectors USING vec0(
                        id text primary key,
                        embedding float[{dimensions}],
                        target_type text,
                        target_id text,
                        model text,
                        dimensions integer,
                        content_hash text
                    )
                    "#
                ),
                [],
            )
            .map_err(sql_error)?;
        Ok(Self {
            connection,
            dimensions,
        })
    }

    /// Inserts one target embedding into the vector index.
    ///
    /// The target fields preserve the link back to memory, chunk, entity, or
    /// concept records while keeping vector storage separate from domain truth.
    pub fn insert(&self, entry: VectorEntry) -> CoreResult<()> {
        validate_dimensions(entry.dimensions, self.dimensions, entry.embedding.len())?;
        let embedding = serialize_f32_vector(&entry.embedding)?;
        self.connection
            .execute(
                r#"
                INSERT INTO vectors
                    (id, embedding, target_type, target_id, model, dimensions, content_hash)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    entry.id,
                    embedding,
                    target_type_name(&entry.target_type),
                    entry.target_id,
                    entry.model,
                    entry.dimensions,
                    entry.content_hash
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    /// Returns nearest vector targets ordered by sqlite-vec distance.
    ///
    /// Ranking fusion and policy filtering happen outside this adapter; this
    /// method only exposes raw nearest-neighbor rows and distances.
    pub fn search(&self, query: &[f32], limit: u32) -> CoreResult<Vec<VectorSearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        validate_dimensions(self.dimensions, self.dimensions, query.len())?;
        let query = serialize_f32_vector(query)?;
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, target_type, target_id, model, dimensions, content_hash, distance
                FROM vectors
                WHERE embedding MATCH ?1 AND k = ?2
                ORDER BY distance
                "#,
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![query, limit], |row| {
                let target_type: String = row.get(1)?;
                Ok(VectorSearchResult {
                    id: row.get(0)?,
                    target_type: parse_target_type(&target_type).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            1,
                            "target_type".to_owned(),
                            rusqlite::types::Type::Text,
                        )
                    })?,
                    target_id: row.get(2)?,
                    model: row.get(3)?,
                    dimensions: row.get(4)?,
                    content_hash: row.get(5)?,
                    distance: row.get(6)?,
                })
            })
            .map_err(sql_error)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(sql_error)?);
        }
        Ok(results)
    }

    /// Removes all vectors from the index.
    pub fn clear(&self) -> CoreResult<()> {
        self.connection
            .execute("DELETE FROM vectors", [])
            .map_err(sql_error)?;
        Ok(())
    }
}

fn validate_dimensions(expected: u32, declared: u32, actual: usize) -> CoreResult<()> {
    if declared != expected || actual != expected as usize {
        return Err(CoreError::InvalidRequest {
            reason: format!(
                "vector dimensions mismatch: expected {expected}, declared {declared}, actual {actual}"
            ),
        });
    }
    Ok(())
}

fn target_type_name(target_type: &EmbeddingTargetType) -> &'static str {
    match target_type {
        EmbeddingTargetType::Memory => "memory",
        EmbeddingTargetType::Chunk => "chunk",
        EmbeddingTargetType::Entity => "entity",
        EmbeddingTargetType::Concept => "concept",
    }
}

fn parse_target_type(value: &str) -> Option<EmbeddingTargetType> {
    match value {
        "memory" => Some(EmbeddingTargetType::Memory),
        "chunk" => Some(EmbeddingTargetType::Chunk),
        "entity" => Some(EmbeddingTargetType::Entity),
        "concept" => Some(EmbeddingTargetType::Concept),
        _ => None,
    }
}

fn sql_error(error: rusqlite::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-vector".to_owned(),
        message: error.to_string(),
    }
}
