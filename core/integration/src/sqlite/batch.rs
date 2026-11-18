//! SQLite construction delegates all batch policy to the shared composition.
use crate::storage_composition::{CallPorts, CompositionKnowledge, PortStorageComposition};
use crate::{BatchIngest, BatchIngestRequest, BatchOutcome, TransactionGuarantee};
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::CoreResult;
use engram_store_sqlite::{SqlKnowledgeStore, SqlMemoryService};
use std::sync::Arc;
#[async_trait]
impl CompositionKnowledge for SqlKnowledgeStore {
    async fn put_source_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeSource,
    ) -> CoreResult<KnowledgeSource> {
        SqlKnowledgeStore::put_source_in_scope(self, expected, record).await
    }
    async fn put_document_in_scope(
        &self,
        expected: &Scope,
        record: SourceDocument,
    ) -> CoreResult<SourceDocument> {
        SqlKnowledgeStore::put_document_in_scope(self, expected, record).await
    }
    async fn put_chunk_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeChunk,
    ) -> CoreResult<KnowledgeChunk> {
        SqlKnowledgeStore::put_chunk_in_scope(self, expected, record).await
    }
    async fn put_entity_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeEntity,
    ) -> CoreResult<KnowledgeEntity> {
        SqlKnowledgeStore::put_entity_in_scope(self, expected, record).await
    }
    async fn put_relationship_in_scope(
        &self,
        expected: &Scope,
        record: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        SqlKnowledgeStore::put_relationship_in_scope(self, expected, record).await
    }
    async fn registry_source(&self, id: &SourceId) -> CoreResult<Option<KnowledgeSource>> {
        self.get_source_by_id(id).await
    }
}
pub struct SqlBatchIngest {
    inner: crate::batch_runtime::SharedBatchIngest,
}
impl SqlBatchIngest {
    pub fn new(memory: Arc<SqlMemoryService>, knowledge: Arc<SqlKnowledgeStore>) -> Self {
        let identity = format!("{:p}:{:p}", Arc::as_ptr(&memory), Arc::as_ptr(&knowledge));
        let ports = CallPorts { memory, knowledge };
        Self {
            inner: crate::batch_runtime::SharedBatchIngest::new(Arc::new(
                PortStorageComposition::new(ports, "sqlite", "local", identity),
            )),
        }
    }
}
#[async_trait]
impl BatchIngest for SqlBatchIngest {
    fn transaction_guarantee(&self) -> TransactionGuarantee {
        self.inner.transaction_guarantee()
    }
    async fn ingest(&self, request: BatchIngestRequest) -> CoreResult<BatchOutcome> {
        self.inner.ingest(request).await
    }
}
