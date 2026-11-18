//! Storage-only contributions to the single governed memory implementation.
use crate::{CoreResult, MemoryEventRepository, MemoryRepository};
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::effect::ObservedResult;

#[async_trait]
pub trait MemoryStorage: MemoryRepository + MemoryEventRepository + Send + Sync {
    async fn enumerate(&self, scope: &Scope) -> CoreResult<Vec<MemoryRecord>>;
    async fn idempotent_response(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Option<WriteMemoryResponse>>;
    /// Record, Written event and optional stored response share one durable effect.
    async fn commit_write(
        &self,
        key: Option<&str>,
        record: MemoryRecord,
        event: MemoryEvent,
    ) -> ObservedResult<WriteMemoryResponse>;
    async fn remove(&self, id: &MemoryId, scope: &Scope) -> CoreResult<bool>;
}
