//! In-memory belief repository implementation.
//!
//! Beliefs and contradictions are stored separately from memories, knowledge,
//! and hierarchy records. This preserves the contract distinction between
//! source-grounded evidence and derived stance records.

use async_trait::async_trait;
use engram_core::{BeliefRepository, CoreError, CoreResult};
use engram_domain::*;

use crate::{scope::scope_allows, service::InMemoryMemoryService};

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

    async fn get_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
    ) -> CoreResult<Option<Contradiction>> {
        let state = self.lock_state()?;
        let contradiction = state
            .contradictions
            .get(id.as_str())
            .filter(|contradiction| scope_allows(&contradiction.scope, scope));
        Ok(contradiction.cloned())
    }

    async fn resolve_contradiction(
        &self,
        id: &ContradictionId,
        scope: &Scope,
        resolution: ContradictionResolution,
    ) -> CoreResult<Contradiction> {
        let mut state = self.lock_state()?;
        let contradiction = state
            .contradictions
            .get_mut(id.as_str())
            .filter(|contradiction| scope_allows(&contradiction.scope, scope))
            .ok_or_else(|| CoreError::NotFound {
                target_type: "contradiction",
                target_id: id.to_string(),
            })?;

        contradiction.status = status_for_resolution(&resolution.kind);
        contradiction.updated_at = Some(resolution.resolved_at);
        contradiction.resolution = Some(resolution);

        Ok(contradiction.clone())
    }
}

fn status_for_resolution(kind: &ContradictionResolutionKind) -> ContradictionStatus {
    match kind {
        ContradictionResolutionKind::ManualIgnore => ContradictionStatus::Ignored,
        ContradictionResolutionKind::NeedsMoreEvidence => ContradictionStatus::Open,
        ContradictionResolutionKind::TargetWon
        | ContradictionResolutionKind::Compatible
        | ContradictionResolutionKind::Merged
        | ContradictionResolutionKind::Retracted => ContradictionStatus::Resolved,
    }
}
