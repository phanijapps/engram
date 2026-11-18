//! Compatibility delegation to the canonical governed memory service.
use super::engine::SqlMemoryService;
use engram_domain::*;
use engram_memory::{CoreResult, MemoryService};
pub(crate) async fn retrieve(
    service: &SqlMemoryService,
    request: RetrievalRequest,
) -> CoreResult<ContextPayload> {
    service.canonical().retrieve(request).await
}
pub(crate) use engram_memory::retrieval::verdict_metadata;
