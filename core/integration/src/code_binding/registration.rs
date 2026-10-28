use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, CodeBinding, RepoId, Scope};
use engram_runtime::{CoreError, CoreResult};

impl CodeBindingCoordinator {
    pub async fn bind(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        branch_ref: &str,
    ) -> CoreResult<CodeBinding> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Bind)
            .await?;
        let binding = CodeBinding {
            scope: scope.clone(),
            repo_id: repo.clone(),
            branch_ref: branch_ref.into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        };
        binding.validate().map_err(|_| CoreError::InvalidRequest {
            reason: "invalid repository binding".into(),
        })?;
        self.store.bind(binding).await
    }
}
