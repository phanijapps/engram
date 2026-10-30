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
/// The durable result of one lane attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetLaneOutcome {
    Completed,
    NotRequired,
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
    ) -> CoreResult<ResetLaneOutcome>;
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
    ) -> CoreResult<ResetLaneOutcome> {
        Ok(ResetLaneOutcome::Completed)
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
        preserved: &[PreservationRecord],
    ) -> CoreResult<ResetLaneOutcome> {
        match lane {
            ResetLane::Lexical => {
                if let Some(lexical) = &self.lexical {
                    for target in retired {
                        lexical
                            .retire_code_target(&target.record_id, &target.lineage.generation_id)
                            .await?;
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
            // r5's derived graph records recompute lazily at the next serving read.
            // This is the same cache that the SQLite serving lanes use, so its
            // invalidation makes every preserved contributor re-enter the r5
            // eligibility-aware recomputation and removes records with none.
            ResetLane::Associative | ResetLane::Community => {
                let _surviving_records = preserved
                    .iter()
                    .filter(|record| {
                        !retired
                            .iter()
                            .any(|target| target.record_id == record.record_id)
                    })
                    .count();
                self.graph_cache.invalidate(scope).await;
            }
            // r5 retains immutable snapshots as durable history, including the
            // last-good generation; no release seam exists in this architecture.
            ResetLane::Source => return Ok(ResetLaneOutcome::NotRequired),
        }
        Ok(ResetLaneOutcome::Completed)
    }
}

fn state_prefix(lane: ResetLane) -> String {
    format!("reset-lane:{}:", lane.name())
}
fn is_terminal(journal: &CodeResetJournal, lane: ResetLane) -> bool {
    let prefix = state_prefix(lane);
    journal.checkpoints.iter().any(|checkpoint| {
        checkpoint == &format!("{prefix}done") || checkpoint == &format!("{prefix}not-required")
    })
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
        let mut preview = self.store.preview_reset(scope, repo).await?;
        preview.preservation_inventory = self.preservation_source.inventory(scope).await?;
        preview.preservation_digests = vec![preview.preservation_inventory.digest.clone()];
        preview.preview_hash = preview.content_hash().map_err(|_| CoreError::Adapter {
            adapter: "engram-preservation".into(),
            message: "cannot hash reset preview".into(),
        })?;
        Ok(preview)
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
        // Probe the coordinator-owned source before the binding-cell mutation so a
        // missing source fails closed without retiring a target.
        let _ = self.preservation_source.inventory(&scope).await?;
        let mut durable = self.store.apply_reset(journal).await?;
        // Re-inventory independently after the fenced retirement.
        let inventory = self.preservation_source.inventory(&scope).await?;
        if inventory != durable.preview.preservation_inventory {
            durable.checkpoints.retain(|checkpoint| {
                checkpoint != "preservation:verified" && checkpoint != "preservation-verified"
            });
            durable
                .checkpoints
                .retain(|checkpoint| checkpoint != "preservation:failed");
            durable.checkpoints.push("preservation:failed".into());
            self.store.save_reset_journal(durable).await?;
            return Err(CoreError::Conflict {
                reason: "ResetPreservationFailed: knowledge preservation inventory changed".into(),
            });
        }
        durable
            .checkpoints
            .retain(|checkpoint| checkpoint != "preservation:failed");
        if !durable
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint == "preservation:verified")
        {
            durable.checkpoints.push("preservation:verified".into());
            // Retain the published spelling for transport compatibility.
            durable.checkpoints.push("preservation-verified".into());
        }
        durable = self.store.save_reset_journal(durable).await?;
        let retired = durable.preview.targets.clone();
        let preserved = durable.preview.preservation_inventory.records.clone();
        let mut failed = false;
        for lane in ResetLane::ALL {
            if is_terminal(&durable, lane) {
                continue;
            }
            replace_state(&mut durable, lane, "pending");
            durable = self.store.save_reset_journal(durable).await?;
            match self
                .reset_lanes
                .cleanup_and_rebuild(lane, &scope, &retired, &preserved)
                .await
            {
                Ok(ResetLaneOutcome::Completed) => replace_state(&mut durable, lane, "done"),
                Ok(ResetLaneOutcome::NotRequired) => {
                    replace_state(&mut durable, lane, "not-required")
                }
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
