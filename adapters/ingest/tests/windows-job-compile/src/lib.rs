//! Compile guard for the exact production Windows job primitive, not a copy.
//! Run on a Windows host for runtime checks; cross-checking proves syntax/types only.
#![allow(dead_code)]
#[cfg(windows)]
#[path = "../../../src/git_source/bounded_process.rs"]
mod bounded_process;
#[cfg(windows)]
#[path = "../../../src/git_source/windows_job.rs"]
mod windows_job;

#[cfg(windows)]
#[path = "../../../src/git_source/private_storage.rs"]
mod private_storage;
#[cfg(windows)]
#[path = "../../../src/git_source/snapshot.rs"]
mod snapshot;
#[cfg(windows)]
#[path = "../../../src/git_source/objects.rs"]
mod objects;
#[cfg(windows)]
#[path = "../../../src/git_source/safety.rs"]
mod safety;
#[cfg(windows)]
use bounded_process::GitResourceLimits;
#[cfg(windows)]
use objects::{CommittedFile,ResolvedCommit};
#[cfg(windows)]
use safety::{validate_branch_ref,validate_repo_path};
