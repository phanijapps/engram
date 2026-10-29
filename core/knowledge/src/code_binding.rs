//! Engine-neutral durable code repository binding and publication ports.
use async_trait::async_trait;
use engram_domain::{
    CodeAttempt, CodeBinding, CodeGeneration, CodeLineage, CodeResetJournal, CodeResetPreview,
    IndexedSource, Scope,
};
use engram_runtime::{CoreError, CoreResult};

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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub trait CodeBindingRepository: crate::CodeReadLeases + Send + Sync {
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
    /// Reads one durable generation record BOUND to this scope and repository
    /// (corrective r2, finding 2): a generation id alone is not authority.
    /// Cells must resolve the (scope, repository, generation) ownership
    /// association durably; foreign scopes/repositories deny with `None`.
    /// Default: unsupported.
    async fn get_generation(
        &self,
        _scope: &Scope,
        _repo_id: &engram_domain::RepoId,
        _generation_id: &str,
    ) -> CoreResult<Option<CodeGeneration>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-generation-read".into(),
            reason: "this cell does not expose generation records".into(),
        })
    }
    /// The durable target lineage for a scope (finding 4): every staged
    /// record → repository + generation association. Default: unsupported.
    async fn target_lineage(&self, _scope: &Scope) -> CoreResult<Vec<TargetLineage>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-lineage-read".into(),
            reason: "this cell does not expose target lineage".into(),
        })
    }
    /// The durable lineage association for ONE record in a scope (work
    /// package A3): a BOUNDED KEYED ownership read. Dependency walks that
    /// resolve a single candidate's own lineage must not materialize the
    /// whole scope's lineage per record. Store-backed implementors MUST
    /// override this with a keyed read; the compatibility default falls
    /// back to [`Self::target_lineage`] and scans the result.
    async fn record_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
    ) -> CoreResult<Option<TargetLineage>> {
        Ok(self
            .target_lineage(scope)
            .await?
            .into_iter()
            .find(|entry| entry.record_id == record_id))
    }
    /// The durable indexed-source descriptor for one (scope, repository,
    /// generation, path) — the retained-blob content digest authority
    /// (finding 7). Default: unsupported.
    async fn get_source_descriptor(
        &self,
        _scope: &Scope,
        _repo_id: &engram_domain::RepoId,
        _generation_id: &str,
        _path: &str,
    ) -> CoreResult<Option<IndexedSource>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-source-descriptor".into(),
            reason: "this cell does not expose source descriptors".into(),
        })
    }
    async fn put_generation(&self, generation: CodeGeneration) -> CoreResult<CodeGeneration>;
    async fn put_lineage(
        &self,
        scope: &Scope,
        record_id: &str,
        lineage: CodeLineage,
    ) -> CoreResult<CodeLineage>;
    /// Owned maintenance mutation (corrective r4, canonical 3a; review r5
    /// findings 3/10): retires the durable lineage of `targets` — TYPED
    /// expected scope/repository/generation associations — under an
    /// EPOCH-CAS fence. The expected-epoch comparison, the validation of
    /// EVERY target association (repository membership AND exact expected
    /// generation) and the lineage retirement (the owned first side effect)
    /// all commit in ONE transaction, and the whole set is validated before
    /// the FIRST change: a publication that committed first makes the CAS
    /// fail with `GenerationChanged` (no side effect), a target whose
    /// durable lineage carries a different generation than the grant
    /// validated rejects the whole operation, and a repeated retirement
    /// whose associations match exactly is an idempotent no-op SUCCESS that
    /// advances no epochs. Epochs advance only when at least one target
    /// actually needs retirement. Default: unsupported (unrelated
    /// implementors keep compiling).
    async fn retire_lineage_targets(
        &self,
        _scope: &Scope,
        _expected_binding_epochs: &[(engram_domain::RepoId, u64)],
        _targets: &[crate::RetirementTarget],
    ) -> CoreResult<()> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-maintenance-retire".into(),
            reason: "this cell does not expose fenced maintenance retirement".into(),
        })
    }
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

/// One durable target-lineage association (corrective r2, finding 4): the
/// scanner's staged lineage record binding a target id to the repository and
/// generation that contributed it. This — not record metadata — is the
/// authority for code classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetLineage {
    pub record_id: String,
    pub repo_id: String,
    /// The immutable historical generation that created this record.
    pub generation_id: String,
    /// Additive durable retirement fact. Retired records retain their true
    /// lineage but are excluded from published-generation eligibility.
    #[serde(default)]
    pub retired: bool,
    /// The record's durable canonical source path at its generation's pinned
    /// commit (final r5 finding 2). Empty for lineage persisted before this
    /// field existed — such rows can never path-match and so deny exact
    /// participation (fail closed).
    #[serde(default)]
    pub source_path: String,
    /// The record's durable canonical line span within `source_path`
    /// (final r5 finding 2). `None` for records without a single canonical
    /// span.
    #[serde(default)]
    pub span: Option<engram_domain::CanonicalSpan>,
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
