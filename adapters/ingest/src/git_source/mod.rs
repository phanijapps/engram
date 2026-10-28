//! Committed Git object acquisition and immutable retained snapshots.
mod objects;
mod safety;
mod snapshot;
pub use objects::{
    CommittedFile, ResolvedCommit, acquire_local_branch, read_resolved_commit, resolve_local_branch,
};
pub use safety::{validate_branch_ref, validate_repo_path};
pub use snapshot::{RetainedSnapshot, retain_snapshot};
