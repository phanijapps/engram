//! Mandatory pre-budget eligibility contracts for code-capable lanes.
use async_trait::async_trait;
use engram_domain::{Id, Scope};
use engram_runtime::CoreResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeEligibilityToken {
    pub scope: Scope,
    pub epoch: u64,
    pub generation_ids: Vec<String>,
    pub allowed_targets: Vec<Id>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeLaneState {
    Ready,
    Unavailable { reason: String },
    FailedClosed { reason: String },
}

#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeCandidateLane: Send + Sync {
    /// Implementations MUST intersect allowed targets before applying limit or ranking.
    async fn search_eligible(
        &self,
        token: &CodeEligibilityToken,
        query: &str,
        limit: usize,
    ) -> CoreResult<Vec<(Id, f32)>>;
    async fn lane_state(&self, scope: &Scope) -> CoreResult<CodeLaneState>;
}

pub fn authorize_before_selection(
    token: Option<&CodeEligibilityToken>,
    limit: usize,
) -> CoreResult<&CodeEligibilityToken> {
    let token = token.ok_or_else(|| engram_runtime::CoreError::InvalidRequest {
        reason: "code eligibility is required before candidate selection".into(),
    })?;
    if limit == 0 || token.allowed_targets.is_empty() {
        return Err(engram_runtime::CoreError::InvalidRequest {
            reason: "code eligibility has no selectable targets or budget".into(),
        });
    }
    Ok(token)
}
