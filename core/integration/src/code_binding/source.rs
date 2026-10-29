//! Authorized exact retained-source coordination (corrective r2, findings
//! 2/3/7).
//!
//! `code_fetch_exact*` answers ONLY for a repository this scope durably
//! bound, ONLY for its published active generation or a generation covered by
//! a live read lease (an explicitly authorized pinned historical read), and
//! ONLY with bytes an ADAPTER read from retained custody and verified against
//! the durable indexed-source descriptor. The caller is authenticated through
//! the same `CodeBindingAuthorizer` as every other code operation, and the
//! resolution runs inside the coordinator's consistent-read window so a
//! publication race retries once or surfaces `GenerationChanged` instead of
//! returning an unchecked view. Nothing in this chain consults a checkout.

use engram_domain::{Actor, RepoId, Scope};
use engram_knowledge::{
    CodeReadLease, CoreError, CoreResult, ExactSourceView, RetainedSourceReader,
};
use std::sync::Arc;
use std::time::Duration;

use super::CodeBindingCoordinator;

/// How long an exact-source consistent-read window pins the view.
const EXACT_READ_WINDOW: Duration = Duration::from_secs(30);

fn binding_required() -> CoreError {
    CoreError::InvalidRequest {
        reason: "BindingRequired: no bound repository for exact source".into(),
    }
}

fn generation_denied(generation_id: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "ExactSourceUnavailable: generation {generation_id} is not published for this \
             repository and no authorized read lease covers it"
        ),
    }
}

impl CodeBindingCoordinator {
    /// Serves the EXACT committed bytes of `path` for a bound repository.
    ///
    /// Authority chain (findings 2/3):
    /// 1. the CALLER is authorized (`Inspect`) for this scope + repository —
    ///    a denying authorizer denies even for fully populated scopes;
    /// 2. the binding must exist for THIS scope + repository;
    /// 3. `generation_id` (or the binding's active generation when `None`)
    ///    must be the PUBLISHED active generation, or be covered by a LIVE
    ///    read lease (`retained_generations` — the explicitly authorized
    ///    pinned historical read); staged, retired-without-lease, foreign and
    ///    unbound generations deny BEFORE any source byte is opened;
    /// 4. binding + generation + descriptor resolution runs inside a
    ///    consistent-read window (retry once on a publication race, else
    ///    `GenerationChanged`);
    /// 5. the ADAPTER reader validates custody and the durable descriptor
    ///    digest before bytes are declared exact (finding 7).
    #[allow(clippy::double_must_use)]
    pub async fn exact_source(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: Option<&str>,
        historical_lease: Option<&CodeReadLease>,
        path: &str,
        span: Option<std::ops::Range<usize>>,
        retained: &Arc<dyn RetainedSourceReader>,
    ) -> CoreResult<ExactSourceView> {
        let repo = RepoId(repo_id.to_owned());
        // (1) Caller authority at the shared seam.
        self.authorize(actor, scope, &repo).await?;
        // (2)-(4) Resolve under a validated consistent-read window: the
        // closure observes the CURRENT published state through the lease it
        // pinned; a concurrent publication retries once or fails with
        // GenerationChanged rather than returning an unchecked view.
        let store = self.store();
        let repo_read = repo.clone();
        let historical_lease = historical_lease.cloned();
        self.consistent_read(actor, scope, &repo, EXACT_READ_WINDOW, move |eligibility| {
            let requested = generation_id.map(str::to_owned);
            let historical_lease = historical_lease.clone();
            let span = span.clone();
            let store = Arc::clone(&store);
            let repo = repo_read.clone();
            let retained = Arc::clone(retained);
            async move {
                let binding = store
                    .get_binding(scope, &repo)
                    .await?
                    .ok_or_else(binding_required)?;
                let active = binding.active_generation.clone();
                let resolved = match &requested {
                    Some(requested) => requested.clone(),
                    None => active.clone().ok_or_else(|| CoreError::InvalidRequest {
                        reason: "ExactSourceUnavailable: the bound repository has no \
                                     published generation"
                            .into(),
                    })?,
                };
                if Some(&resolved) != active.as_ref() {
                    // Historical authority is caller-presented and durable.
                    // Never authorize from the union of unrelated live leases:
                    // another caller's lease cannot turn a generation id into
                    // read authority.
                    let lease = historical_lease
                        .as_ref()
                        .ok_or_else(|| generation_denied(&resolved))?;
                    if !store
                        .validate_historical_read_lease(
                            lease,
                            scope,
                            repo_id,
                            &resolved,
                            chrono::Utc::now(),
                        )
                        .await?
                    {
                        return Err(generation_denied(&resolved));
                    }
                } else if !eligibility.generations.iter().any(|id| id == &resolved) {
                    // Defensive: the active generation must be in the pinned
                    // window's published set.
                    return Err(generation_denied(&resolved));
                }
                let generation = store
                    .get_generation(scope, &repo, &resolved)
                    .await?
                    .ok_or_else(|| generation_denied(&resolved))?;
                let descriptor = store
                    .get_source_descriptor(scope, &repo, &resolved, path)
                    .await?
                    .ok_or_else(|| CoreError::InvalidRequest {
                        reason: format!(
                            "ExactSourceUnavailable: no durable indexed-source \
                                 descriptor for the requested path"
                        ),
                    })?;
                if descriptor.commit_oid != generation.commit_oid {
                    return Err(generation_denied(&resolved));
                }
                // (5) Adapter-owned custody + digest-validated read.
                let view = retained
                    .read_span(
                        &generation.commit_oid,
                        path,
                        span,
                        &descriptor.content_digest,
                    )
                    .map_err(|error| match error {
                        CoreError::InvalidRequest { reason }
                            if reason.contains("ExactSourceUnavailable") =>
                        {
                            CoreError::InvalidRequest { reason }
                        }
                        other => other,
                    })?;
                // The read window extends PAST the byte I/O: a historical
                // lease released or expired DURING the read denies the
                // answer — the token must still be live when the bytes are
                // handed back (release/expiry-during-read barrier).
                if let Some(lease) = historical_lease.as_ref()
                    && !store
                        .validate_historical_read_lease(
                            lease,
                            scope,
                            repo_id,
                            &resolved,
                            chrono::Utc::now(),
                        )
                        .await?
                {
                    return Err(generation_denied(&resolved));
                }
                Ok(view)
            }
        })
        .await
    }
}

impl crate::EngramProvider {
    /// Serves the EXACT committed bytes of `path` for a bound repository's
    /// published generation through the provider's authorized retained-source
    /// handle (see [`CodeBindingCoordinator::exact_source`]).
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(actor, scope, repo_id, None, None, path, span, &retained)
            .await
    }

    /// [`Self::code_fetch_exact`] pinned to one indexed generation — the
    /// revision a specific chunk set was stamped with. The generation must be
    /// the published active generation or live-lease-covered; bytes still
    /// come from the retained snapshot of THAT commit, never from a checkout.
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact_at(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        self.code_fetch_exact_at_with_lease(actor, scope, repo_id, generation_id, None, path, span)
            .await
    }

    /// Fetches a historical generation only when this caller presents the
    /// durable canonical lease that pins it.  The active generation needs no
    /// historical token, while a generation id alone is deliberately denied.
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact_at_with_lease(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: &str,
        historical_lease: Option<&CodeReadLease>,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(
                actor,
                scope,
                repo_id,
                Some(generation_id),
                historical_lease,
                path,
                span,
                &retained,
            )
            .await
    }
}
