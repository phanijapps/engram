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
#[async_trait]
pub trait LexicalFeed: Send + Sync {
    /// Upsert one searchable document (`target_id` → `text`).
    async fn upsert(&self, target_id: &str, text: &str) -> CoreResult<()>;

    /// Upsert many `(target_id, text)` entries at once.
    async fn upsert_batch(&self, entries: &[(String, String)]) -> CoreResult<()>;
}
