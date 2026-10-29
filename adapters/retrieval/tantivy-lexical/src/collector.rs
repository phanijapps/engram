//! Distinct-target collection (corrective r4, canonical 5).
//!
//! A [`tantivy::Collector`] that keeps the BEST matching document score per
//! target id. One target occupies exactly one slot in the bounded page
//! regardless of how many (target, contributor) documents matched, so the
//! target budget is a true TARGET budget — a flood of duplicate contributor
//! documents for one allowed target cannot crowd out another allowed target,
//! and no fixed document overfetch is involved.

use std::collections::HashMap;

use tantivy::collector::Collector;
use tantivy::schema::Value;
use tantivy::{DocId, Score, SegmentOrdinal, SegmentReader};

/// Per-target best score (the collector's fruit).
pub(crate) type TargetBestScores = HashMap<String, Score>;

/// Collects the best score per `target_field` value over the matching
/// documents, with no document-count limit semantics.
pub(crate) struct DistinctTargetCollector {
    #[allow(dead_code)] // reserved for bounded-heap streaming in later wiring
    limit: usize,
    target_field: tantivy::schema::Field,
}

impl DistinctTargetCollector {
    pub(crate) fn new(limit: usize, target_field: tantivy::schema::Field) -> Self {
        Self {
            limit,
            target_field,
        }
    }
}

impl Collector for DistinctTargetCollector {
    type Fruit = TargetBestScores;
    type Child = DistinctTargetSegmentCollector;

    fn for_segment(
        &self,
        _segment_local_id: SegmentOrdinal,
        segment: &SegmentReader,
    ) -> tantivy::Result<Self::Child> {
        let store = segment.get_store_reader(100)?;
        Ok(DistinctTargetSegmentCollector {
            store,
            target_field: self.target_field,
            best: HashMap::new(),
        })
    }

    fn requires_scoring(&self) -> bool {
        true
    }

    fn merge_fruits(
        &self,
        segment_fruits: Vec<<Self::Child as tantivy::collector::SegmentCollector>::Fruit>,
    ) -> tantivy::Result<Self::Fruit> {
        let mut merged: TargetBestScores = HashMap::new();
        for fruit in segment_fruits {
            for (target, score) in fruit {
                let entry = merged.entry(target).or_insert(score);
                if score > *entry {
                    *entry = score;
                }
            }
        }
        Ok(merged)
    }
}

impl tantivy::collector::SegmentCollector for DistinctTargetSegmentCollector {
    type Fruit = TargetBestScores;

    fn collect(&mut self, doc: DocId, score: Score) {
        let Ok(stored) = self.store.get::<tantivy::TantivyDocument>(doc) else {
            return;
        };
        let Some(target) = stored
            .get_first(self.target_field)
            .and_then(|value| value.as_str())
            .map(str::to_owned)
        else {
            return;
        };
        let entry = self.best.entry(target).or_insert(score);
        if score > *entry {
            *entry = score;
        }
    }

    fn harvest(self) -> Self::Fruit {
        self.best
    }
}

pub(crate) struct DistinctTargetSegmentCollector {
    store: tantivy::store::StoreReader,
    #[allow(dead_code)]
    target_field: tantivy::schema::Field,
    best: TargetBestScores,
}
