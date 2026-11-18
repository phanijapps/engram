//! Compatibility delegation to the canonical governed memory service.
use super::engine::SqlMemoryService;
use engram_domain::*;
use engram_memory::{CoreResult, MemoryService};
pub(crate) async fn write_memory(
    service: &SqlMemoryService,
    request: WriteMemoryRequest,
) -> CoreResult<WriteMemoryResponse> {
    service.canonical().write_memory(request).await
}
