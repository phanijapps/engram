//! SQLite-backed [`LexicalFeed`] over the file-backed Tantivy [`LexicalIndex`]
//! that [`bootstrap_sqlite`] shares with unified recall's lexical lane.
//!
//! Engine-specific (names `LexicalIndex`, gated behind the `sqlite` feature).
//! The [`LexicalFeed`] trait (parent crate's [`lexical_feed`] module) stays
//! engine-neutral. One `Arc<LexicalIndex>` is shared between this feed (writes)
//! and [`LexicalRetrievalIndex`] (reads), so a `scan_repo` feed is immediately
//! visible to `search`/`recall`. Because the index is file-backed, those writes
//! also persist across process restarts — a fresh MCP process sees the lexical
//! lane without re-running `scan_repo`.
//!
//! [`bootstrap_sqlite`]: crate::sqlite::bootstrap_sqlite
//! [`lexical_feed`]: crate::lexical_feed
//! [`LexicalRetrievalIndex`]: engram_store_lexical::LexicalRetrievalIndex

use async_trait::async_trait;
use engram_runtime::{CoreError, CoreResult};
use engram_store_lexical::LexicalIndex;
use std::sync::Arc;

use crate::lexical_feed::LexicalFeed;

/// Feeds the shared file-backed Tantivy lexical index.
pub struct SqlLexicalFeed {
    index: Arc<LexicalIndex>,
    knowledge: Arc<engram_store_sqlite::SqlKnowledgeStore>,
}

impl SqlLexicalFeed {
    /// Wrap a shared lexical index (the same `Arc` used by the retrieval lane).
    pub fn new(
        index: Arc<LexicalIndex>,
        knowledge: Arc<engram_store_sqlite::SqlKnowledgeStore>,
    ) -> Self {
        Self { index, knowledge }
    }
}

#[async_trait]
impl LexicalFeed for SqlLexicalFeed {
    async fn upsert(&self, target_id: &str, _text: &str) -> CoreResult<()> {
        let text = self
            .knowledge
            .semantic_lexical_text(target_id)?
            .ok_or_else(|| CoreError::PolicyDenied {
                reason: "lexical target is not current eligible knowledge".into(),
            })?;
        self.index
            .upsert(target_id, &text)
            .map_err(|e| CoreError::Adapter {
                adapter: "lexical".to_string(),
                message: e.to_string(),
            })
    }

    async fn upsert_batch(&self, entries: &[(String, String)]) -> CoreResult<()> {
        let canonical = entries
            .iter()
            .map(|(id, _)| {
                self.knowledge
                    .semantic_lexical_text(id)?
                    .map(|text| (id.clone(), text))
                    .ok_or_else(|| CoreError::PolicyDenied {
                        reason: "lexical target is not current eligible knowledge".into(),
                    })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        self.index
            .upsert_batch(&canonical)
            .map_err(|e| CoreError::Adapter {
                adapter: "lexical".to_string(),
                message: e.to_string(),
            })
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use engram_domain::*;
    use engram_knowledge::KnowledgeRepository;
    use engram_store_sqlite::SqlKnowledgeStore;
    use futures::executor::block_on;
    use serde_json::json;

    #[test]
    fn feed_rejects_unowned_code_and_uses_canonical_content() {
        let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
        let index = Arc::new(LexicalIndex::new().unwrap());
        let feed = SqlLexicalFeed::new(index.clone(), store.clone());
        assert!(block_on(feed.upsert("snapshot", "poison source")).is_err());
        let entity:KnowledgeEntity=serde_json::from_value(json!({"id":"eligible","kind":"function","name":"getUser","scope":{"tenant":"t"},"provenance":{"source":"test","actor":{"id":"a","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"},"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
        block_on(store.put_entity(entity)).unwrap();
        block_on(feed.upsert("eligible", "arbitrary_snapshot_poison")).unwrap();
        assert!(
            index
                .search("arbitrary_snapshot_poison", 20)
                .unwrap()
                .is_empty()
        );
        assert_eq!(index.search("getUser", 20).unwrap()[0].0, "eligible");
        assert!(
            block_on(feed.upsert_batch(&[
                ("eligible".into(), "getUser".into()),
                ("snapshot".into(), "poison".into())
            ]))
            .is_err()
        );
    }
}
