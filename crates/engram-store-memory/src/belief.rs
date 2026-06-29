//! In-memory belief repository implementation.
//!
//! Beliefs and contradictions are stored separately from memories, knowledge,
//! and hierarchy records. This preserves the contract distinction between
//! source-grounded evidence and derived stance records.

use async_trait::async_trait;
use engram_core::{BeliefRepository, CoreResult};
use engram_domain::*;

use crate::service::InMemoryMemoryService;

#[async_trait]
impl BeliefRepository for InMemoryMemoryService {
    async fn put_belief(&self, belief: Belief) -> CoreResult<Belief> {
        let mut state = self.lock_state()?;
        state.beliefs.insert(belief.id.to_string(), belief.clone());
        Ok(belief)
    }

    async fn put_contradiction(&self, contradiction: Contradiction) -> CoreResult<Contradiction> {
        let mut state = self.lock_state()?;
        state
            .contradictions
            .insert(contradiction.id.to_string(), contradiction.clone());
        Ok(contradiction)
    }
}
