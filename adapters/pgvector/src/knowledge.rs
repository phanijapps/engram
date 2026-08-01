//! PgKnowledgeStore — KnowledgeRepository + KnowledgeGraphRepository over Postgres.
//!
//! Uses a generic JSONB pattern: each scoped record is stored as (id, record_json, scope...).
//! Trait methods are thin SQL wrappers over the shared `PgConnection`.

use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{CoreResult, KnowledgeGraphRepository, KnowledgeRepository};
use engram_runtime::CoreError;
use serde_json;

use crate::connection::PgConnection;

pub struct PgKnowledgeStore {
    conn: PgConnection,
}

impl PgKnowledgeStore {
    pub fn new(conn: PgConnection) -> Self {
        Self { conn }
    }

    fn pg_err(e: String) -> CoreError {
        CoreError::Adapter {
            adapter: "engram-store-pgvector".into(),
            message: e,
        }
    }

    // -- generic scoped JSONB helpers --

    fn put_scoped(
        &self,
        table: &str,
        id: &str,
        json: serde_json::Value,
        s: &Scope,
    ) -> CoreResult<()> {
        self.conn.block_on(async {
            self.conn.client.execute(
                &format!("INSERT INTO {table} (id, record_json, tenant, subject, workspace, session, environment) \
                          VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (id) DO UPDATE SET \
                          record_json=EXCLUDED.record_json, last_updated_at=now()"),
                &[&id, &json, &s.tenant, &s.subject, &s.workspace, &s.session, &s.environment],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            Ok::<_, CoreError>(())
        })
    }

    fn get_scoped<T: serde::de::DeserializeOwned + Send>(
        &self,
        table: &str,
        id: &str,
        scope: &Scope,
    ) -> CoreResult<Option<T>> {
        self.conn.block_on(async {
            let row = self.conn.client.query_opt(
                &format!("SELECT record_json FROM {table} WHERE id=$1 AND tenant=$2 \
                          AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)"),
                &[&id, &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            row.map(|r| {
                let v: serde_json::Value = r.get(0);
                serde_json::from_value(v).map_err(|e| Self::pg_err(e.to_string()))
            }).transpose()
        })
    }

    fn delete_scoped(&self, table: &str, id: &str, scope: &Scope) -> CoreResult<bool> {
        self.conn.block_on(async {
            let n = self.conn.client.execute(
                &format!("DELETE FROM {table} WHERE id=$1 AND tenant=$2 \
                          AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)"),
                &[&id, &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            Ok(n > 0)
        })
    }
}

#[async_trait]
impl KnowledgeRepository for PgKnowledgeStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        let json = serde_json::to_value(&source).map_err(|e| Self::pg_err(e.to_string()))?;
        self.put_scoped(
            "knowledge_sources",
            &source.id.to_string(),
            json,
            &source.scope,
        )?;
        Ok(source)
    }

    async fn put_document(&self, doc: SourceDocument) -> CoreResult<SourceDocument> {
        let json = serde_json::to_value(&doc).map_err(|e| Self::pg_err(e.to_string()))?;
        let key = doc.path.as_deref().unwrap_or("");
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO knowledge_documents (id, source_id, record_json, stable_source_key, path) \
                 VALUES ($1,$2,$3,$4,$5) ON CONFLICT (id) DO UPDATE SET record_json=EXCLUDED.record_json, last_updated_at=now()",
                &[&doc.id.to_string(), &doc.source_id.to_string(), &json, &key, &doc.path],
            ).await.map_err(|e| Self::pg_err(e.to_string()))
        })?;
        Ok(doc)
    }

    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        let json = serde_json::to_value(&chunk).map_err(|e| Self::pg_err(e.to_string()))?;
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO knowledge_chunks (id, document_id, source_id, record_json) \
                 VALUES ($1,$2,$3,$4) ON CONFLICT (id) DO UPDATE SET record_json=EXCLUDED.record_json, last_updated_at=now()",
                &[&chunk.id.to_string(), &chunk.document_id.to_string(), &chunk.source_id.to_string(), &json],
            ).await.map_err(|e| Self::pg_err(e.to_string()))
        })?;
        Ok(chunk)
    }

    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        self.conn.block_on(async {
            let row = self.conn.client.query_opt(
                "SELECT c.record_json FROM knowledge_chunks c \
                 JOIN knowledge_sources s ON c.source_id = s.id \
                 WHERE c.id=$1 AND s.tenant=$2 AND (s.subject IS NULL OR s.subject=$3) AND (s.workspace IS NULL OR s.workspace=$4)",
                &[&id.to_string(), &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            row.map(|r| {
                let v: serde_json::Value = r.get(0);
                serde_json::from_value(v).map_err(|e| Self::pg_err(e.to_string()))
            }).transpose()
        })
    }

    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        let json = serde_json::to_value(&entity).map_err(|e| Self::pg_err(e.to_string()))?;
        let gid = entity.graph_id.as_ref().map(|g| g.to_string());
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO knowledge_entities (id, graph_id, tenant, subject, workspace, session, environment, record_json) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (id) DO UPDATE SET \
                 record_json=EXCLUDED.record_json, graph_id=EXCLUDED.graph_id, last_updated_at=now()",
                &[&entity.id.to_string(), &gid, &entity.scope.tenant, &entity.scope.subject,
                  &entity.scope.workspace, &entity.scope.session, &entity.scope.environment, &json],
            ).await.map_err(|e| Self::pg_err(e.to_string()))
        })?;
        Ok(entity)
    }

    async fn put_relationship(
        &self,
        rel: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        let json = serde_json::to_value(&rel).map_err(|e| Self::pg_err(e.to_string()))?;
        let gid = rel.graph_id.as_ref().map(|g| g.to_string());
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO knowledge_relationships (id, graph_id, tenant, subject, workspace, session, environment, record_json) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (id) DO UPDATE SET \
                 record_json=EXCLUDED.record_json, graph_id=EXCLUDED.graph_id, last_updated_at=now()",
                &[&rel.id.to_string(), &gid, &rel.scope.tenant, &rel.scope.subject,
                  &rel.scope.workspace, &rel.scope.session, &rel.scope.environment, &json],
            ).await.map_err(|e| Self::pg_err(e.to_string()))
        })?;
        Ok(rel)
    }

    async fn delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        self.delete_scoped("knowledge_entities", &id.to_string(), scope)
    }
    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        self.delete_scoped("knowledge_relationships", &id.to_string(), scope)
    }

    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        self.get_scoped("knowledge_entities", &id.to_string(), scope)
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        self.get_scoped("knowledge_relationships", &id.to_string(), scope)
    }

    async fn list_chunks_by_document(
        &self,
        doc_id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        self.conn.block_on(async {
            let rows = self.conn.client.query(
                "SELECT c.record_json FROM knowledge_chunks c JOIN knowledge_sources s ON c.source_id=s.id \
                 WHERE c.document_id=$1 AND s.tenant=$2 AND (s.subject IS NULL OR s.subject=$3) AND (s.workspace IS NULL OR s.workspace=$4)",
                &[&doc_id.to_string(), &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            rows.iter().map(|r| {
                serde_json::from_value::<KnowledgeChunk>(r.get(0)).map_err(|e| Self::pg_err(e.to_string()))
            }).collect()
        })
    }

    async fn delete_document(&self, id: &DocumentId, _scope: &Scope) -> CoreResult<bool> {
        self.conn.block_on(async {
            let n = self
                .conn
                .client
                .execute(
                    "DELETE FROM knowledge_documents WHERE id=$1",
                    &[&id.to_string()],
                )
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            Ok(n > 0)
        })
    }
    async fn delete_chunk(&self, id: &ChunkId, _scope: &Scope) -> CoreResult<bool> {
        self.conn.block_on(async {
            let n = self
                .conn
                .client
                .execute(
                    "DELETE FROM knowledge_chunks WHERE id=$1",
                    &[&id.to_string()],
                )
                .await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            Ok(n > 0)
        })
    }
}

#[async_trait]
impl KnowledgeGraphRepository for PgKnowledgeStore {
    async fn put_graph(&self, graph: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        let json = serde_json::to_value(&graph).map_err(|e| Self::pg_err(e.to_string()))?;
        let key = graph
            .metadata
            .as_ref()
            .and_then(|m| m.get("stableSourceKey"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = graph
            .metadata
            .as_ref()
            .and_then(|m| m.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        self.conn.block_on(async {
            self.conn.client.execute(
                "INSERT INTO knowledge_graphs (id, tenant, subject, workspace, session, environment, stable_source_key, path, record_json) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (id) DO UPDATE SET record_json=EXCLUDED.record_json, last_updated_at=now()",
                &[&graph.id.to_string(), &graph.scope.tenant, &graph.scope.subject,
                  &graph.scope.workspace, &graph.scope.session, &graph.scope.environment,
                  &key, &path, &json],
            ).await.map_err(|e| Self::pg_err(e.to_string()))
        })?;
        Ok(graph)
    }

    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        self.get_scoped("knowledge_graphs", &id.to_string(), scope)
    }

    async fn neighbors(
        &self,
        graph_id: &KnowledgeGraphId,
        _node_id: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        let lim = limit.unwrap_or(100) as i64;
        self.conn.block_on(async {
            let rows = self.conn.client.query(
                "SELECT record_json FROM knowledge_relationships WHERE graph_id=$1 \
                 AND tenant=$2 AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4) LIMIT $5",
                &[&graph_id.to_string(), &scope.tenant, &scope.subject, &scope.workspace, &lim],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            rows.iter().map(|r| {
                serde_json::from_value::<KnowledgeRelationship>(r.get(0)).map_err(|e| Self::pg_err(e.to_string()))
            }).collect()
        })
    }

    async fn delete_graph(&self, id: &KnowledgeGraphId, scope: &Scope) -> CoreResult<bool> {
        // Cascade: delete entities + relationships for this graph first, then the graph.
        let id_s = id.to_string();
        self.conn.block_on(async {
            self.conn.client.execute("DELETE FROM knowledge_relationships WHERE graph_id=$1", &[&id_s]).await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            self.conn.client.execute("DELETE FROM knowledge_entities WHERE graph_id=$1", &[&id_s]).await
                .map_err(|e| Self::pg_err(e.to_string()))?;
            let n = self.conn.client.execute(
                "DELETE FROM knowledge_graphs WHERE id=$1 AND tenant=$2 AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)",
                &[&id_s, &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            Ok(n > 0)
        })
    }

    async fn list_graphs_by_source(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        self.conn.block_on(async {
            let rows = self.conn.client.query(
                "SELECT record_json FROM knowledge_graphs WHERE stable_source_key=$1 \
                 AND tenant=$2 AND (subject IS NULL OR subject=$3) AND (workspace IS NULL OR workspace=$4)",
                &[&key, &scope.tenant, &scope.subject, &scope.workspace],
            ).await.map_err(|e| Self::pg_err(e.to_string()))?;
            rows.iter().map(|r| {
                serde_json::from_value::<KnowledgeGraph>(r.get(0)).map_err(|e| Self::pg_err(e.to_string()))
            }).collect()
        })
    }
}
