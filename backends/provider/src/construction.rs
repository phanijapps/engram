//! Selected-recipe construction after validation.
use crate::selection::{invalid, missing, normalize_paths, validate, validate_selection};
use crate::{HelixMode, StorageProvider};
use engram_domain::Scope;
use engram_integration::{EngramConfig, EngramProvider};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use std::sync::Arc;
pub fn open(
    config: &EngramConfig,
    scope: &Scope,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    if scope.tenant.is_empty() {
        return Err(invalid("launch scope tenant is required"));
    }
    validate_selection(config)?;
    let normalized = normalize_paths(config)?;
    let config = &normalized;
    let selected = validate(config)?;
    match selected {
        StorageProvider::Sqlite => {
            #[cfg(feature = "sqlite")]
            {
                return engram_backend_sqlite::open(config).map_err(|e| e.to_redacted());
            }
            #[cfg(not(feature = "sqlite"))]
            {
                Err(missing("sqlite"))
            }
        }
        StorageProvider::Pgvector => {
            #[cfg(feature = "pgvector")]
            {
                return engram_backend_pgvector::open(config).map_err(|e| match e {
                    CoreError::Adapter { .. } => CoreError::Adapter {
                        adapter: "pgvector".into(),
                        message: "selected pgvector provider failed to open".into(),
                    },
                    other => other.to_redacted(),
                });
            }
            #[cfg(not(feature = "pgvector"))]
            {
                Err(missing("pgvector"))
            }
        }
        StorageProvider::Helixdb => open_helix(config, scope, authorizer),
    }
}

fn open_helix(
    config: &EngramConfig,
    scope: &Scope,
    authorizer: Arc<dyn PolicyAuthorizer>,
) -> CoreResult<EngramProvider> {
    let h = config
        .helixdb
        .as_ref()
        .ok_or_else(|| invalid("Helix configuration is required"))?;
    #[cfg(any(feature = "helixdb-service", feature = "helixdb-embedded"))]
    let (partition, prepared) = prepare_helix(config, scope)?;
    match h
        .mode
        .ok_or_else(|| invalid("explicit Helix mode is required"))?
    {
        HelixMode::Embedded => {
            #[cfg(feature = "helixdb-embedded")]
            {
                let store = engram_store_helixdb::HelixStore::open_embedded(
                    engram_store_helixdb::config::EmbeddedConfig {
                        trusted_root: config.trusted_root.clone(),
                        path: h
                            .path
                            .clone()
                            .unwrap_or_else(|| config.storage_path.join("helixdb")),
                        read_only: h.read_only,
                        queue_capacity: h.queue_capacity,
                    },
                    partition,
                )
                .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
                engram_backend_helixdb::finish_open_prepared(store, config, authorizer, prepared)
                    .map_err(|e| e.to_redacted())
            }
            #[cfg(not(feature = "helixdb-embedded"))]
            {
                let _ = (scope, authorizer);
                Err(missing("helixdb-embedded"))
            }
        }
        HelixMode::Service => {
            #[cfg(feature = "helixdb-service")]
            {
                let store = engram_store_helixdb::HelixStore::open_service(
                    engram_store_helixdb::config::ServiceConfig {
                        endpoint: h
                            .endpoint
                            .clone()
                            .ok_or_else(|| invalid("Helix endpoint is required"))?,
                        credential_env: h
                            .credential_env
                            .clone()
                            .ok_or_else(|| invalid("Helix credential reference is required"))?,
                        vectors_enabled: config.enable_vector,
                        read_only: h.read_only,
                        timeout: std::time::Duration::from_millis(h.timeout_millis),
                    },
                    partition,
                )
                .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
                engram_backend_helixdb::finish_open_prepared(store, config, authorizer, prepared)
                    .map_err(|e| e.to_redacted())
            }
            #[cfg(not(feature = "helixdb-service"))]
            {
                let _ = (scope, authorizer);
                Err(missing("helixdb-service"))
            }
        }
    }
}

#[cfg(any(
    feature = "helixdb-service",
    feature = "helixdb-embedded",
    feature = "test-controls"
))]
pub(crate) fn prepare_helix(
    config: &EngramConfig,
    scope: &Scope,
) -> CoreResult<(
    engram_store_helixdb::codec::Partition,
    engram_integration::PreparedEmbedding,
)> {
    let h = config
        .helixdb
        .as_ref()
        .ok_or_else(|| invalid("Helix configuration is required"))?;
    let prepared =
        engram_integration::prepare_embedding(&config.embedding_provider, config.enable_vector)?;
    let resolved = prepared
        .provider
        .as_ref()
        .map(|provider| provider.embedding_space());
    if let (Some(explicit), Some(actual)) = (&h.embedding_space, &resolved) {
        if explicit != actual {
            return Err(invalid(
                "embedding provider differs from selected native space",
            ));
        }
    }
    let partition = engram_store_helixdb::codec::Partition {
        logical_store: h.logical_store.clone(),
        scope: scope.clone(),
        embedding_space: h.embedding_space.clone().or(resolved),
    };
    Ok((partition, prepared))
}
