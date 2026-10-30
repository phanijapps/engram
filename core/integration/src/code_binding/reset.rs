//! Authorization boundary for immutable exact reset proposals and journals.
//!
//! Previewing is read-only.
//! Applying a journal is intentionally owned by TASK-024.
use super::{CodeAuthorityAction, CodeBindingCoordinator};
use engram_domain::{Actor, CodeResetJournal, CodeResetPreview, RepoId, Scope};
use engram_runtime::{CoreError, CoreResult};

impl CodeBindingCoordinator {
    /// Computes an exact reset proposal only after reset authority is proven.
    pub async fn preview_reset(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
    ) -> CoreResult<CodeResetPreview> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Reset)
            .await?;
        self.store.preview_reset(scope, repo).await
    }

    /// Applies or resumes an immutable reset journal after reset authorization.
    /// The durable cell revalidates targets and leases and returns checkpoints.
    /// It never claims rollback of a successfully completed external store.
    pub async fn apply_reset(
        &self,
        actor: &Actor,
        journal: CodeResetJournal,
    ) -> CoreResult<CodeResetJournal> {
        let scope = journal.preview.binding.scope.clone();
        let repo = journal.preview.binding.repo_id.clone();
        self.authorizer
            .authorize(actor, &scope, &repo, CodeAuthorityAction::Reset)
            .await?;
        self.store.apply_reset(journal).await
    }

    /// Creates an immutable journal only through the reset-authorized seam.
    /// The cell replays its current preview before persistence, so a stale
    /// preview, substituted fingerprint, or subset authorization is denied.
    pub async fn journal_reset(
        &self,
        actor: &Actor,
        mut journal: CodeResetJournal,
        decision: String,
    ) -> CoreResult<CodeResetJournal> {
        let scope = journal.preview.binding.scope.clone();
        let repo = journal.preview.binding.repo_id.clone();
        self.authorizer
            .authorize(actor, &scope, &repo, CodeAuthorityAction::Reset)
            .await?;
        if decision.trim().is_empty() {
            return Err(CoreError::PolicyDenied {
                reason: "ResetApprovalRequired: an authorizing decision is required".into(),
            });
        }
        if !journal
            .authorizing_decisions
            .iter()
            .any(|item| item == &decision)
        {
            journal.authorizing_decisions.push(decision);
        }
        journal.validate().map_err(|_| CoreError::InvalidRequest {
            reason: "InvalidResetPreview: journal does not bind the complete preview".into(),
        })?;
        self.store.journal_reset(journal).await
    }
}
