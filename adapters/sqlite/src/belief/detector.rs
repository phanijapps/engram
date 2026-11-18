//! SQLite clock/trait compatibility for canonical advisory contradiction detection.
use crate::belief::service::SqlBeliefStore;
use async_trait::async_trait;
use engram_belief::ContradictionDetector;
use engram_domain::{Belief, Contradiction};
use engram_runtime::CoreResult;
#[async_trait]
impl ContradictionDetector for SqlBeliefStore {
    async fn detect_contradictions(&self, beliefs: &[Belief]) -> CoreResult<Vec<Contradiction>> {
        engram_belief::contradiction::detect_belief_contradictions(beliefs, chrono::Utc::now())
    }
}
