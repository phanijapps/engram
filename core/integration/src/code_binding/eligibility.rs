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
