//! Distinct-target collection (corrective r4, canonical 5).
//!
//! A [`tantivy::Collector`] that keeps the BEST matching document score per
//! target id. One target occupies exactly one slot in the bounded page
//! regardless of how many (target, contributor) documents matched, so the
//! target budget is a true TARGET budget — a flood of duplicate contributor
//! documents for one allowed target cannot crowd out another allowed target,
//! and no fixed document overfetch is involved.
//!
//! Retained-state bounds (A5 instrumentation): every segment collector trims
//! after each insert, so its retained map is at most a TRANSIENT K+1 entries
//! (the entry being inserted) and at most K after trim; `merge_fruits`
//! receives at most K entries per segment, so the merge input volume is
//! O(segments × K) — never one global unbounded map. There is deliberately NO
//! hard scan cap: every matching document is scan work. Any future cap MUST
//! return an explicit error rather than silently returning a partial page.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tantivy::collector::Collector;
use tantivy::schema::Value;
use tantivy::{DocId, Score, SegmentOrdinal, SegmentReader};

/// Per-target best score (the collector's fruit).
pub(crate) type TargetBestScores = HashMap<String, Score>;

/// Retained-state instrumentation for the distinct-target collector (A5):
/// records what the collector actually retained so tests can document the
/// transient K+1 per-segment bound and the O(segments × K) merge input
/// volume. All counters are relaxed atomics — they observe bounds, they do
/// not participate in collection decisions.
#[derive(Default)]
pub struct DistinctTargetCounters {
    /// Largest per-segment retained map observed immediately AFTER a single
    /// insert, BEFORE the per-insert trim (the documented transient K+1).
    retained_max: AtomicUsize,
    /// Total (target, score) entries across all segment fruits entering
    /// `merge_fruits` (each segment contributes at most K, so this is the
    /// O(segments × K) merge storage evidence).
    merge_documents: AtomicUsize,
    /// Number of segment collectors created (the "segments" factor).
    segments: AtomicUsize,
}

impl DistinctTargetCounters {
    /// Largest post-insert pre-trim per-segment retained size (≤ K+1).
    pub fn retained_max(&self) -> usize {
        self.retained_max.load(Ordering::Relaxed)
    }
    /// Total entries entering the fruit merge (≤ segments × K).
    pub fn merge_documents(&self) -> usize {
        self.merge_documents.load(Ordering::Relaxed)
    }
    /// Segment collectors created for the instrumented search.
    pub fn segments(&self) -> usize {
        self.segments.load(Ordering::Relaxed)
    }
}

/// Collects the best score per `target_field` value over the matching
/// documents, with no document-count limit semantics.
pub(crate) struct DistinctTargetCollector {
    #[allow(dead_code)] // reserved for bounded-heap streaming in later wiring
    limit: usize,
    target_field: tantivy::schema::Field,
    counters: Option<Arc<DistinctTargetCounters>>,
}

impl DistinctTargetCollector {
    pub(crate) fn new(limit: usize, target_field: tantivy::schema::Field) -> Self {
        Self {
            limit,
            target_field,
            counters: None,
        }
    }

    /// Attaches retained-state instrumentation (A5); the counters observe the
    /// documented bounds without changing collection behavior.
    pub(crate) fn with_counters(mut self, counters: Arc<DistinctTargetCounters>) -> Self {
        self.counters = Some(counters);
        self
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
        if let Some(counters) = &self.counters {
            counters.segments.fetch_add(1, Ordering::Relaxed);
        }
        Ok(DistinctTargetSegmentCollector {
            store,
            target_field: self.target_field,
            limit: self.limit,
            best: HashMap::new(),
            counters: self.counters.clone(),
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
            if let Some(counters) = &self.counters {
                counters
                    .merge_documents
                    .fetch_add(fruit.len(), Ordering::Relaxed);
            }
            for (target, score) in fruit {
                let entry = merged.entry(target).or_insert(score);
                if score > *entry {
                    *entry = score;
                }
                trim_to_limit(&mut merged, self.limit);
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
        // A5 instrumentation: observe the retained size AFTER the insert and
        // BEFORE the trim — this is where the documented transient K+1 bound
        // is visible.
        if let Some(counters) = &self.counters {
            counters
                .retained_max
                .fetch_max(self.best.len(), Ordering::Relaxed);
        }
        trim_to_limit(&mut self.best, self.limit);
    }

    fn harvest(self) -> Self::Fruit {
        self.best
    }
}

pub(crate) struct DistinctTargetSegmentCollector {
    store: tantivy::store::StoreReader,
    target_field: tantivy::schema::Field,
    limit: usize,
    best: TargetBestScores,
    counters: Option<Arc<DistinctTargetCounters>>,
}

/// Retain only the best distinct targets as documents arrive. Tie-breaking is
/// deterministic, so every collector stage has a bounded target map rather
/// than collecting every matching contributor before truncation.
fn trim_to_limit(scores: &mut TargetBestScores, limit: usize) {
    while scores.len() > limit {
        let worst = scores
            .iter()
            .min_by(|(left_id, left_score), (right_id, right_score)| {
                left_score
                    .partial_cmp(right_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| right_id.cmp(left_id))
            })
            .map(|(id, _)| id.clone());
        if let Some(worst) = worst {
            scores.remove(&worst);
        } else {
            break;
        }
    }
}
