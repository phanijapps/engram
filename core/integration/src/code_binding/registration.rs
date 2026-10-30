use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, CodeBinding, RepoId, Scope};
use engram_knowledge::RepositoryRegistration;
use engram_runtime::{CoreError, CoreResult};

impl CodeBindingCoordinator {
    /// Registers an acquisition locator only after the same shared authority
    /// seam that protects binding, publication, and inspection authorizes it.
    pub async fn register(
        &self,
        actor: &Actor,
        scope: &Scope,
        registration: RepositoryRegistration,
    ) -> CoreResult<RepositoryRegistration> {
        self.authorizer
            .authorize(actor, scope, &registration.repo_id, CodeAuthorityAction::Register)
            .await?;
        self.store.register(registration).await
    }

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
