//! Authorization and complete orchestration for immutable exact reset journals.
//!
//! The durable store primitive retires fenced lineage and preserves its R1a
//! journal/checkpoint contract.
//! This coordinator owns the later BestEffort lane work because lexical,
//! vector, associative, community, and retained-source implementations are
//! integration seams rather than knowledge-cell concerns.
use super::{CodeAuthorityAction, CodeBindingCoordinator};
use async_trait::async_trait;
use engram_domain::{
    Actor, CodeResetJournal, CodeResetPreview, CodeResetTarget, PreservationRecord, RepoId, Scope,
};
use engram_runtime::{CoreError, CoreResult};
use std::collections::BTreeSet;
use std::sync::Arc;

/// One integration lane participating in reset cleanup and rebuild.
/// Implementors compose the established r5 lane seams; they do not duplicate
/// cleanup in the knowledge store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetLane {
    Lexical,
    Vector,
    Associative,
    Community,
    Source,
}
impl ResetLane {
    const ALL: [Self; 5] = [
        Self::Lexical,
        Self::Vector,
        Self::Associative,
        Self::Community,
        Self::Source,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Vector => "vector",
            Self::Associative => "associative",
            Self::Community => "community",
            Self::Source => "source",
        }
    }
}

/// Bundle injected by a recipe after it composes its lexical targeted-delete,
/// vector cleanup, associative/community recompute, and retained-snapshot seams.
#[async_trait]
pub trait ResetLaneLifecycle: Send + Sync {
    async fn cleanup_and_rebuild(
        &self,
        lane: ResetLane,
        scope: &Scope,
        retired: &[CodeResetTarget],
        preserved: &[PreservationRecord],
    ) -> CoreResult<()>;
}

/// Compatibility construction is intentionally inert; a recipe that exposes
/// reset MUST inject real lane composition with `with_reset_lanes`.
pub(crate) struct NoopResetLaneLifecycle;
#[async_trait]
impl ResetLaneLifecycle for NoopResetLaneLifecycle {
    async fn cleanup_and_rebuild(
        &self,
        _lane: ResetLane,
        _scope: &Scope,
        _retired: &[CodeResetTarget],
        _preserved: &[PreservationRecord],
    ) -> CoreResult<()> {
        Ok(())
    }
}

/// The real recipe bundle composes the existing lane seams.
/// Optional handles are absent only where a recipe does not ship that lane.
pub struct RecipeResetLaneLifecycle {
    lexical: Option<Arc<dyn crate::LexicalFeed>>,
    vector: Option<Arc<dyn engram_retrieval::VectorIndex>>,
    graph_cache: Arc<dyn engram_retrieval::GraphCache>,
}

impl RecipeResetLaneLifecycle {
    pub fn new(
        lexical: Option<Arc<dyn crate::LexicalFeed>>,
        vector: Option<Arc<dyn engram_retrieval::VectorIndex>>,
        graph_cache: Arc<dyn engram_retrieval::GraphCache>,
    ) -> Self {
        Self {
            lexical,
            vector,
            graph_cache,
        }
    }
}

#[async_trait]
impl ResetLaneLifecycle for RecipeResetLaneLifecycle {
    async fn cleanup_and_rebuild(
        &self,
        lane: ResetLane,
        scope: &Scope,
        retired: &[CodeResetTarget],
        _preserved: &[PreservationRecord],
    ) -> CoreResult<()> {
        match lane {
            ResetLane::Lexical => {
                if let Some(lexical) = &self.lexical {
                    let generations: BTreeSet<_> = retired
                        .iter()
                        .map(|target| target.lineage.generation_id.as_str())
                        .collect();
                    for generation in generations {
                        lexical.retire_generation(generation).await?;
                    }
                }
            }
            ResetLane::Vector => {
                if let Some(vector) = &self.vector {
                    for target in retired {
                        vector
                            .delete_target(&target.record_id.clone().into())
                            .await?;
                    }
                }
            }
            // These lanes derive their candidate structures from the shared graph snapshot.
            // Invalidate after durable retirement so the next read recomputes from preserved records.
            ResetLane::Associative | ResetLane::Community => {
                self.graph_cache.invalidate(scope).await
            }
            // Retained source reads are guarded by the durable eligibility/binding view.
            // Snapshots can be shared by immutable commits, so reset never removes a blob by path.
            ResetLane::Source => {}
        }
        Ok(())
    }
}

fn state_prefix(lane: ResetLane) -> String {
    format!("reset-lane:{}:", lane.name())
}
fn is_done(journal: &CodeResetJournal, lane: ResetLane) -> bool {
    journal
        .checkpoints
        .iter()
        .any(|c| c == &format!("{}done", state_prefix(lane)))
}
fn replace_state(journal: &mut CodeResetJournal, lane: ResetLane, state: &str) {
    let prefix = state_prefix(lane);
    journal
        .checkpoints
        .retain(|checkpoint| !checkpoint.starts_with(&prefix));
    journal.checkpoints.push(format!("{prefix}{state}"));
}

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

    /// Applies/resumes the complete reset state machine.
    /// The store commits retirement and its durable `completed:` checkpoints first.
    /// Each later lane transitions durably from pending to done or failed.
    /// Failure is BestEffort: earlier done lanes remain done, no rollback occurs,
    /// and a subsequent invocation retries only failed/pending lanes.
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
        let mut durable = self.store.apply_reset(journal).await?;
        let retired = durable.preview.targets.clone();
        let preserved = durable.preview.preservation_inventory.records.clone();
        let mut failed = false;
        for lane in ResetLane::ALL {
            if is_done(&durable, lane) {
                continue;
            }
            replace_state(&mut durable, lane, "pending");
            durable = self.store.save_reset_journal(durable).await?;
            match self
                .reset_lanes
                .cleanup_and_rebuild(lane, &scope, &retired, &preserved)
                .await
            {
                Ok(()) => replace_state(&mut durable, lane, "done"),
                Err(_error) => {
                    // Keep errors non-secret and durable; the lane is retried on resume.
                    replace_state(&mut durable, lane, "failed");
                    durable = self.store.save_reset_journal(durable).await?;
                    failed = true;
                    continue;
                }
            }
            durable = self.store.save_reset_journal(durable).await?;
        }
        if failed {
            // The returned journal is the explicit BestEffort result; callers inspect
            // failed checkpoints and resume with the immutable approved request.
        }
        Ok(durable)
    }

    /// Creates an immutable journal only through the reset-authorized seam.
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
        if journal.has_reserved_progress() {
            return Err(CoreError::InvalidRequest {
                reason: "ResetProgressSupplied: journal creation accepts no store-owned progress"
                    .into(),
            });
        }
        // Creation is the sole progress initializer.  Caller metadata never
        // becomes durable progress, including non-reserved compatibility notes.
        journal.checkpoints.clear();
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
