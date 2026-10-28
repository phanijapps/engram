use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, CodeAttempt, CodeBinding, CodeGeneration, RepoId, Scope};
use engram_runtime::{CoreError, CoreResult};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBindingStatus {
    pub binding: CodeBinding,
    pub attempt: Option<(CodeGeneration, CodeAttempt)>,
    pub code_epoch: u64,
}
impl CodeBindingCoordinator {
    pub async fn status(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        attempt_id: Option<&str>,
    ) -> CoreResult<CodeBindingStatus> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Inspect)
            .await?;
        for _ in 0..2 {
            let before = self.store.eligibility(scope).await?;
            let binding = self.store.get_binding(scope, repo).await?.ok_or_else(|| {
                CoreError::InvalidRequest {
                    reason: "BindingRequired".into(),
                }
            })?;
            let attempt = match attempt_id {
                Some(id) => self.store.get_attempt(scope, repo, id).await?,
                None => None,
            };
            if self.store.eligibility(scope).await? == before {
                return Ok(CodeBindingStatus {
                    binding,
                    attempt,
                    code_epoch: before.epoch,
                });
            }
        }
        Err(CoreError::Conflict {
            reason: "GenerationChanged".into(),
        })
    }
}
