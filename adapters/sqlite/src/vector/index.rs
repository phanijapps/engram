//! SQLite vector index implementation.
//!
//! This module owns the first sqlite-vec adapter slice: table creation,
//! dimension validation, insertion, and nearest-neighbor reads. It does not own
//! embeddings, hybrid ranking, policy decisions, or portable retrieval
//! contracts.

use engram_domain::{EmbeddingSpace, EmbeddingTargetType, Id};
use engram_runtime::{CoreError, CoreResult};
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::{Arc, Mutex as StdMutex};

use crate::vector::{
    entry::{VectorEntry, VectorSearchResult},
    extension::register_sqlite_vec,
    vector::serialize_f32_vector,
};

use engram_retrieval::VectorIndex;

/// SQLite-backed vector index using the sqlite-vec extension.
pub struct SqliteVectorIndex {
    connection: Arc<StdMutex<Connection>>,
    dimensions: u32,
    embedding_space: EmbeddingSpace,
    requires_reindex: bool,
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
        create_vectors_table(&connection, dimensions)?;

        // Default embedding space for in-memory index. In-memory indexes are
        // ephemeral, so metadata is always empty and reindex is never required.
        let embedding_space =
            EmbeddingSpace::new("sqlite-vec", "default", dimensions, "query", None::<String>);

        Ok(Self {
            connection: Arc::new(StdMutex::new(connection)),
            dimensions,
            embedding_space,
            requires_reindex: false,
        })
    }

    /// Opens a file-backed sqlite-vec index whose vectors persist across
    /// processes.
    ///
    /// The vec0 virtual table and its shadow tables live in the SQLite file at
    /// `path`, so embeddings survive restarts — the durable backing for lazy
    /// query-time embeddings. The sqlite-vec extension is registered on every
    /// open; re-opening an existing file skips table creation (`IF NOT EXISTS`)
    /// and reads the existing shadow tables.
    pub fn open(path: &str, dimensions: u32) -> CoreResult<Self> {
        if dimensions == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "dimensions must be greater than zero".to_owned(),
            });
        }
        register_sqlite_vec();
        let connection = Connection::open(path).map_err(sql_error)?;
        create_vectors_table(&connection, dimensions)?;

        // Default embedding space for file-backed index. Metadata is resolved
        // so a reopened file built under a different space signals reindex.
        let requested =
            EmbeddingSpace::new("sqlite-vec", "default", dimensions, "query", None::<String>);
        let (embedding_space, requires_reindex) = resolve_meta_space(&connection, &requested);

        Ok(Self {
            connection: Arc::new(StdMutex::new(connection)),
            dimensions,
            embedding_space,
            requires_reindex,
        })
    }

    /// Opens a file-backed index configured for a specific embedding space.
    ///
    /// On first open the `embedding_space` is persisted to the index's metadata.
    /// On reopen, if the persisted space differs from `embedding_space`, the
    /// existing vectors are in a different space: the index's
    /// [`embedding_space`](Self::embedding_space) reflects the *persisted* space
    /// (the actual current space) and [`requires_reindex`](Self::requires_reindex)
    /// returns `true` so the caller can report `RequiresReindex`.
    pub fn open_with_embedding_space(
        path: &str,
        embedding_space: EmbeddingSpace,
    ) -> CoreResult<Self> {
        let dimensions = embedding_space.dimensions;
        if dimensions == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "dimensions must be greater than zero".to_owned(),
            });
        }
        register_sqlite_vec();
        let connection = Connection::open(path).map_err(sql_error)?;
        create_vectors_table(&connection, dimensions)?;
        let (persisted, requires_reindex) = resolve_meta_space(&connection, &embedding_space);
        Ok(Self {
            connection: Arc::new(StdMutex::new(connection)),
            dimensions,
            embedding_space: persisted,
            requires_reindex,
        })
    }

    /// Returns `true` when the index was reopened under a different embedding
    /// space than the one its vectors were built with.
    ///
    /// Callers should surface this as a `RequiresReindex` capability state and
    /// rebuild the index before inserting or querying.
    pub fn requires_reindex(&self) -> bool {
        self.requires_reindex
    }

    /// Inserts one target embedding into the vector index.
    ///
    /// The target fields preserve the link back to memory, chunk, entity, or
    /// concept records while keeping vector storage separate from domain truth.
    pub fn insert(&self, entry: VectorEntry) -> CoreResult<()> {
        validate_dimensions(entry.dimensions, self.dimensions, entry.embedding.len())?;
        let embedding = serialize_f32_vector(&entry.embedding)?;
        let conn = self.connection.lock().unwrap();
        conn.execute(
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
        let conn = self.connection.lock().unwrap();
        let mut statement = conn
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
        let conn = self.connection.lock().unwrap();
        conn.execute("DELETE FROM vectors", []).map_err(sql_error)?;
        Ok(())
    }

    /// Sets the embedding space for this index.
    ///
    /// This allows the embedding space to be configured after construction,
    /// typically when loading configuration or migrating between models.
    pub fn with_embedding_space(mut self, embedding_space: EmbeddingSpace) -> Self {
        self.embedding_space = embedding_space;
        self
    }
}

