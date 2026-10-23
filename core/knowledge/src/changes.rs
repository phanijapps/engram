//! Knowledge change-log and scan-checkpoint ports (enhancements adoption,
//! slice 3).
//!
//! Reads only: mutation logging is the knowledge adapter's obligation on its
//! existing operations (an append forgeable by tools would corrupt cursor
//! semantics), so no `append_change` port exists. Checkpoints are written by
//! the scan coordinator through [`ScanCheckpointStore`].

use async_trait::async_trait;
use engram_domain::{KnowledgeChangesPage, KnowledgeChangesRequest, ScanCheckpoint, Scope};
use engram_runtime::CoreResult;

/// Scoped, resumable reads over the persisted knowledge change log.
// async_trait codegen stamps #[must_use] on the boxed-future helper while
// CoreResult is already must_use — newer clippy flags the combination
// (same allow as the procedures port).
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait KnowledgeChangeLog: Send + Sync {
    /// One bounded page after the opaque cursor. A cursor older than the
    /// retained window returns `reset_required = true` (never a silently
    /// valid empty page).
    async fn changes_since(
        &self,
        request: &KnowledgeChangesRequest,
    ) -> CoreResult<KnowledgeChangesPage>;

    /// Latest retained checkpoint for one (scope, stable source key), if any.
    async fn latest_checkpoint(
        &self,
        scope: &Scope,
        stable_source_key: &str,
    ) -> CoreResult<Option<ScanCheckpoint>>;
}

/// Write side for scan checkpoints (the scan coordinator owns when a scan
/// starts, completes, or degrades).
// async_trait codegen stamps #[must_use] on the boxed-future helper while
// CoreResult is already must_use — newer clippy flags the combination
// (same allow as the procedures port).
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait ScanCheckpointStore: Send + Sync {
    /// Upserts the checkpoint for (scope, stable source key). A checkpoint
    /// row is REPLACED only by a newer scan run id (the same run updates in
    /// place; a stale run never overwrites a newer one).
    async fn put_checkpoint(&self, checkpoint: ScanCheckpoint) -> CoreResult<ScanCheckpoint>;
}
