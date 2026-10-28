//! Authorized, engine-neutral code binding and publication coordination.
mod eligibility;
mod publication;
mod registration;
mod status;
use async_trait::async_trait;
use engram_domain::{
    Actor, CodeAttempt, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
};
use engram_knowledge::{CodeBindingRepository, CoreResult, StageReceipt};
pub use publication::CodeRebuildRequest;
pub use status::CodeBindingStatus;
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
}
