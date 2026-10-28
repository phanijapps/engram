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
    let git = p.join(".git");
    if !git.exists() {
        return Err(CoreError::InvalidRequest {
            reason: "registered local repository has no .git directory".into(),
        });
    }
    if p.join(".git/objects/info/alternates").exists() {
        return Err(CoreError::InvalidRequest {
            reason: "repository object alternates are unsupported".into(),
        });
    }
    Ok(p)
}
