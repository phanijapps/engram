//! Engine-neutral durable code repository binding and publication ports.
use async_trait::async_trait;
use engram_domain::{
    CodeAttempt, CodeBinding, CodeGeneration, CodeLineage, CodeResetJournal, CodeResetPreview,
    IndexedSource, Scope,
};
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

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

/// Ref-observation policy (plan DEC-007): observations of the bound branch
/// ref happen ONLY on an explicit on-request refresh — never on ordinary
/// search, exact fetch, or status without refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefObservationPolicy {
    OnRequest,
}

/// One durable on-request observation of a bound branch ref (TASK-018):
/// the last successful observation of the CONFIGURED ref, recorded
/// SEPARATELY from the indexed generation OID. Never derived from a
/// checkout HEAD, a scan-root sidecar, or any unrelated checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRefObservation {
    pub branch_ref: String,
    pub observed_oid: String,
    pub observed_at: engram_domain::Timestamp,
    pub policy: RefObservationPolicy,
}

impl CodeRefObservation {
    /// Durable-record validation: a full hex object id (40/64) on the
    /// observed ref and a well-formed `refs/heads/…` branch (shared
    /// [`engram_domain::validate_branch_ref`]). An invalid observation is
    /// never persisted.
    pub fn validate(&self) -> Result<(), engram_domain::CodeBindingError> {
        if !matches!(self.observed_oid.len(), 40 | 64)
            || !self.observed_oid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(engram_domain::CodeBindingError::InvalidSource);
        }
        engram_domain::validate_branch_ref(&self.branch_ref)
    }
}

