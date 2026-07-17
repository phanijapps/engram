//! Surreal vector cell — `VectorIndex` over embedded SurrealKV (MTREE).
//!
//! Mirrors `engram-store-sqlite::vector` at the VectorIndex contract: stores
//! vectors in a `vector_record` table with an MTREE index (spike-verified KNN)
//! and validates embedding-space identity + dimensions. The embedding *provider*
//! (query → vector) is wired at the retrieval-lane level, not here — this index
//! only stores/searches vectors, exactly like the SQLite cell.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{EmbeddingSpace, Id};
use engram_retrieval::VectorIndex;
use engram_runtime::{CoreError, CoreResult};
use serde::Deserialize;

use crate::util::surreal_err;
use crate::SurrealConnection;

const TABLE: &str = "vector_record";

#[derive(Deserialize)]
struct VectorRow {
    target_id: String,
    embedding: Vec<f32>,
}

/// `VectorIndex` backed by embedded SurrealKV (MTREE KNN).
pub struct SurrealVectorIndex {
    conn: Arc<SurrealConnection>,
    embedding_space: EmbeddingSpace,
}

impl SurrealVectorIndex {
    /// Creates a vector index over a shared Surreal connection for the given
    /// embedding space.
    pub fn new(conn: Arc<SurrealConnection>, embedding_space: EmbeddingSpace) -> Self {
        Self {
            conn,
            embedding_space,
        }
    }

    fn validate(&self, space: &EmbeddingSpace, vector_len: usize) -> CoreResult<()> {
        if space != &self.embedding_space {
            return Err(CoreError::InvalidRequest {
                reason: "embedding_space_mismatch".to_string(),
            });
        }
        if vector_len != self.embedding_space.dimensions as usize {
            return Err(CoreError::InvalidRequest {
                reason: "dimension_mismatch".to_string(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl VectorIndex for SurrealVectorIndex {
    fn embedding_space(&self) -> &EmbeddingSpace {
        &self.embedding_space
    }

    async fn insert(
        &self,
        target_id: &Id,
        embedding_space: &EmbeddingSpace,
        vector: Vec<f32>,
    ) -> CoreResult<()> {
        self.validate(embedding_space, vector.len())?;
        let db = self.conn.db().await?;
        // Ensure the MTREE index exists (idempotent — ignore "already exists").
        let dim = self.embedding_space.dimensions;
        let _ = db
            .query(&format!(
                "DEFINE INDEX vec_idx ON {TABLE} FIELDS embedding MTREE DIMENSION {dim}"
            ))
            .await;
        let vec_lit = vector_literal(&vector);
        db.query(&format!(
            "UPSERT type::thing('{TABLE}', $key) SET target_id = $target, embedding = {vec_lit}"
        ))
        .bind(("key", target_id.to_string()))
        .bind(("target", target_id.to_string()))
        .await
        .map_err(surreal_err)?;
        Ok(())
    }

    async fn search(
        &self,
        query_embedding_space: &EmbeddingSpace,
        query_vector: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>> {
        self.validate(query_embedding_space, query_vector.len())?;
        let db = self.conn.db().await?;
        let q_lit = vector_literal(&query_vector);
        let mut res = db
            .query(&format!(
                "SELECT target_id, embedding FROM {TABLE} WHERE embedding<|{limit}|>{q_lit}"
            ))
            .await
            .map_err(surreal_err)?;
        let rows: Vec<VectorRow> = res.take(0).map_err(surreal_err)?;
        let mut results = Vec::new();
        for row in rows {
            let id = Id::new(&row.target_id).map_err(|_| CoreError::Adapter {
                adapter: "surreal.vector".to_string(),
                message: format!("stored vector target_id is not a valid Id: {}", row.target_id),
            })?;
            results.push((id, cosine(&query_vector, &row.embedding)));
        }
        // Highest similarity first.
        results.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(results)
    }

    async fn delete_target(&self, target_id: &Id) -> CoreResult<()> {
        let db = self.conn.db().await?;
        db.query(&format!("DELETE type::thing('{TABLE}', $key)"))
            .bind(("key", target_id.to_string()))
            .await
            .map_err(surreal_err)?;
        Ok(())
    }

    async fn clear(&self) -> CoreResult<()> {
        let db = self.conn.db().await?;
        db.query(&format!("DELETE {TABLE}"))
            .await
            .map_err(surreal_err)?;
        Ok(())
    }
}

/// Formats a vector as a SurrealQL inline literal `[0.1,0.2,...]`. The KNN
/// operator and MTREE embedding field require the vector inline (a bound
/// `Vec<f32>` parameter is rejected by the vector conversion).
fn vector_literal(v: &[f32]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Cosine similarity in [−1, 1]; returns 0.0 for a zero vector.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}
