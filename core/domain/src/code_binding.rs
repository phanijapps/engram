//! Portable contracts for project-bound code indexing.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBinding {
    pub scope: crate::Scope,
    pub repo_id: RepoId,
    pub branch_ref: String,
    pub version: u64,
    pub active_generation: Option<String>,
    pub epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeGeneration {
    pub id: String,
    pub binding_version: u64,
    pub commit_oid: String,
    pub snapshot_digest: String,
    pub parser_fingerprint: String,
    pub filter_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLineage {
    pub repo_id: RepoId,
    pub generation_id: String,
    pub source_path: String,
    pub contributors: Vec<String>,
    /// The record's durable canonical line span in its `source_path` at the
    /// generation's pinned commit (final r5 finding 2): staged once at
    /// publication and never re-read from mutable rows. `None` for records
    /// without a single canonical span (source/graph/relationship rows, and
    /// pre-span lineage persisted before this field existed).
    #[serde(default)]
    pub span: Option<CanonicalSpan>,
}

/// A durable 1-based inclusive canonical line span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalSpan {
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedSource {
    pub repo_id: RepoId,
    pub generation_id: String,
    pub commit_oid: String,
    pub path: String,
    pub content_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAttempt {
    pub id: String,
    pub idempotency_key: String,
    pub binding_version: u64,
    pub expected_epoch: u64,
    pub expected_active_generation: Option<String>,
    pub fencing_token: u64,
    pub state: CodeAttemptState,
    pub receipt_kinds: Vec<CodeReceiptKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeAttemptState {
    Created,
    Acquiring,
    Staging,
    Ready,
    Published,
    Failed,
    Superseded,
    CleanupPending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeReceiptKind {
    Source,
    Knowledge,
    Vector,
    Lexical,
    Derived,
}

/// One exact, durable target in a reset preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeResetTarget {
    pub record_id: String,
    pub store_id: String,
    pub fingerprint: String,
    pub lineage: CodeLineage,
    pub snapshot_ref: String,
    pub scan_metadata_keys: Vec<String>,
    pub binding_version: u64,
    pub code_epoch: u64,
}

/// A legacy row that cannot be proved code-owned at record level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousOwnership {
    pub record_id: String,
    pub reason: String,
}

/// Immutable reset proposal only.
/// This document names exact records and makes no cross-store atomicity claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservationRecord {
    pub record_id: String,
    pub content_digest: String,
    /// This digest binds the contributors that a surviving source shares with
    /// this preserved record.
    pub contributor_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservationInventory {
    pub records: Vec<PreservationRecord>,
    pub digest: String,
}

impl Default for PreservationInventory {
    fn default() -> Self {
        let mut inventory = Self {
            records: Vec::new(),
            digest: String::new(),
        };
        inventory.digest = inventory
            .content_hash()
            .expect("empty inventory serializes");
        inventory
    }
}

impl PreservationInventory {
    pub fn content_hash(&self) -> Result<String, CodeBindingError> {
        let bytes =
            serde_json::to_vec(&self.records).map_err(|_| CodeBindingError::InvalidResetPreview)?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.digest.len() != 64
            || !self.digest.bytes().all(|b| b.is_ascii_hexdigit())
            || self.records.iter().any(|record| {
                record.record_id.trim().is_empty()
                    || record.content_digest.len() != 64
                    || record.contributor_digest.len() != 64
            })
            || self.content_hash()? != self.digest
        {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeResetPreview {
    pub binding: CodeBinding,
    pub preview_hash: String,
    /// Retained for transport compatibility; it exactly mirrors `targets`.
    pub target_ids: Vec<String>,
    #[serde(default)]
    pub targets: Vec<CodeResetTarget>,
    #[serde(default)]
    pub ambiguity: Vec<AmbiguousOwnership>,
    #[serde(default)]
    pub preservation_digests: Vec<String>,
    /// Durable ordinary-record inventory that must survive this reset.
    #[serde(default)]
    pub preservation_inventory: PreservationInventory,
    #[serde(default)]
    pub binding_versions: Vec<(RepoId, u64)>,
    #[serde(default)]
    pub code_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeResetJournal {
    pub reset_id: String,
    pub preview: CodeResetPreview,
    pub preview_hash: String,
    pub target_fingerprints: Vec<(String, String)>,
    pub authorizing_decisions: Vec<String>,
    pub checkpoints: Vec<String>,
    pub preservation_digests: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeBindingError {
    InvalidRepoId,
    InvalidBranchRef,
    InvalidBindingVersion,
    InvalidGeneration,
    BindingConflict,
    BindingRequired,
    StaleGeneration,
    InvalidSource,
    InvalidResetPreview,
    InvalidAttempt,
    PersistenceFailure,
    /// The durable PUBLISH boundary could not take its write fence —
    /// busy/lock contention with another durable writer holding the fence
    /// (r5 C4, verification finding 2). A DISTINCT, retryable outcome:
    /// unrelated persistence failures (query, decode, transaction
    /// machinery) remain `PersistenceFailure`; a lost CAS race remains
    /// `StaleGeneration`. Engine-neutral by construction — adapters map
    /// their engine's busy/contention condition onto it.
    PublicationContention,
}

/// Shared full-branch-ref validation (Git ref-format rules for
/// `refs/heads/...`): used by [`CodeBinding::validate`] and by ref
/// observations (TASK-018) so both reject the same malformed refs - tags,
/// HEAD, OID input, revision expressions, ambiguous shorthand, and
/// case-distinct missing names never reach durable records.
pub fn validate_branch_ref(branch_ref: &str) -> Result<(), CodeBindingError> {
    let suffix = branch_ref
        .strip_prefix("refs/heads/")
        .ok_or(CodeBindingError::InvalidBranchRef)?;
    if suffix.is_empty()
        || suffix.starts_with('/')
        || suffix.ends_with('/')
        || suffix.ends_with('.')
        || suffix.contains("..")
        || suffix.contains("@{")
        || suffix.contains("//")
        || suffix
            .chars()
            .any(|c| c.is_control() || " ~^:?*[\\".contains(c))
        || suffix
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        return Err(CodeBindingError::InvalidBranchRef);
    }
    Ok(())
}

impl CodeBinding {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.repo_id.0.trim().is_empty() {
            return Err(CodeBindingError::InvalidRepoId);
        }
        if self.version == 0 {
            return Err(CodeBindingError::InvalidBindingVersion);
        }
        if self.active_generation.as_deref() == Some("") {
            return Err(CodeBindingError::InvalidGeneration);
        }
        validate_branch_ref(&self.branch_ref)
    }
}

impl CodeGeneration {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.id.trim().is_empty()
            || self.binding_version == 0
            || !matches!(self.commit_oid.len(), 40 | 64)
            || !self.commit_oid.bytes().all(|b| b.is_ascii_hexdigit())
            || self.snapshot_digest.len() != 64
            || !self.snapshot_digest.bytes().all(|b| b.is_ascii_hexdigit())
            || self.parser_fingerprint.is_empty()
            || self.filter_fingerprint.is_empty()
        {
            return Err(CodeBindingError::InvalidSource);
        }
        Ok(())
    }
}

impl CodeLineage {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.repo_id.0.trim().is_empty()
            || self.generation_id.trim().is_empty()
            || self.source_path.is_empty()
            || self.source_path.starts_with('/')
            || self
                .source_path
                .split('/')
                .any(|p| p == ".." || p.is_empty())
            || self.contributors.is_empty()
            || self.contributors.iter().any(|x| x.trim().is_empty())
        {
            return Err(CodeBindingError::InvalidSource);
        }
        if let Some(span) = &self.span
            && (span.start_line == 0 || span.end_line < span.start_line)
        {
            return Err(CodeBindingError::InvalidSource);
        }
        Ok(())
    }
}

impl IndexedSource {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.repo_id.0.trim().is_empty()
            || self.generation_id.trim().is_empty()
            || self.path.is_empty()
            || self.path.starts_with('/')
            || self.path.split('/').any(|p| p == ".." || p.is_empty())
            || !matches!(self.commit_oid.len(), 40 | 64)
            || !self.commit_oid.bytes().all(|b| b.is_ascii_hexdigit())
            || self.content_digest.len() != 64
            || !self.content_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(CodeBindingError::InvalidSource);
        }
        Ok(())
    }
}

impl CodeAttempt {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.id.trim().is_empty()
            || self.idempotency_key.trim().is_empty()
            || self.binding_version == 0
            || self.fencing_token == 0
            || (self.state == CodeAttemptState::Ready && self.receipt_kinds.is_empty())
        {
            return Err(CodeBindingError::InvalidAttempt);
        }
        Ok(())
    }
}

