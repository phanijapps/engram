//! Committed Git object acquisition and immutable retained snapshots.
mod acquire;
mod bounded_process;
mod objects;
mod safety;
mod snapshot;
pub use acquire::{RemotePolicy, RemoteSnapshot, acquire_remote_branch};
pub use bounded_process::GitResourceLimits;
pub use objects::{
    CommittedFile, ResolvedCommit, acquire_local_branch, acquire_local_branch_with_limits,
    read_resolved_commit, read_resolved_commit_with_limits, resolve_local_branch,
    resolve_local_branch_with_limits,
};
pub use safety::{validate_branch_ref, validate_repo_path};
pub(crate) use snapshot::private_directory;
pub use snapshot::{RetainedSnapshot, retain_snapshot};
