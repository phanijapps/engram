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
fn git(repo: &Path, args: &[&str]) -> CoreResult<Output> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .output()
        .map_err(|_| CoreError::Adapter {
            adapter: "engram-ingest-git".into(),
            message: "git command could not start".into(),
        })?;
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
    validate_branch_ref(branch_ref)?;
    let root = validate_repo_path(repo)?;
    let oid = text(git(
        &root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{branch_ref}^{{commit}}"),
        ],
    )?)?;
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidRequest {
            reason: "Git returned an invalid full commit object ID".into(),
        });
    }
    let object_format = text(git(&root, &["rev-parse", "--show-object-format"])?)?;
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
    validate_branch_ref(&commit.branch_ref)?;
    if !matches!(commit.oid.len(), 40 | 64) || !commit.oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidRequest {
            reason: "invalid resolved object identifier".into(),
        });
    }
    let root = validate_repo_path(repo)?;
    let raw = git(&root, &["ls-tree", "-rz", "--full-tree", &commit.oid])?.stdout;
    let mut files = Vec::new();
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
        let bytes = git(&root, &["cat-file", "blob", blob_oid])?.stdout;
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
    let pinned = resolve_local_branch(repo, branch_ref)?;
    let files = read_resolved_commit(repo, &pinned)?;
    Ok((pinned, files))
}
