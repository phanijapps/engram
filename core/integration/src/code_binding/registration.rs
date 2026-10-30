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
            .authorize(
                actor,
                scope,
                &registration.repo_id,
                CodeAuthorityAction::Register,
            )
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

impl CodeBindingCoordinator {
    /// ONE on-request refresh of the bound branch ref, resolved from the
    /// REGISTERED repository locator (review r6 finding 5): the shared
    /// behavior behind the Rust facade, the N-API binding, TypeScript, the
    /// stdio tools, and the one-shot CLI. The caller never supplies an OID —
    /// the observation is whatever the registered locator's local git
    /// resolves for the CONFIGURED branch ref, recorded separately from the
    /// indexed generation OID. Absent acquisition modes deny with honest
    /// typed states: no registration is a `BindingRequired`-family
    /// `InvalidRequest`; `configured-remote` is a typed `CapabilityUnsupported`
    /// (remote refresh is not wired on this transport).
    #[cfg(feature = "code-lanes")]
    pub async fn refresh_from_registration(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
    ) -> CoreResult<engram_knowledge::CodeRefObservation> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Inspect)
            .await?;
        let registration =
            self.store
                .get_registration(repo)
                .await?
                .ok_or_else(|| CoreError::InvalidRequest {
                    reason: "BindingRequired: no registered repository — register a locator first"
                        .into(),
                })?;
        match registration.acquisition {
            engram_knowledge::AcquisitionMode::LocalOnly => {}
            engram_knowledge::AcquisitionMode::ConfiguredRemote => {
                return Err(CoreError::CapabilityUnsupported {
                    capability: "code-remote-refresh".into(),
                    reason: "configured-remote acquisition is not wired on this transport; \
                             remote refresh requires the remote acquisition path"
                        .into(),
                });
            }
        }
        let binding = self.store.get_binding(scope, repo).await?.ok_or_else(|| {
            CoreError::InvalidRequest {
                reason: "BindingRequired: bind the repository before refreshing its ref".into(),
            }
        })?;
        let resolved = engram_ingest::git_source::resolve_local_branch(
            std::path::Path::new(&registration.locator_id),
            &binding.branch_ref,
        )?;
        self.record_ref_observation(actor, scope, repo, &resolved.oid)
            .await
    }
}