impl VectorIndex for SqliteVectorIndex {
    fn embedding_space(&self) -> &EmbeddingSpace {
        &self.embedding_space
    }

    fn insert(
        &self,
        target_id: &Id,
        embedding_space: &EmbeddingSpace,
        vector: Vec<f32>,
    ) -> CoreResult<()> {
        // Validate embedding space matches
        if embedding_space != self.embedding_space() {
            return Err(CoreError::InvalidRequest {
                reason: "embedding_space_mismatch".to_string(),
            });
        }

        // Validate vector dimensions match embedding space
        if vector.len() != self.embedding_space().dimensions as usize {
            return Err(CoreError::InvalidRequest {
                reason: "dimension_mismatch".to_string(),
            });
        }

        // Insert using the existing insert method with a VectorEntry
        let entry = VectorEntry {
            id: target_id.to_string(),
            target_type: EmbeddingTargetType::Chunk, // Default to chunk for VectorIndex
            target_id: target_id.to_string(),
            model: embedding_space.model.clone(),
            dimensions: embedding_space.dimensions,
            content_hash: content_hash_for(&vector),
            embedding: vector,
        };

        self.insert(entry)
    }

    fn search(
        &self,
        query_embedding_space: &EmbeddingSpace,
        query_vector: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>> {
        // Validate embedding space matches
        if query_embedding_space != self.embedding_space() {
            return Err(CoreError::InvalidRequest {
                reason: "embedding_space_mismatch".to_string(),
            });
        }

        // Validate vector dimensions match embedding space
        if query_vector.len() != self.embedding_space().dimensions as usize {
            return Err(CoreError::InvalidRequest {
                reason: "dimension_mismatch".to_string(),
            });
        }

        // Use existing search method
        let results = self.search(&query_vector, limit as u32)?;

        // Convert VectorSearchResult to (Id, f32) tuples. A stored id that
        // fails validation is an adapter-level corruption, not a panic.
        let converted = results
            .into_iter()
            .map(|result| {
                let id = Id::new(result.id).map_err(|_| CoreError::Adapter {
                    adapter: "engram-store-vector".to_string(),
                    message: format!("stored vector id is not a valid Id: {}", result.target_id),
                })?;
                Ok::<_, CoreError>((id, result.distance))
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(converted)
    }

    fn delete_target(&self, target_id: &Id) -> CoreResult<()> {
        let conn = self.connection.lock().unwrap();
        conn.execute("DELETE FROM vectors WHERE id = ?1", [target_id.as_str()])
            .map_err(sql_error)?;
        Ok(())
    }

    fn clear(&self) -> CoreResult<()> {
        self.clear()
    }
}

/// Creates the `vectors` vec0 virtual table if it does not already exist.
///
/// Shared by the in-memory and file-backed constructors so the schema stays in
/// one place. `IF NOT EXISTS` makes reopen idempotent for the persistent path.
fn create_vectors_table(connection: &Connection, dimensions: u32) -> CoreResult<()> {
    connection
        .execute(
            &format!(
                r#"
                CREATE VIRTUAL TABLE IF NOT EXISTS vectors USING vec0(
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
    // Metadata table records the embedding space the index was built with, so
    // a later reopen with a different configured space can signal RequiresReindex.
    connection
        .execute(
            r#"
            CREATE TABLE IF NOT EXISTS vector_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
            [],
        )
        .map_err(sql_error)?;
    Ok(())
}

/// Resolves the index's embedding space against persisted metadata.
///
/// On first open the metadata is empty: the `requested` space is persisted and
/// returned with `requires_reindex = false`. On reopen, the persisted space is
/// read back and compared to `requested`: if they differ, the existing vectors
/// are in a different space and `requires_reindex = true` is returned alongside
/// the *persisted* space (the index's actual current space).
fn resolve_meta_space(
    connection: &Connection,
    requested: &EmbeddingSpace,
) -> (EmbeddingSpace, bool) {
    let get = |key: &str| -> Option<String> {
        connection
            .query_row(
                "SELECT value FROM vector_meta WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    };
    let set = |key: &str, value: &str| {
        let _ = connection.execute(
            "INSERT OR REPLACE INTO vector_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        );
    };

    match get("provider") {
        Some(provider) => {
            let model = get("model").unwrap_or_default();
            let dims = get("dimensions")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(requested.dimensions);
            let profile = get("prompt_profile").unwrap_or_default();
            let normalization = get("normalization");
            let persisted = EmbeddingSpace::new(provider, model, dims, profile, normalization);
            let requires_reindex = &persisted != requested;
            (persisted, requires_reindex)
        }
        None => {
            set("provider", &requested.provider);
            set("model", &requested.model);
            set("dimensions", &requested.dimensions.to_string());
            set("prompt_profile", &requested.prompt_profile);
            if let Some(norm) = &requested.normalization {
                set("normalization", norm);
            }
            (requested.clone(), false)
        }
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

/// Computes a stable SHA-256 content hash for an embedding vector.
///
/// The hash is taken over the vector's raw little-endian f32 bytes, prefixed by
/// the dimension count, so two vectors with identical content produce identical
/// hashes and a dimension change is detectable.
fn content_hash_for(vector: &[f32]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(vector.len().to_le_bytes());
    for component in vector {
        hasher.update(component.to_le_bytes());
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(2 * digest.len() + 7);
    hex.push_str("sha256:");
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::entry::VectorEntry;
    use engram_domain::EmbeddingTargetType;

    #[test]
    fn file_backed_index_survives_reopen() {
        let dir = std::env::temp_dir().join("engram-vec-persist-test.sqlite");
        let _ = std::fs::remove_file(&dir);
        let path = dir.to_str().expect("path");

        let dims = 4u32;
        // Write a vector, then drop the handle (simulating a process restart).
        {
            let index = SqliteVectorIndex::open(path, dims).expect("open");
            index
                .insert(VectorEntry {
                    id: "chunk-1".to_owned(),
                    target_type: EmbeddingTargetType::Chunk,
                    target_id: "chunk-1".to_owned(),
                    model: "test".to_owned(),
                    dimensions: dims,
                    content_hash: "chunk-1".to_owned(),
                    embedding: vec![0.1, 0.2, 0.3, 0.4],
                })
                .expect("insert");
        }
        // Reopen: the vector must persist.
        let reopened = SqliteVectorIndex::open(path, dims).expect("reopen");
        let hits = reopened.search(&[0.1, 0.2, 0.3, 0.4], 1).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].target_id, "chunk-1", "vector must survive reopen");

        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn reopen_under_different_embedding_space_signals_reindex() {
        use engram_domain::EmbeddingSpace;
        let dir = std::env::temp_dir().join("engram-vec-reindex-test.sqlite");
        let _ = std::fs::remove_file(&dir);
        let path = dir.to_str().expect("path");

        let original = EmbeddingSpace::new("fastembed", "bge-small", 4, "query", None::<String>);
        // First open persists the embedding space; no reindex required.
        {
            let index =
                SqliteVectorIndex::open_with_embedding_space(path, original.clone()).unwrap();
            assert!(
                !index.requires_reindex(),
                "fresh index must not require reindex"
            );
        }
        // Reopen under the same space: still no reindex.
        let same = SqliteVectorIndex::open_with_embedding_space(path, original.clone()).unwrap();
        assert!(!same.requires_reindex());

        // Reopen under a different space: the persisted vectors are incompatible.
        let changed = EmbeddingSpace::new("ollama", "nomic-embed-text", 4, "query", None::<String>);
        let reopened = SqliteVectorIndex::open_with_embedding_space(path, changed).unwrap();
        assert!(
            reopened.requires_reindex(),
            "reopen under a different embedding space must signal RequiresReindex"
        );

        let _ = std::fs::remove_file(&dir);
    }
}
