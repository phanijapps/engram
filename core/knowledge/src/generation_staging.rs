//! Durable staging writes that precede a generation's knowledge readiness receipt.
use async_trait::async_trait;
use engram_domain::{CodeLineage, IndexedSource, Scope};
use engram_runtime::CoreResult;

#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeGenerationStaging: Send + Sync {
    async fn stage_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: &CodeLineage,
    ) -> CoreResult<()>;
    async fn stage_source(&self, scope: &Scope, source: &IndexedSource) -> CoreResult<()>;
}
