//! Explicit mode, partition and confined local configuration.
use crate::error::{Error, Result};
use std::path::{Component, Path, PathBuf};

pub fn confined_path(root: &Path, path: &Path) -> Result<PathBuf> {
    if !root.is_absolute()
        || !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::InvalidConfig(
            "storage paths must be absolute without traversal",
        ));
    }
    let root = root
        .canonicalize()
        .map_err(|_| Error::InvalidConfig("trusted root is unavailable"))?;
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        // symlink_metadata distinguishes dangling symlinks from absent children.
        if std::fs::symlink_metadata(existing).is_ok() {
            return Err(Error::InvalidConfig("dangling storage symlink"));
        }
        missing.push(
            existing
                .file_name()
                .ok_or(Error::InvalidConfig("invalid storage path"))?,
        );
        existing = existing
            .parent()
            .ok_or(Error::InvalidConfig("invalid storage ancestor"))?;
    }
    let mut confined = existing
        .canonicalize()
        .map_err(|_| Error::InvalidConfig("storage ancestor unavailable"))?;
    if !confined.starts_with(&root) {
        return Err(Error::InvalidConfig("storage path escapes trusted root"));
    }
    for name in missing.iter().rev() {
        confined.push(name);
    }
    Ok(confined)
}

#[derive(Clone)]
pub struct EmbeddedConfig {
    pub trusted_root: PathBuf,
    pub path: PathBuf,
    pub read_only: bool,
    pub queue_capacity: usize,
}

#[derive(Clone)]
pub struct ServiceConfig {
    pub endpoint: String,
    pub credential_env: String,
    pub vectors_enabled: bool,
    pub read_only: bool,
    pub timeout: std::time::Duration,
}
