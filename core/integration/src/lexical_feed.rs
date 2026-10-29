//! Engine-neutral feed port for the lexical (BM25) search lane.
//!
//! Unified recall's lexical lane is an in-RAM Tantivy index that
//! [`bootstrap_sqlite`](crate::sqlite::bootstrap_sqlite) constructs **empty**
//! — nothing feeds it. Code ingestion (`scan_repo`) feeds code-symbol names
//! through this port so keyword `search`/`recall` return them. The port lets
//! that feed route through the [`EngramProvider`](crate::EngramProvider)
//! instead of the old `codegraph/mcp-server`'s direct `LexicalIndex` bypass.

use async_trait::async_trait;
use engram_runtime::CoreResult;

/// Write port: feed searchable text into the lexical (BM25) lane.
#[allow(clippy::double_must_use)]
// async_trait codegen stamps #[must_use] on the boxed-future helper; CoreResult is already must_use
#[async_trait]
pub trait LexicalFeed: Send + Sync {
    /// Upsert one searchable document (`target_id` → `text`).
    async fn upsert(&self, target_id: &str, text: &str) -> CoreResult<()>;

    /// Upsert many `(target_id, text)` entries at once.
    async fn upsert_batch(&self, entries: &[(String, String)]) -> CoreResult<()>;

    /// Upsert one searchable document under an explicit bound-code
    /// contributor key (`"code:<generation-id>"`): the entry is eligible only
    /// while that generation is published, and retiring the generation
    /// removes exactly these tokens (task_013 / task_014 seam).
    async fn upsert_code_target(
        &self,
        target_id: &str,
        text: &str,
        generation_id: &str,
    ) -> CoreResult<()>;

    /// Targeted warm-index retirement (task_014): removes every document the
    /// retired generation contributed and returns the removed count — the
    /// durable receipt. Targets holding OTHER contributor keys (an ordinary
    /// `doc` contribution, a surviving generation) keep their tokens.
    async fn retire_generation(&self, generation_id: &str) -> CoreResult<usize>;

    /// Removes every contributor document for one target id (task_014
    /// target-ID deletion).
    async fn delete_target(&self, target_id: &str) -> CoreResult<()>;
}
