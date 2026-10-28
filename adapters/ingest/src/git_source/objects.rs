use super::bounded_process::{GitResourceLimits, run_limited};
use super::{validate_branch_ref, validate_repo_path};
use engram_knowledge::{CoreError, CoreResult};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCommit {
    pub branch_ref: String,
    pub oid: String,
    pub object_format: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedFile {
    pub path: PathBuf,
    pub oid: String,
    pub mode: String,
    pub bytes: Vec<u8>,
}
fn git_with_limits(
    repo: &Path,
    args: &[&str],
    limits: &GitResourceLimits,
    output_limit: usize,
) -> CoreResult<Output> {
    let mut command = Command::new("git");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args([
            "--no-lazy-fetch",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.attributesFile=/dev/null",
            "-c",
            "protocol.allow=never",
            "-c",
            "credential.helper=",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_OBJECT_DIRECTORY");
    #[cfg(windows)]
    for variable in ["SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    let out = run_limited(&mut command, limits, output_limit)?;
    if !out.status.success() {
        return Err(CoreError::InvalidRequest {
            reason: "requested committed Git object is unavailable".into(),
        });
    }
    Ok(out)
}
fn text(out: Output) -> CoreResult<String> {
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| CoreError::InvalidRequest {
            reason: "git returned a non-text object identifier".into(),
        })
}
pub fn resolve_local_branch(repo: &Path, branch_ref: &str) -> CoreResult<ResolvedCommit> {
    resolve_local_branch_with_limits(repo, branch_ref, &GitResourceLimits::default())
}
pub fn resolve_local_branch_with_limits(
    repo: &Path,
    branch_ref: &str,
    limits: &GitResourceLimits,
) -> CoreResult<ResolvedCommit> {
    validate_branch_ref(branch_ref)?;
    let root = validate_repo_path(repo)?;
    let oid = text(git_with_limits(
        &root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{branch_ref}^{{commit}}"),
        ],
        limits,
        128,
    )?)?;
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidRequest {
            reason: "Git returned an invalid full commit object ID".into(),
        });
    }
    let object_format = text(git_with_limits(
        &root,
        &["rev-parse", "--show-object-format"],
        limits,
        128,
    )?)?;
    Ok(ResolvedCommit {
        branch_ref: branch_ref.into(),
        oid,
        object_format,
    })
}
pub fn read_resolved_commit(
    repo: &Path,
    commit: &ResolvedCommit,
) -> CoreResult<Vec<CommittedFile>> {
    read_resolved_commit_with_limits(repo, commit, &GitResourceLimits::default())
}
pub fn read_resolved_commit_with_limits(
    repo: &Path,
    commit: &ResolvedCommit,
    limits: &GitResourceLimits,
) -> CoreResult<Vec<CommittedFile>> {
    validate_branch_ref(&commit.branch_ref)?;
    if !matches!(commit.oid.len(), 40 | 64) || !commit.oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidRequest {
            reason: "invalid resolved object identifier".into(),
        });
    }
    let root = validate_repo_path(repo)?;
    let raw = git_with_limits(
        &root,
        &["ls-tree", "-rz", "--full-tree", &commit.oid],
        limits,
        limits.max_stdout_bytes,
    )?
    .stdout;
    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    for item in raw.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let separator =
            item.iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| CoreError::InvalidRequest {
                    reason: "malformed tree entry".into(),
                })?;
        let (meta, tail) = item.split_at(separator);
        let path = &tail[1..];
        let fields = std::str::from_utf8(meta)
            .map_err(|_| CoreError::InvalidRequest {
                reason: "malformed tree metadata".into(),
            })?
            .split_whitespace()
            .collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(CoreError::InvalidRequest {
                reason: "malformed tree metadata".into(),
            });
        }
        let (mode, kind, blob_oid) = (fields[0], fields[1], fields[2]);
        if mode == "120000" || kind == "commit" {
            return Err(CoreError::InvalidRequest {
                reason: "selected tree contains an unsupported symlink or gitlink".into(),
            });
        }
        if kind != "blob" {
            continue;
        }
        let relative = std::str::from_utf8(path).map_err(|_| CoreError::InvalidRequest {
            reason: "committed path is not valid UTF-8".into(),
        })?;
        let relative_path = Path::new(relative);
        if relative_path.is_absolute()
            || relative_path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(CoreError::InvalidRequest {
                reason: "committed path escapes repository root".into(),
            });
        }
        if files.len() >= limits.max_source_files {
            return Err(CoreError::InvalidRequest {
                reason: "source file count exceeded its configured limit".into(),
            });
        }
        for component in relative_path.components() {
            if let std::path::Component::Normal(name) = component {
                super::safety::validate_retained_component(name.to_str().ok_or(
                    CoreError::InvalidRequest {
                        reason: "committed path is not UTF-8".into(),
                    },
                )?)?;
            }
        }
        let size_out = git_with_limits(&root, &["cat-file", "-s", blob_oid], limits, 64)?.stdout;
        let size = std::str::from_utf8(&size_out)
            .ok()
            .and_then(|x| x.trim().parse::<usize>().ok())
            .ok_or(CoreError::InvalidRequest {
                reason: "invalid Git blob size".into(),
            })?;
        if size > limits.max_file_bytes
            || total_bytes
                .checked_add(size)
                .is_none_or(|n| n > limits.max_snapshot_bytes)
        {
            return Err(CoreError::InvalidRequest {
                reason: "source file or aggregate snapshot exceeded its configured byte limit"
                    .into(),
            });
        }
        let bytes = git_with_limits(
            &root,
            &["cat-file", "blob", blob_oid],
            limits,
            limits.max_file_bytes,
        )?
        .stdout;
        if bytes.len() != size {
            return Err(CoreError::InvalidRequest {
                reason: "Git blob size changed while reading".into(),
            });
        }
        total_bytes += size;
        if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n")
            || bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\r\n")
        {
            return Err(CoreError::InvalidRequest {
                reason: "selected tree contains an unsupported Git LFS pointer".into(),
            });
        }
        files.push(CommittedFile {
            path: relative_path.to_path_buf(),
            oid: blob_oid.into(),
            mode: mode.into(),
            bytes,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
pub fn acquire_local_branch(
    repo: &Path,
    branch_ref: &str,
) -> CoreResult<(ResolvedCommit, Vec<CommittedFile>)> {
    acquire_local_branch_with_limits(repo, branch_ref, &GitResourceLimits::default())
}
pub fn acquire_local_branch_with_limits(
    repo: &Path,
    branch_ref: &str,
    limits: &GitResourceLimits,
) -> CoreResult<(ResolvedCommit, Vec<CommittedFile>)> {
    let pinned = resolve_local_branch_with_limits(repo, branch_ref, limits)?;
    let files = read_resolved_commit_with_limits(repo, &pinned, limits)?;
    Ok((pinned, files))
}
