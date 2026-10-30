//! Committed Git object acquisition and immutable retained snapshots.
mod acquire;
mod bounded_process;
mod objects;
mod portable_pack;
mod portable_remote;
mod private_storage;
mod retained_reader;
mod safety;
mod snapshot;
#[cfg(windows)]
mod windows_job;
pub use acquire::{RemotePolicy, RemoteSnapshot, acquire_remote_branch};
pub use bounded_process::GitResourceLimits;
pub use objects::{
    CommittedFile, ResolvedCommit, acquire_local_branch, acquire_local_branch_with_limits,
    read_resolved_commit, read_resolved_commit_with_limits, resolve_local_branch,
    resolve_local_branch_with_limits,
};
pub(crate) use private_storage::private_directory;
pub use retained_reader::RetainedSnapshotReader;
pub use safety::{validate_branch_ref, validate_repo_path};
pub use snapshot::{
    RetainedSnapshot, retain_snapshot, retain_snapshot_with_limits, validate_retained_snapshot,
};
