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

/// Reject filesystem-special components before any source content is persisted.
pub(crate) fn validate_retained_component(name: &str) -> CoreResult<()> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|tail| {
            matches!(
                tail,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.eq_ignore_ascii_case(".git")
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        || reserved
    {
        return Err(CoreError::InvalidRequest {
            reason: "committed path contains a nonportable or reserved component".into(),
        });
    }
    Ok(())
}
