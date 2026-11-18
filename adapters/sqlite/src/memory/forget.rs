//! Compatibility delegation to the canonical governed memory service.
use super::engine::SqlMemoryService;
use engram_domain::*;
use engram_memory::{CoreResult, MemoryService};
pub(crate) async fn forget(
    service: &SqlMemoryService,
    request: ForgetRequest,
) -> CoreResult<ForgetResult> {
    service.canonical().forget(request).await
}
