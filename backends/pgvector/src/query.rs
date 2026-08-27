//! Postgres-backed `KnowledgeQuery` — the read surface the MCP code-intel
//! tools need (`search`, `symbol_context`, `architecture`, `whats_changed`,
//! the scan's lexical delta feed + embed listing).
//!
//! The trait lives in the facade (`engram_integration::KnowledgeQuery`) so
//! implementing it belongs to this recipe (the composition layer), not to the
//! `adapters/pgvector` cells (which implement core ports only). Scope matching
//! mirrors the cells' `get_scoped` shape: `tenant` equality + `subject` /
//! `workspace` matching NULL-or-equal — the strict-scope read contract.

use async_trait::async_trait;
use engram_domain::{
    KnowledgeChunk, KnowledgeEntity, KnowledgeGraph, KnowledgeRelationship, Scope,
};
use engram_integration::KnowledgeQuery;
use engram_runtime::{CoreError, CoreResult};
use engram_store_pgvector::PgConnection;

/// `KnowledgeQuery` over the pgvector knowledge tables.
pub(crate) struct PgKnowledgeQuery {
    conn: PgConnection,
}

impl PgKnowledgeQuery {
    pub(crate) fn new(conn: PgConnection) -> Self {
        Self { conn }
    }

    fn pg_err(e: String) -> CoreError {
        CoreError::Adapter {
            adapter: "engram-store-pgvector".to_owned(),
            message: e,
        }
    }

    /// Scope predicate shared by every list query — identical to the cells'
    /// `get_scoped` WHERE clause so reads and queries agree on scope.
    const SCOPE_WHERE: &'static str =
        "tenant=$1 AND (subject IS NULL OR subject=$2) AND (workspace IS NULL OR workspace=$3)";

    async fn list_records<T: serde::de::DeserializeOwned + Send>(
        &self,
        table: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<T>> {
        let sql = format!(
            "SELECT record_json FROM {table} WHERE {} ORDER BY id",
            Self::SCOPE_WHERE
        );
        let rows = self.conn.block_on(async {
            self.conn
                .client
                .query(&sql, &[&scope.tenant, &scope.subject, &scope.workspace])
                .await
                .map_err(|e| Self::pg_err(e.to_string()))
        })?;
        rows.into_iter()
            .map(|row| {
                let value: serde_json::Value = row.get(0);
                serde_json::from_value(value).map_err(|e| Self::pg_err(e.to_string()))
            })
            .collect()
    }
}

#[async_trait]
impl KnowledgeQuery for PgKnowledgeQuery {
    async fn list_entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        self.list_records("knowledge_entities", scope).await
    }

    async fn list_relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        self.list_records("knowledge_relationships", scope).await
    }

    async fn list_chunks(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeChunk>> {
        self.list_records("knowledge_chunks", scope).await
    }

    async fn list_graphs(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeGraph>> {
        self.list_records("knowledge_graphs", scope).await
    }
}
