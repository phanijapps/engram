//! SQLite-backed [`LexicalFeed`] over the in-RAM Tantivy [`LexicalIndex`] that
//! [`bootstrap_sqlite`] shares with unified recall's lexical lane.
//!
//! Engine-specific (names `LexicalIndex`, gated behind the `sqlite` feature).
//! The [`LexicalFeed`] trait (parent crate's [`lexical_feed`] module) stays
//! engine-neutral. One `Arc<LexicalIndex>` is shared between this feed (writes)
//! and [`LexicalRetrievalIndex`] (reads), so a `scan_repo` feed is immediately
//! visible to `search`/`recall`.
//!
//! [`bootstrap_sqlite`]: crate::sqlite::bootstrap_sqlite
//! [`lexical_feed`]: crate::lexical_feed
//! [`LexicalRetrievalIndex`]: engram_store_lexical::LexicalRetrievalIndex

use async_trait::async_trait;
use engram_runtime::{CoreError, CoreResult};
use engram_store_lexical::LexicalIndex;
use std::sync::Arc;

use crate::lexical_feed::LexicalFeed;

/// Feeds the shared in-RAM Tantivy lexical index.
pub struct SqlLexicalFeed {
    index: Arc<LexicalIndex>,
}

impl SqlLexicalFeed {
    /// Wrap a shared lexical index (the same `Arc` used by the retrieval lane).
    pub fn new(index: Arc<LexicalIndex>) -> Self {
        Self { index }
    }
}

#[async_trait]
impl LexicalFeed for SqlLexicalFeed {
    async fn upsert(&self, target_id: &str, text: &str) -> CoreResult<()> {
        self.index
            .upsert(target_id, text)
            .map_err(|e| CoreError::Adapter {
                adapter: "lexical".to_string(),
                message: e.to_string(),
            })
    }

    async fn upsert_batch(&self, entries: &[(String, String)]) -> CoreResult<()> {
        self.index
            .upsert_batch(entries)
            .map_err(|e| CoreError::Adapter {
                adapter: "lexical".to_string(),
                message: e.to_string(),
            })
    }
}
