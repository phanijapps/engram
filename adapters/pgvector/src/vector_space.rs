//! Durable embedding identity adoption and schema compatibility; never migrates vectors.
use engram_domain::EmbeddingSpace;
use engram_runtime::{CoreError, CoreResult};

use crate::connection::{runtime, PgConnection};

fn mismatch() -> CoreError {
    CoreError::Conflict {
        reason: "pgvector embedding identity or dimensions differ from persisted vectors; select the original configuration, use a new store, or explicitly reindex before switching models".into(),
    }
}

fn pg_error(_: tokio_postgres::Error) -> CoreError {
    CoreError::Adapter {
        adapter: "engram-store-pgvector".into(),
        message: "could not verify persisted embedding identity".into(),
    }
}

pub(super) fn verify(conn: &mut PgConnection, space: &EmbeddingSpace) -> CoreResult<()> {
    if !(1..=8192).contains(&space.dimensions) {
        return Err(CoreError::InvalidRequest {
            reason: "pgvector dimensions must be between 1 and 8192".into(),
        });
    }
    runtime().block_on(async {
        let tx = conn.client.transaction().await.map_err(pg_error)?;
        // Serializes first adoption across recipe instances before metadata DDL.
        tx.execute("SELECT pg_advisory_xact_lock(7246623901279401)", &[]).await.map_err(pg_error)?;
        tx.batch_execute("LOCK TABLE vectors IN SHARE ROW EXCLUSIVE MODE").await.map_err(pg_error)?;
        let row = tx.query_opt("SELECT format_type(atttypid, atttypmod) FROM pg_attribute WHERE attrelid = 'vectors'::regclass AND attname = 'embedding' AND NOT attisdropped", &[]).await.map_err(pg_error)?.ok_or_else(mismatch)?;
        let actual: String = row.get(0);
        if actual != format!("vector({})", space.dimensions) { return Err(mismatch()); }
        tx.batch_execute("CREATE TABLE IF NOT EXISTS vector_embedding_space (singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton), space_json JSONB NOT NULL)").await.map_err(pg_error)?;
        let persisted = tx.query_opt("SELECT space_json FROM vector_embedding_space WHERE singleton = TRUE", &[]).await.map_err(pg_error)?;
        let identity = serde_json::to_value(space).map_err(|_| mismatch())?;
        if let Some(row) = persisted {
            let actual: serde_json::Value = row.get(0);
            if actual != identity { return Err(mismatch()); }
        } else {
            let default = EmbeddingSpace::new("fastembed", "BAAI/bge-small-en-v1.5", 384, "query", None::<String>);
            let has_vectors: bool = tx.query_one("SELECT EXISTS(SELECT 1 FROM vectors)", &[]).await.map_err(pg_error)?.get(0);
            if has_vectors && space != &default { return Err(mismatch()); }
            let dimensions = space.dimensions as i32;
            let inconsistent: bool = tx.query_one("SELECT EXISTS(SELECT 1 FROM vectors WHERE model IS DISTINCT FROM $1 OR dimensions IS DISTINCT FROM $2)", &[&space.model, &dimensions]).await.map_err(pg_error)?.get(0);
            if inconsistent { return Err(mismatch()); }
            tx.execute("INSERT INTO vector_embedding_space (singleton, space_json) VALUES (TRUE, $1)", &[&identity]).await.map_err(pg_error)?;
        }
        tx.commit().await.map_err(pg_error)
    })
}
