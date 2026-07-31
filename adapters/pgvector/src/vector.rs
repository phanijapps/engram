//! PgVectorIndex — VectorIndex impl over pgvector.
//!
//! Uses text-formatted vectors (`[1.0,2.0,...]`) + the `<=>` cosine distance operator.
//! Avoids needing the pgvector Rust crate (the text format is native pgvector input).

use async_trait::async_trait;
use engram_domain::{EmbeddingSpace, Id};
use engram_retrieval::VectorIndex;
use engram_runtime::{CoreError, CoreResult};

use crate::connection::PgConnection;

pub struct PgVectorIndex {
    conn: PgConnection,
    space: EmbeddingSpace,
}

impl PgVectorIndex {
    pub fn new(conn: PgConnection, space: EmbeddingSpace) -> Self {
        Self { conn, space }
    }

    fn pg_err(e: String) -> CoreError {
        CoreError::Adapter {
            adapter: "engram-store-pgvector".into(),
            message: e,
        }
    }

    fn vec_to_text(v: &[f32]) -> String {
        let inner = v
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("[{inner}]")
    }
}

#[async_trait]
impl VectorIndex for PgVectorIndex {
    fn embedding_space(&self) -> &EmbeddingSpace {
        &self.space
    }

    async fn insert(
        &self,
        target_id: &Id,
        space: &EmbeddingSpace,
        vector: Vec<f32>,
    ) -> CoreResult<()> {
        if space != &self.space {
            return Err(CoreError::InvalidRequest {
                reason: "embedding_space_mismatch".into(),
            });
        }
        let vec_text = Self::vec_to_text(&vector);
        let content_hash = crate::content_hash(&vector);
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO vectors (id, embedding, target_type, target_id, model, dimensions, content_hash) \
                 VALUES ($1, $2::vector, 'chunk', $1, $3, $4, $5) \
                 ON CONFLICT (id) DO UPDATE SET embedding=EXCLUDED.embedding, content_hash=EXCLUDED.content_hash, last_updated_at=now()",
                &[&target_id.to_string(), &vec_text, &space.model, &(space.dimensions as i32), &content_hash],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            Ok::<_, CoreError>(())
        })
    }

    async fn search(
        &self,
        space: &EmbeddingSpace,
        query: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>> {
        if space != &self.space {
            return Err(CoreError::InvalidRequest {
                reason: "embedding_space_mismatch".into(),
            });
        }
        let q_text = Self::vec_to_text(&query);
        self.conn.block_on(async {
            let rows = self
                .conn
                .client
                .query(
                    "SELECT id, 1 - (embedding <=> $1::vector) AS score FROM vectors \
                 ORDER BY embedding <=> $1::vector LIMIT $2",
                    &[&q_text, &(limit as i64)],
                )
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| {
                    let id: String = r.get(0);
                    let score: f64 = r.get(1);
                    (Id::from(id), score as f32)
                })
                .collect())
        })
    }

    async fn delete_target(&self, target_id: &Id) -> CoreResult<()> {
        self.conn.block_on(async {
            self.conn
                .client
                .execute("DELETE FROM vectors WHERE id=$1", &[&target_id.to_string()])
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            Ok::<_, CoreError>(())
        })
    }

    async fn gc_orphan_targets(&self, live: &[Id]) -> CoreResult<usize> {
        self.conn.block_on(async {
            let all: Vec<String> = self
                .conn
                .client
                .query("SELECT id FROM vectors", &[])
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?
                .iter()
                .map(|r| r.get::<_, String>(0))
                .collect();
            let live_set: std::collections::HashSet<&str> =
                live.iter().map(|id| id.as_str()).collect();
            let mut deleted = 0;
            for id in &all {
                if !live_set.contains(id.as_str()) {
                    self.conn
                        .client
                        .execute("DELETE FROM vectors WHERE id=$1", &[id])
                        .await
                        .map_err(|e| Self::pg_err(e.to_string()))?;
                    deleted += 1;
                }
            }
            Ok(deleted)
        })
    }

    async fn clear(&self) -> CoreResult<()> {
        self.conn.block_on(async {
            self.conn
                .client
                .execute("DELETE FROM vectors", &[])
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            Ok::<_, CoreError>(())
        })
    }
}