#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CodeBindingRepository: crate::CodeReadLeases + Send + Sync {
    async fn register(
        &self,
        registration: RepositoryRegistration,
    ) -> CoreResult<RepositoryRegistration>;
    async fn bind(&self, binding: CodeBinding) -> CoreResult<CodeBinding>;
    /// The durable registration for one repository (TASK-021): the
    /// scope-authorized locator an on-request refresh resolves through.
    /// `Ok(None)` is the honest "no registration exists" state — callers
    /// deny typed rather than guessing a checkout. Default: none.
    async fn get_registration(
        &self,
        _repo_id: &engram_domain::RepoId,
    ) -> CoreResult<Option<RepositoryRegistration>> {
        Ok(None)
    }
    async fn get_binding(
        &self,
        scope: &Scope,
        repo_id: &engram_domain::RepoId,
    ) -> CoreResult<Option<CodeBinding>>;
    /// All repository bindings in one scope, including bindings without a
    /// published generation. Callers use this authority boundary to keep
    /// legacy arbitrary-path lanes closed after binding.
    async fn bindings(&self, _scope: &Scope) -> CoreResult<Vec<CodeBinding>> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-binding-scope-list".into(),
            reason: "this cell does not expose scope binding records".into(),
        })
    }
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
    /// Builds the durable inventory of ordinary records that a reset of
    /// `target_repo` must leave untouched.
    async fn preservation_inventory(
        &self,
        scope: &Scope,
        target_repo: &engram_domain::RepoId,
    ) -> CoreResult<engram_domain::PreservationInventory> {
        let mut records = self
            .target_lineage(scope)
            .await?
            .into_iter()
            .filter(|entry| entry.repo_id != target_repo.0)
            .map(|entry| {
                let content = serde_json::to_vec(&entry).map_err(|_| CoreError::Adapter {
                    adapter: "engram-code-binding".into(),
                    message: "cannot hash durable preservation record".into(),
                })?;
                let contributors =
                    serde_json::to_vec(&entry.contributors).map_err(|_| CoreError::Adapter {
                        adapter: "engram-code-binding".into(),
                        message: "cannot hash durable preservation contributors".into(),
                    })?;
                Ok(engram_domain::PreservationRecord {
                    record_class: "lineage".into(),
                    record_id: entry.record_id,
                    content_digest: Sha256::digest(content)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                    contributor_digest: Sha256::digest(contributors)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;
        records.sort_by(|left, right| left.record_id.cmp(&right.record_id));
        let mut inventory = engram_domain::PreservationInventory {
            records,
            digest: String::new(),
        };
        inventory.digest = inventory.content_hash().map_err(|_| CoreError::Adapter {
            adapter: "engram-code-binding".into(),
            message: "cannot hash preservation inventory".into(),
        })?;
        Ok(inventory)
    }
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
    /// Retires fenced lineage and, when supplied, atomically records the
    /// reset-owned completion rows for exactly these targets. The default
    /// preserves retirement-only callers and cells that do not persist reset
    /// journals.
    async fn retire_lineage_targets_with_reset_progress(
        &self,
        scope: &Scope,
        expected_binding_epochs: &[(engram_domain::RepoId, u64)],
        targets: &[crate::RetirementTarget],
        _reset_progress: Option<(&str, &[String])>,
    ) -> CoreResult<()> {
        self.retire_lineage_targets(scope, expected_binding_epochs, targets)
            .await
    }
    async fn put_source_descriptor(
        &self,
        scope: &Scope,
        source: IndexedSource,
    ) -> CoreResult<IndexedSource>;
    /// Persists one on-request ref observation for a BOUND repository
    /// (TASK-018). The observation is keyed by (scope, RepoId) — never by a
    /// root basename or source string — and records the observed OID/time
    /// separately from the indexed generation OID. Default: unsupported.
    async fn put_ref_observation(
        &self,
        _scope: &Scope,
        _repo_id: &engram_domain::RepoId,
        _observation: CodeRefObservation,
    ) -> CoreResult<CodeRefObservation> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-ref-observation".into(),
            reason: "this cell does not persist ref observations".into(),
        })
    }
    /// The durable last ref observation for one bound repository.
    /// `Ok(None)` is the honest "no valid observation exists" state
    /// (freshness stays UNKNOWN); it is not an error. Default: none.
    async fn get_ref_observation(
        &self,
        _scope: &Scope,
        _repo_id: &engram_domain::RepoId,
    ) -> CoreResult<Option<CodeRefObservation>> {
        Ok(None)
    }
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
    /// Loads the immutable owner-approved reset journal by reset identifier.
    async fn load_reset_journal(&self, _reset_id: &str) -> CoreResult<CodeResetJournal> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-reset-journal-load".into(),
            reason: "this cell does not expose durable reset journals".into(),
        })
    }
    /// Persists durable reset progress after a completed local mutation.
    async fn save_reset_journal(&self, _journal: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        Err(CoreError::CapabilityUnsupported {
            capability: "code-reset-journal-progress".into(),
            reason: "this cell does not persist reset progress".into(),
        })
    }
    /// Applies only the journal's unfinished exact targets.
    /// Each cell inherits this neutral, fenced operation and supplies the
    /// durable retirement and lease primitives behind it.
    /// Successful local-store work is never rolled back when a later store
    /// reports an error; callers receive the returned checkpoints to resume.
    async fn apply_reset(&self, supplied: CodeResetJournal) -> CoreResult<CodeResetJournal> {
        supplied.validate().map_err(|_| CoreError::InvalidRequest {
            reason: "InvalidResetPreview: journal does not bind the complete preview".into(),
        })?;
        if supplied
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.starts_with("completed:"))
        {
            return Err(CoreError::InvalidRequest {
                reason: "ResetProgressSupplied: completion checkpoints are durable-store owned"
                    .into(),
            });
        }
        let mut journal = self.load_reset_journal(&supplied.reset_id).await?;
        if journal.preview_hash != supplied.preview_hash
            || journal.preview != supplied.preview
            || journal.target_fingerprints != supplied.target_fingerprints
            || journal.authorizing_decisions != supplied.authorizing_decisions
            || journal.preservation_digests != supplied.preservation_digests
        {
            return Err(CoreError::Conflict {
                reason:
                    "ResetJournalUnauthorized: request does not match the owner-approved journal"
                        .into(),
            });
        }
        let scope = journal.preview.binding.scope.clone();
        let repo = journal.preview.binding.repo_id.clone();
        let binding =
            self.get_binding(&scope, &repo)
                .await?
                .ok_or_else(|| CoreError::Conflict {
                    reason: "ResetPreviewStale: binding no longer exists".into(),
                })?;
        let completed: std::collections::BTreeSet<String> = journal
            .checkpoints
            .iter()
            .filter_map(|checkpoint| checkpoint.strip_prefix("completed:").map(str::to_owned))
            .collect();
        let remaining: Vec<_> = journal
            .preview
            .targets
            .iter()
            .filter(|target| !completed.contains(&target.record_id))
            .collect();
        if remaining.is_empty()
            && journal
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint == "preservation:verified")
        {
            return Ok(journal);
        }
        // Recompute the complete target fingerprint from CURRENT durable
        // lineage and generation before the first mutation.
        for target in &remaining {
            let lineage = self
                .record_lineage(&scope, &target.record_id)
                .await?
                .ok_or_else(|| CoreError::Conflict {
                    reason: "ResetTargetSubstituted: target is absent".into(),
                })?;
            if lineage.retired
                || lineage.repo_id != repo.0
                || lineage.generation_id != target.lineage.generation_id
            {
                return Err(CoreError::Conflict {
                    reason: "ResetTargetSubstituted: durable lineage changed".into(),
                });
            }
            let generation = self
                .get_generation(&scope, &repo, &lineage.generation_id)
                .await?
                .ok_or_else(|| CoreError::Conflict {
                    reason: "ResetTargetSubstituted: generation is absent".into(),
                })?;
            let durable_lineage = engram_domain::CodeLineage {
                repo_id: repo.clone(),
                generation_id: lineage.generation_id,
                source_path: lineage.source_path,
                contributors: lineage.contributors,
                span: lineage.span,
            };
            let material = serde_json::to_vec(&(
                target.record_id.clone(),
                &durable_lineage,
                &generation.snapshot_digest,
                target.binding_version,
                target.code_epoch,
            ))
            .map_err(|_| CoreError::Adapter {
                adapter: "engram-code-binding".into(),
                message: "cannot validate reset target".into(),
            })?;
            let fingerprint: String = Sha256::digest(material)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            if fingerprint != target.fingerprint || target.binding_version != binding.version {
                return Err(CoreError::Conflict {
                    reason: "ResetPreviewStale: target fingerprint or binding version changed"
                        .into(),
                });
            }
        }
        // Only durable checkpoints written by this reset journal establish
        // completed work. A retired lineage without its checkpoint is stale.
        let completed: std::collections::BTreeSet<String> = journal
            .checkpoints
            .iter()
            .filter_map(|checkpoint| checkpoint.strip_prefix("completed:").map(str::to_owned))
            .collect();
        let remaining: Vec<_> = journal
            .preview
            .targets
            .iter()
            .filter(|target| !completed.contains(&target.record_id))
            .collect();
        if remaining.is_empty()
            && journal
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint == "preservation:verified")
        {
            return Ok(journal);
        }
        if completed.is_empty() && binding.epoch != journal.preview.code_epoch {
            return Err(CoreError::Conflict {
                reason: "ResetPreviewStale: code epoch changed".into(),
            });
        }
        let expected_epoch = binding.epoch;
        let targets = remaining
            .iter()
            .map(|target| crate::RetirementTarget {
                repo_id: repo.0.clone(),
                generation_id: target.lineage.generation_id.clone(),
                record_id: target.record_id.clone(),
            })
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            let completed_ids = remaining
                .iter()
                .map(|target| target.record_id.clone())
                .collect::<Vec<_>>();
            self.retire_lineage_targets_with_reset_progress(
                &scope,
                &[(repo.clone(), expected_epoch)],
                &targets,
                Some((&journal.reset_id, &completed_ids)),
            )
            .await?;
            // The cell committed these rows with retirement. Keep this local
            // response current without a second progress commit; recovery
            // reloads the same rows from the durable journal-progress table.
            journal.checkpoints.extend(
                completed_ids
                    .iter()
                    .map(|record_id| format!("completed:{record_id}")),
            );
        }
        // Re-read the durable ordinary-record inventory after mutation.
        // This is intentionally independent of the owner-approved preview.
        let durable_inventory = self.preservation_inventory(&scope, &repo).await?;
        if durable_inventory != journal.preview.preservation_inventory {
            journal
                .checkpoints
                .retain(|checkpoint| checkpoint != "preservation:verified");
            journal
                .checkpoints
                .retain(|checkpoint| checkpoint != "preservation:failed");
            journal.checkpoints.push("preservation:failed".into());
            self.save_reset_journal(journal).await?;
            return Err(CoreError::Conflict {
                reason: "ResetPreservationFailed: ordinary preservation inventory changed".into(),
            });
        }
        journal
            .checkpoints
            .retain(|checkpoint| checkpoint != "preservation:failed");
        if !journal
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint == "preservation:verified")
        {
            journal.checkpoints.push("preservation:verified".into());
            // Retain the published spelling for transport compatibility.
            journal.checkpoints.push("preservation-verified".into());
        }
        self.save_reset_journal(journal).await
    }
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
    /// Source-role contributor ids read from the durable lineage manifest.
    #[serde(default)]
    pub contributors: Vec<String>,
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
