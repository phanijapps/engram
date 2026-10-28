//! Portable contracts for project-bound code indexing.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBinding {
    pub scope: crate::Scope,
    pub repo_id: RepoId,
    pub branch_ref: String,
    pub version: u64,
    pub active_generation: Option<String>,
    pub epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeGeneration {
    pub id: String,
    pub binding_version: u64,
    pub commit_oid: String,
    pub snapshot_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLineage {
    pub repo_id: RepoId,
    pub generation_id: String,
    pub source_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedSource {
    pub repo_id: RepoId,
    pub generation_id: String,
    pub commit_oid: String,
    pub path: String,
    pub content_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeResetPreview {
    pub binding: CodeBinding,
    pub preview_hash: String,
    pub target_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeBindingError {
    InvalidRepoId,
    InvalidBranchRef,
    BindingConflict,
    BindingRequired,
    StaleGeneration,
    InvalidSource,
    InvalidResetPreview,
}

impl CodeBinding {
    pub fn validate(&self) -> Result<(), CodeBindingError> {
        if self.repo_id.0.trim().is_empty() {
            return Err(CodeBindingError::InvalidRepoId);
        }
        if !self.branch_ref.starts_with("refs/heads/")
            || self.branch_ref.len() <= "refs/heads/".len()
            || self.branch_ref.contains("..")
            || self.branch_ref.ends_with('/')
            || self.branch_ref.contains('@')
        {
            return Err(CodeBindingError::InvalidBranchRef);
        }
        Ok(())
    }
}
