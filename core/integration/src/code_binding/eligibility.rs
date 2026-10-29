use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, RepoId, Scope};
use engram_knowledge::{CodeEligibility, CodeReadLease};
use engram_runtime::{CoreError, CoreResult};
use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }
}
impl CodeBindingCoordinator {
    /// Resolve the published-generation eligibility view for retrieval lanes
    /// and graph composition (task_013 / task_015, engine-neutral): the
    /// scope's published generations + repositories intersected with the
    /// request's `code_index_keys` narrowing. Requesting an unbound repository
    /// selects nothing; the wildcard keeps exactly the published set.
    pub async fn eligibility_view(
        &self,
        scope: &Scope,
        requested_keys: Option<&[String]>,
    ) -> CoreResult<engram_retrieval::BoundCodeView> {
        // Corrective r2 (findings 4/9): the view is built from durable
        // published bindings (repository + branch + generation verified
        // together) and the durable target lineage.
        let eligibility = self.store.eligibility(scope).await?;
        let mut bindings = Vec::with_capacity(eligibility.repository_ids.len());
        for repo in &eligibility.repository_ids {
            let repo_id = RepoId(repo.clone());
            let Some(binding) = self.store.get_binding(scope, &repo_id).await? else {
                continue;
            };
            if let Some(generation) = binding.active_generation {
                bindings.push(engram_retrieval::BoundBinding {
                    repo_id: repo.clone(),
                    branch_ref: binding.branch_ref,
                    generation_id: generation,
                });
            }
        }
        let lineage = self
            .store
            .target_lineage(scope)
            .await?
            .into_iter()
            // Retirement is an additive ownership fact: keep it available to
            // callers that classify durable code ownership, but never admit a
            // retired target into a published-generation eligibility view.
            .filter(|entry| !entry.retired)
            .map(|entry| engram_retrieval::DurableLineage {
                record_id: entry.record_id,
                repo_id: entry.repo_id,
                generation_id: entry.generation_id,
            })
            .collect();
        let view = engram_retrieval::BoundCodeView::from_eligibility(
            &engram_retrieval::BoundCodeEligibility {
                epoch: eligibility.epoch,
                bindings,
                lineage,
            },
        );
        Ok(view.with_requested_keys(requested_keys))
    }

    /// Pin all published repositories only after authorizing every repository in the set.
    pub async fn pin_read(
        &self,
        actor: &Actor,
        scope: &Scope,
        authority_repo: &RepoId,
        duration: Duration,
    ) -> CoreResult<CodeReadLease> {
        self.authorizer
            .authorize(actor, scope, authority_repo, CodeAuthorityAction::Inspect)
            .await?;
        let eligibility = self.store.eligibility(scope).await?;
        for repo in &eligibility.repository_ids {
            self.authorizer
                .authorize(
                    actor,
                    scope,
                    &RepoId(repo.clone()),
                    CodeAuthorityAction::Inspect,
                )
                .await?;
        }
        let now = self.clock.now();
        let delta = chrono::Duration::from_std(duration).map_err(|_| changed())?;
        let expires_at = now
            .checked_add_signed(delta)
            .filter(|expiry| expiry > &now)
            .ok_or_else(changed)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = format!(
            "read-{}-{}",
            now.timestamp_nanos_opt().ok_or_else(changed)?,
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        self.store
            .acquire_read_lease(
                CodeReadLease {
                    id,
                    eligibility,
                    fencing_token: 1,
                    expires_at,
                },
                now,
            )
            .await
    }
    pub async fn renew_read(
        &self,
        actor: &Actor,
        authority_repo: &RepoId,
        lease: &CodeReadLease,
        duration: Duration,
    ) -> CoreResult<CodeReadLease> {
        self.authorizer
            .authorize(
                actor,
                &lease.eligibility.scope,
                authority_repo,
                CodeAuthorityAction::Inspect,
            )
            .await?;
        for repo in &lease.eligibility.repository_ids {
            self.authorizer
                .authorize(
                    actor,
                    &lease.eligibility.scope,
                    &RepoId(repo.clone()),
                    CodeAuthorityAction::Inspect,
                )
                .await?;
        }
        let now = self.clock.now();
        let delta = chrono::Duration::from_std(duration).map_err(|_| changed())?;
        let expires_at = now.checked_add_signed(delta).ok_or_else(changed)?;
        self.store.renew_read_lease(lease, now, expires_at).await
    }
    pub async fn release_read(
        &self,
        actor: &Actor,
        authority_repo: &RepoId,
        lease: &CodeReadLease,
    ) -> CoreResult<()> {
        self.authorizer
            .authorize(
                actor,
                &lease.eligibility.scope,
                authority_repo,
                CodeAuthorityAction::Inspect,
            )
            .await?;
        for repo in &lease.eligibility.repository_ids {
            self.authorizer
                .authorize(
                    actor,
                    &lease.eligibility.scope,
                    &RepoId(repo.clone()),
                    CodeAuthorityAction::Inspect,
                )
                .await?;
        }
        self.store.release_read_lease(lease).await
    }

    /// Owned maintenance mutation (corrective r4, canonical 3a): authorizes
    /// the caller for every involved repository, then executes the store's
    /// EPOCH-CAS fenced lineage retirement. The retirement is the owned
    /// first side effect and linearizes atomically with the epoch check in
    /// one store transaction — a publication that committed first denies
    /// with `GenerationChanged` and NOTHING is retired.
    pub async fn retire_maintenance_targets(
        &self,
        actor: &Actor,
        scope: &Scope,
        expected_binding_epochs: &[(RepoId, u64)],
        targets: &[String],
    ) -> CoreResult<()> {
        for (repo, _) in expected_binding_epochs {
            self.authorize(actor, scope, repo).await?;
        }
        self.store()
            .retire_lineage_targets(scope, expected_binding_epochs, targets)
            .await
    }

    /// Retry one changed generation set; never return a response from a stale epoch or expired lease.
    pub async fn consistent_read<T, F, Fut>(
        &self,
        actor: &Actor,
        scope: &Scope,
        authority_repo: &RepoId,
        duration: Duration,
        mut read: F,
    ) -> CoreResult<T>
    where
        F: FnMut(CodeEligibility) -> Fut,
        Fut: Future<Output = CoreResult<T>>,
    {
        for _ in 0..2 {
            let lease = match self.pin_read(actor, scope, authority_repo, duration).await {
                Ok(lease) => lease,
                Err(CoreError::Conflict { reason }) if reason == "GenerationChanged" => continue,
                Err(error) => return Err(error),
            };
            let output = read(lease.eligibility.clone()).await;
            let current = self.store.eligibility(scope).await;
            let released = self.store.release_read_lease(&lease).await;
            let output = output?;
            let current = current?;
            released?;
            if current == lease.eligibility && self.clock.now() < lease.expires_at {
                return Ok(output);
            }
        }
        Err(changed())
    }
}