impl CodeResetPreview {
    /// Hashes the complete proposal with its hash field blanked.
    pub fn content_hash(&self) -> Result<String, CodeBindingError> {
        let mut document = self.clone();
        document.preview_hash.clear();
        let bytes =
            serde_json::to_vec(&document).map_err(|_| CodeBindingError::InvalidResetPreview)?;
        use sha2::{Digest, Sha256};
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn validate(&self) -> Result<(), CodeBindingError> {
        self.binding.validate()?;
        if self.preview_hash.len() != 64
            || !self.preview_hash.bytes().all(|b| b.is_ascii_hexdigit())
            || self.target_ids.is_empty()
            || self.target_ids.iter().any(|x| x.trim().is_empty())
            || !self.ambiguity.is_empty()
            || (!self.targets.is_empty()
                && (self.targets.len() != self.target_ids.len()
                    || self
                        .targets
                        .iter()
                        .map(|target| &target.record_id)
                        .collect::<Vec<_>>()
                        != self.target_ids.iter().collect::<Vec<_>>()))
            || self.preservation_inventory.validate().is_err()
            || self.preservation_digests != vec![self.preservation_inventory.digest.clone()]
            || (!self.targets.is_empty() && self.content_hash()? != self.preview_hash)
        {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        Ok(())
    }
}

impl CodeResetJournal {
    /// Progress is store-owned and is never part of an owner approval request.
    pub fn has_reserved_progress(&self) -> bool {
        self.checkpoints.iter().any(|checkpoint| {
            checkpoint.starts_with("completed:")
                || checkpoint.starts_with("reset-lane:")
                || checkpoint.starts_with("preservation:")
                || checkpoint == "preservation-verified"
        })
    }

    pub fn validate(&self) -> Result<(), CodeBindingError> {
        self.preview.validate()?;
        if self.reset_id.trim().is_empty()
            || self.preview_hash != self.preview.preview_hash
            || self.authorizing_decisions.is_empty()
            || self.target_fingerprints.len() != self.preview.targets.len()
            || self
                .target_fingerprints
                .iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
                != self
                    .preview
                    .targets
                    .iter()
                    .map(|target| &target.record_id)
                    .collect::<Vec<_>>()
            || self
                .target_fingerprints
                .iter()
                .zip(&self.preview.targets)
                .any(|((_, fingerprint), target)| fingerprint != &target.fingerprint)
            || self.preservation_digests != self.preview.preservation_digests
        {
            return Err(CodeBindingError::InvalidResetPreview);
        }
        Ok(())
    }
}
