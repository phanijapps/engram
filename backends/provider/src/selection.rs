//! Pure provider selection and confined path validation.
use crate::{HelixMode, StorageProvider};
use engram_integration::EngramConfig;
use engram_runtime::{CoreError, CoreResult};
use std::path::{Component, Path, PathBuf};
pub(crate) fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
pub(crate) fn missing(feature: &str) -> CoreError {
    CoreError::CapabilityUnsupported {
        capability: "selected_storage_provider".into(),
        reason: format!(
            "{}: enable {feature}",
            engram_integration::capability::BUILD_FEATURE_MISSING
        ),
    }
}
/// Pure selection/shape/feature validation. No filesystem, credentials, models,
/// native engine or network is accessed by this function.
pub fn validate_selection(config: &EngramConfig) -> CoreResult<StorageProvider> {
    let inferred = if config.helixdb.is_some() {
        StorageProvider::Helixdb
    } else if config.pgvector_connection_string.is_some() {
        StorageProvider::Pgvector
    } else {
        StorageProvider::Sqlite
    };
    let selected = config.storage_provider.unwrap_or(inferred);
    if (config.helixdb.is_some() && selected != StorageProvider::Helixdb)
        || (config.pgvector_connection_string.is_some() && selected != StorageProvider::Pgvector)
        || (config.helixdb.is_some() && config.pgvector_connection_string.is_some())
    {
        return Err(invalid("conflicting storage provider configuration"));
    }
    match selected {
        StorageProvider::Sqlite => {
            if !cfg!(feature = "sqlite") {
                return Err(missing("sqlite"));
            }
        }
        StorageProvider::Pgvector => {
            if !cfg!(feature = "pgvector") {
                return Err(missing("pgvector"));
            }
            let value = config
                .pgvector_connection_string
                .as_deref()
                .ok_or_else(|| invalid("pgvector connection is required"))?;
            let parsed =
                url::Url::parse(value).map_err(|_| invalid("invalid pgvector connection"))?;
            if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none()
            {
                return Err(invalid("invalid pgvector connection"));
            }
            if config.rebuild_vectors {
                return Err(invalid("pgvector does not support rebuild_vectors"));
            }
        }
        StorageProvider::Helixdb => {
            let h = config
                .helixdb
                .as_ref()
                .ok_or_else(|| invalid("Helix configuration is required"))?;
            let mode = h
                .mode
                .ok_or_else(|| invalid("explicit Helix mode is required"))?;
            match mode {
                HelixMode::Service if !cfg!(feature = "helixdb-service") => {
                    return Err(missing("helixdb-service"));
                }
                HelixMode::Embedded if !cfg!(feature = "helixdb-embedded") => {
                    return Err(missing("helixdb-embedded"));
                }
                _ => {}
            }
            if h.logical_store.trim().is_empty()
                || h.logical_store.len() > 256
                || h.logical_store.contains('\0')
            {
                return Err(invalid("invalid Helix logical store"));
            }
            if !(1..=1024).contains(&h.queue_capacity) || !(1..=120_000).contains(&h.timeout_millis)
            {
                return Err(invalid("invalid Helix runtime bounds"));
            }
            if config.rebuild_vectors {
                return Err(invalid("Helix does not support rebuild_vectors"));
            }
            if h.embedding_space.as_ref().is_some_and(|s| {
                s.dimensions == 0
                    || s.provider.is_empty()
                    || s.model.is_empty()
                    || s.prompt_profile.is_empty()
            }) {
                return Err(invalid("invalid Helix embedding space"));
            }
            match mode {
                HelixMode::Embedded if h.endpoint.is_some() || h.credential_env.is_some() => {
                    return Err(invalid(
                        "service settings conflict with embedded Helix mode",
                    ));
                }
                HelixMode::Service => {
                    if h.path.is_some() {
                        return Err(invalid("embedded path conflicts with service Helix mode"));
                    }
                    let u = url::Url::parse(
                        h.endpoint
                            .as_deref()
                            .ok_or_else(|| invalid("Helix service endpoint is required"))?,
                    )
                    .map_err(|_| invalid("invalid Helix service endpoint"))?;
                    if !matches!(u.scheme(), "http" | "https")
                        || u.host_str().is_none()
                        || !u.username().is_empty()
                        || u.password().is_some()
                        || u.query().is_some()
                        || u.fragment().is_some()
                        || u.path() != "/"
                    {
                        return Err(invalid(
                            "Helix service endpoint must be a fixed origin without credentials",
                        ));
                    }
                    let key = h.credential_env.as_deref().ok_or_else(|| {
                        invalid("Helix credential environment reference is required")
                    })?;
                    if key.is_empty()
                        || key.len() > 128
                        || !key.bytes().enumerate().all(|(i, b)| {
                            b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit()
                        })
                    {
                        return Err(invalid("invalid Helix credential environment reference"));
                    }
                    if !config.enable_vector {
                        return Err(invalid("PLT-001 prohibits disabled-vector service access"));
                    }
                }
                _ => {}
            }
        }
    }
    if config.enable_vector {
        if !matches!(
            config.embedding_provider.provider_type.as_str(),
            "none" | "fastembed" | "ollama" | "openai-compatible"
        ) {
            return Err(invalid("unknown embedding provider"));
        }
        config
            .embedding_provider
            .validate()
            .map_err(|_| invalid("invalid embedding configuration"))?;
        if config.embedding_provider.provider_type == "fastembed"
            && config.embedding_provider.dimensions == 0
            && !cfg!(feature = "fastembed")
        {
            return Err(missing("fastembed"));
        }
        if matches!(
            config.embedding_provider.provider_type.as_str(),
            "ollama" | "openai-compatible"
        ) && !cfg!(feature = "http-embeddings")
        {
            return Err(missing("http-embeddings"));
        }
    }
    if config.storage_path.as_os_str().is_empty()
        || config.trusted_root.as_os_str().is_empty()
        || config
            .storage_path
            .components()
            .chain(config.trusted_root.components())
            .any(|p| matches!(p, Component::ParentDir))
    {
        return Err(invalid("invalid confined storage configuration"));
    }
    Ok(selected)
}
/// Existing ancestor used for the legacy deep-storage launch default. Call
/// `validate_selection` first; this function performs metadata reads only.
pub fn infer_trusted_root(storage: &Path) -> CoreResult<PathBuf> {
    if storage
        .components()
        .any(|p| matches!(p, Component::ParentDir))
    {
        return Err(invalid("storage path contains traversal"));
    }
    let absolute = if storage.is_absolute() {
        storage.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| invalid("working directory unavailable"))?
            .join(storage)
    };
    let mut ancestor = absolute
        .parent()
        .ok_or_else(|| invalid("storage path has no parent"))?;
    while !ancestor.exists() {
        if std::fs::symlink_metadata(ancestor).is_ok() {
            return Err(invalid("storage ancestor is a dangling symlink"));
        }
        ancestor = ancestor
            .parent()
            .ok_or_else(|| invalid("storage ancestor unavailable"))?;
    }
    if !ancestor.is_dir() {
        return Err(invalid("storage ancestor is not a directory"));
    }
    ancestor
        .canonicalize()
        .map_err(|_| invalid("storage ancestor unavailable"))
}
pub fn validate(config: &EngramConfig) -> CoreResult<StorageProvider> {
    let selected = validate_selection(config)?;
    config
        .validate()
        .map_err(|_| invalid("storage path or trusted root validation failed"))?;
    if let Some(h) = &config.helixdb {
        if let Some(path) = &h.path {
            let mut local = config.clone();
            local.storage_path = path.clone();
            local
                .validate()
                .map_err(|_| invalid("Helix path or trusted root validation failed"))?;
        }
    }
    Ok(selected)
}
pub(crate) fn normalize_paths(config: &EngramConfig) -> CoreResult<EngramConfig> {
    let cwd = std::env::current_dir().map_err(|_| invalid("working directory unavailable"))?;
    let absolute = |p: &Path| -> PathBuf {
        (if p.is_absolute() {
            p.to_owned()
        } else {
            cwd.join(p)
        })
        .components()
        .collect()
    };
    let mut normalized = config.clone();
    normalized.storage_path = absolute(&config.storage_path);
    normalized.trusted_root = absolute(&config.trusted_root);
    if let Some(h) = &mut normalized.helixdb {
        if let Some(path) = &h.path {
            h.path = Some(absolute(path));
        }
    }
    Ok(normalized)
}
