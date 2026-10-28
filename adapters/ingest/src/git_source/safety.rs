use engram_knowledge::{CoreError, CoreResult};
use std::path::{Path, PathBuf};

pub fn validate_branch_ref(value: &str) -> CoreResult<()> {
    if !value.starts_with("refs/heads/")
        || value.len() <= 11
        || value.contains("..")
        || value.contains("@{")
        || value.contains("//")
        || value.ends_with('/')
        || value.ends_with('.')
        || value
            .split('/')
            .any(|p| p.starts_with('.') || p.ends_with(".lock"))
        || value
            .chars()
            .any(|c| c.is_control() || " ~^:?*[\\".contains(c))
    {
        return Err(CoreError::InvalidRequest {
            reason: "branch must be an exact canonical refs/heads name".into(),
        });
    }
    Ok(())
}
pub fn validate_repo_path(path: &Path) -> CoreResult<PathBuf> {
    let p = std::fs::canonicalize(path).map_err(|_| CoreError::InvalidRequest {
        reason: "registered repository path is unavailable".into(),
    })?;
    if !p.is_dir() {
        return Err(CoreError::InvalidRequest {
            reason: "registered repository path is not a directory".into(),
        });
    }
    let git_dir = if p.join(".git").is_dir() {
        p.join(".git")
    } else if p.join(".git").is_file() {
        return Err(CoreError::InvalidRequest {
            reason: "linked worktree metadata is unsupported".into(),
        });
    } else if p.join("HEAD").is_file() && p.join("objects").is_dir() && p.join("config").is_file() {
        p.clone()
    } else {
        return Err(CoreError::InvalidRequest {
            reason: "registered path is not a supported Git repository".into(),
        });
    };
    if git_dir.join("objects/info/alternates").exists() {
        return Err(CoreError::InvalidRequest {
            reason: "repository object alternates are unsupported".into(),
        });
    }
    Ok(p)
}
