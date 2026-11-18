//! Explicit instrumented-fixture initialization; absent from production builds.
use crate::selection::{invalid, missing, normalize_paths, validate, validate_selection};
use crate::{HelixMode, StorageProvider};
use engram_domain::Scope;
use engram_integration::{EngramConfig, EngramProvider};
use engram_runtime::{CoreResult, PolicyAuthorizer};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use engram_store_helixdb::{HelixStore, composition_storage::test_controls::TestControl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureHost {
    Rust,
    Native,
    McpStdio,
    McpOneshot,
}
/// Artifact/source values are supplied by the external fixture recorder;
/// this binding does not itself certify which host artifact was executed.
#[derive(Clone, Debug)]
pub struct FixtureIdentity {
    invocation: uuid::Uuid,
    host: FixtureHost,
    source_root: PathBuf,
    source_revision: String,
    artifact_sha256: String,
    owned_root: PathBuf,
}
impl FixtureIdentity {
    pub fn new(
        invocation: &str,
        host: FixtureHost,
        source_root: PathBuf,
        source_revision: String,
        artifact_sha256: String,
        owned_root: PathBuf,
    ) -> CoreResult<Self> {
        let id =
            uuid::Uuid::parse_str(invocation).map_err(|_| invalid("invalid fixture invocation"))?;
        let hex = |s: &str, n| {
            s.len() == n
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if id.to_string() != invocation
            || !hex(&source_revision, 40)
            || !hex(&artifact_sha256, 64)
            || !source_root.is_absolute()
            || !owned_root.is_absolute()
        {
            return Err(invalid("invalid fixture source/artifact/root binding"));
        }
        Ok(Self {
            invocation: id,
            host,
            source_root,
            source_revision,
            artifact_sha256,
            owned_root,
        })
    }
    pub fn invocation(&self) -> uuid::Uuid {
        self.invocation
    }
    pub fn host(&self) -> FixtureHost {
        self.host
    }
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }
    pub fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }
}
#[derive(Clone, Debug)]
pub struct FixtureBinding {
    pub identity: FixtureIdentity,
    pub backend: String,
    pub mode: String,
    pub logical_store: String,
    pub scope: Scope,
}
pub struct InstrumentedProvider {
    pub provider: EngramProvider,
    pub control: Arc<TestControl>,
    pub binding: FixtureBinding,
}
/// Owns a fresh native store, attaches its retained native control before
/// composition creates any aliases, then delegates the unchanged recipe.
pub fn open(
    config: &EngramConfig,
    scope: &Scope,
    authorizer: Arc<dyn PolicyAuthorizer>,
    identity: FixtureIdentity,
) -> CoreResult<InstrumentedProvider> {
    if validate_selection(config)? != StorageProvider::Helixdb {
        return Err(invalid(
            "fixture controls require explicitly selected Helix storage",
        ));
    }
    let config = normalize_paths(config)?;
    validate(&config)?;
    let compiled_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|_| invalid("fixture source root unavailable"))?;
    if identity
        .source_root
        .canonicalize()
        .map_err(|_| invalid("fixture source root unavailable"))?
        != compiled_root
        || identity
            .owned_root
            .canonicalize()
            .map_err(|_| invalid("fixture owned root unavailable"))?
            != config
                .trusted_root
                .canonicalize()
                .map_err(|_| invalid("fixture trusted root unavailable"))?
    {
        return Err(invalid("fixture source or owned root mismatch"));
    }
    let h = config
        .helixdb
        .as_ref()
        .ok_or_else(|| invalid("Helix configuration required"))?;
    if h.read_only || scope.tenant.is_empty() {
        return Err(invalid(
            "fixture mutation controls require an owned writable scope",
        ));
    }
    let (partition, prepared) = crate::construction::prepare_helix(&config, scope)?;
    let opened: engram_store_helixdb::error::Result<HelixStore> =
        match h.mode.ok_or_else(|| invalid("Helix mode required"))? {
            HelixMode::Embedded => {
                #[cfg(feature = "helixdb-embedded")]
                {
                    HelixStore::open_embedded(
                        engram_store_helixdb::config::EmbeddedConfig {
                            trusted_root: config.trusted_root.clone(),
                            path: h
                                .path
                                .clone()
                                .unwrap_or_else(|| config.storage_path.join("helixdb")),
                            read_only: false,
                            queue_capacity: h.queue_capacity,
                        },
                        partition,
                    )
                }
                #[cfg(not(feature = "helixdb-embedded"))]
                {
                    let _ = partition;
                    return Err(missing("helixdb-embedded"));
                }
            }
            HelixMode::Service => {
                #[cfg(feature = "helixdb-service")]
                {
                    HelixStore::open_service(
                        engram_store_helixdb::config::ServiceConfig {
                            endpoint: h
                                .endpoint
                                .clone()
                                .ok_or_else(|| invalid("Helix endpoint required"))?,
                            credential_env: h
                                .credential_env
                                .clone()
                                .ok_or_else(|| invalid("Helix credential reference required"))?,
                            vectors_enabled: config.enable_vector,
                            read_only: false,
                            timeout: std::time::Duration::from_millis(h.timeout_millis),
                        },
                        partition,
                    )
                }
                #[cfg(not(feature = "helixdb-service"))]
                {
                    let _ = partition;
                    return Err(missing("helixdb-service"));
                }
            }
        };
    let mut store = opened.map_err(engram_store_helixdb::composition_storage::native_core_error)?;
    let actual = {
        let session = store.native_session(scope)?;
        session.binding().clone()
    };
    let control = store
        .instrument_native_test_control()
        .map_err(engram_store_helixdb::composition_storage::native_core_error)?;
    let binding = FixtureBinding {
        identity,
        backend: actual.backend,
        mode: actual.mode,
        logical_store: actual.store,
        scope: actual.scope,
    };
    let provider =
        engram_backend_helixdb::finish_open_prepared(store, &config, authorizer, prepared)?;
    Ok(InstrumentedProvider {
        provider,
        control,
        binding,
    })
}
