//! Authorized, engine-neutral code binding and publication coordination.
mod eligibility;
mod publication;
mod registration;
mod source;
mod status;
use async_trait::async_trait;
use engram_domain::{
    Actor, CodeAttempt, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
};
use engram_knowledge::{CodeBindingRepository, CoreResult, StageReceipt};
pub use publication::CodeRebuildRequest;
pub use source::{ExactReadCandidate, ParticipantExactSource};
pub use status::CodeBindingStatus;

/// Engine-neutral [`CodeEligibilityResolver`] over the durable
/// `CodeBindingRepository::eligibility` port: publishes the scope's published
/// generation/repository set to retrieval lanes without naming an engine.
pub struct RepositoryEligibilitySource {
    store: Arc<dyn engram_knowledge::CodeBindingRepository>,
}
impl RepositoryEligibilitySource {
    pub fn new(store: Arc<dyn engram_knowledge::CodeBindingRepository>) -> Self {
        Self { store }
    }
}
#[async_trait::async_trait]
impl engram_retrieval::CodeEligibilityResolver for RepositoryEligibilitySource {
    async fn resolve(
        &self,
        scope: &engram_domain::Scope,
    ) -> engram_runtime::CoreResult<engram_retrieval::BoundCodeEligibility> {
        // Corrective r2 (findings 4/9): resolve the DURABLE authority —
        // published bindings with their immutable branch identity (read
        // per-repo from the binding records) and the staged target lineage —
        // never record metadata.
        let eligibility = self.store.eligibility(scope).await?;
        let mut bindings = Vec::with_capacity(eligibility.repository_ids.len());
        for repo in &eligibility.repository_ids {
            let repo_id = engram_domain::RepoId(repo.clone());
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
            // Review r5 finding 2: retired associations remain POSITIVE code
            // ownership with denied eligibility — the view carries the
            // retirement fact instead of discarding the row (a discarded row
            // made retired code flow through retirement-blind lanes).
            .map(|entry| engram_retrieval::DurableLineage {
                record_id: entry.record_id,
                repo_id: entry.repo_id,
                generation_id: entry.generation_id,
                retired: entry.retired,
                source_path: entry.source_path,
                span: entry.span,
            })
            .collect();
        Ok(engram_retrieval::BoundCodeEligibility {
            epoch: eligibility.epoch,
            bindings,
            lineage,
        })
    }
}
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeAuthorityAction {
    Bind,
    Rebuild,
    Inspect,
    Register,
    Reset,
}
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeBindingAuthorizer: Send + Sync {
    async fn authorize(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo: &RepoId,
        action: CodeAuthorityAction,
    ) -> CoreResult<()>;
}
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeGenerationStages: Send + Sync {
    /// Complete durable attempt-owned writes before returning a readiness receipt.
    async fn stage(
        &self,
        lane: &CodeReceiptKind,
        binding: &CodeBinding,
        generation: &CodeGeneration,
        attempt: &CodeAttempt,
    ) -> CoreResult<StageReceipt>;
}
pub struct CodeBindingCoordinator {
    store: Arc<dyn CodeBindingRepository>,
    authorizer: Arc<dyn CodeBindingAuthorizer>,
    required_lanes: Vec<CodeReceiptKind>,
    clock: Arc<dyn engram_runtime::Clock>,
}
impl CodeBindingCoordinator {
    /// Required source and knowledge lanes cannot be removed by a request.
    pub fn new(
        store: Arc<dyn CodeBindingRepository>,
        authorizer: Arc<dyn CodeBindingAuthorizer>,
        configured_lanes: Vec<CodeReceiptKind>,
    ) -> Self {
        let mut required_lanes = vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge];
        for lane in configured_lanes {
            if !required_lanes.contains(&lane) {
                required_lanes.push(lane);
            }
        }
        Self {
            store,
            authorizer,
            required_lanes,
            clock: Arc::new(UtcClock),
        }
    }
}
struct UtcClock;
impl engram_runtime::Clock for UtcClock {
    fn now(&self) -> engram_domain::Timestamp {
        chrono::Utc::now()
    }
}
impl CodeBindingCoordinator {
    pub fn with_clock(mut self, clock: Arc<dyn engram_runtime::Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Shared caller-authority check for code operations (corrective r2,
    /// finding 3): every new seam authenticates through the SAME authorizer
    /// as registration/publication — a denying authorizer denies even for
    /// fully populated scopes.
    #[allow(clippy::double_must_use)]
    pub async fn authorize(&self, actor: &Actor, scope: &Scope, repo: &RepoId) -> CoreResult<()> {
        self.authorizer
            .authorize(actor, scope, repo, CodeAuthorityAction::Inspect)
            .await
    }

    /// The durable binding store handle (shared seams read through this).
    pub fn store(&self) -> Arc<dyn CodeBindingRepository> {
        Arc::clone(&self.store)
    }

    /// The shared authorizer handle (crate-internal seams revalidate caller
    /// authority INSIDE their read windows — corrective r5, A4).
    pub(crate) fn authorizer_handle(&self) -> Arc<dyn CodeBindingAuthorizer> {
        Arc::clone(&self.authorizer)
    }
}
