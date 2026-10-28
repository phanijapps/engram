//! Engine-neutral durable code repository binding and publication ports.
use async_trait::async_trait;
use engram_domain::{
    CodeAttempt, CodeBinding, CodeGeneration, CodeLineage, CodeResetJournal, CodeResetPreview,
    IndexedSource, Scope,
};
use engram_runtime::CoreResult;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositoryRegistration {
    pub repo_id: engram_domain::RepoId,
    pub locator_id: String,
    pub acquisition: AcquisitionMode,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionMode {
    LocalOnly,
    ConfiguredRemote,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeEligibility {
    pub scope: Scope,
    pub epoch: u64,
    pub generations: Vec<String>,
    pub repository_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReceipt {
    pub attempt_id: String,
    pub generation_id: String,
    pub lane: String,
    pub digest: String,
}

#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeBindingRepository: Send + Sync {
    async fn register(
        &self,
        registration: RepositoryRegistration,
    ) -> CoreResult<RepositoryRegistration>;
    async fn bind(&self, binding: CodeBinding) -> CoreResult<CodeBinding>;
    async fn get_binding(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
    ) -> CoreResult<Option<CodeBinding>>;
    async fn begin_attempt(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
        generation: CodeGeneration,
        attempt: CodeAttempt,
    ) -> CoreResult<CodeAttempt>;
    async fn get_attempt(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
        attempt_id: &str,
    ) -> CoreResult<Option<(CodeGeneration, CodeAttempt)>>;
    async fn put_generation(&self, generation: CodeGeneration) -> CoreResult<CodeGeneration>;
    async fn put_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: CodeLineage,
    ) -> CoreResult<CodeLineage>;
    async fn put_source_descriptor(
        &self,
        scope: &Scope,
        source: IndexedSource,
    ) -> CoreResult<IndexedSource>;
    async fn record_receipt(&self, receipt: StageReceipt) -> CoreResult<StageReceipt>;
    async fn mark_ready(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
        attempt_id: &str,
        fencing_token: u64,
    ) -> CoreResult<CodeAttempt>;
    async fn publish(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
        attempt_id: &str,
        expected_epoch: u64,
    ) -> CoreResult<CodeBinding>;
    async fn eligibility(&self, scope: &Scope) -> CoreResult<CodeEligibility>;
    async fn preview_reset(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
    ) -> CoreResult<CodeResetPreview>;
    async fn journal_reset(&self, journal: CodeResetJournal) -> CoreResult<CodeResetJournal>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstrainedCodeQuery {
    pub eligibility: CodeEligibility,
    pub target_ids: Vec<String>,
    pub limit: usize,
}
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeEligibilityGate: Send + Sync {
    async fn authorize_query(
        &self,
        scope: &Scope,
        requested_target_ids: &[String],
        limit: usize,
    ) -> CoreResult<ConstrainedCodeQuery>;
    async fn authorize_maintenance(
        &self,
        scope: &Scope,
        requested_target_ids: &[String],
    ) -> CoreResult<Vec<String>>;
}
