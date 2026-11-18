use engram_backend_provider::{
    HelixConfig, HelixMode, StorageProvider, open, validate, validate_selection,
};
use engram_domain::*;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
use engram_runtime::{CoreError, CoreResult, PolicyAuthorizer};
use std::{path::Path, sync::Arc};
struct Permit;
impl PolicyAuthorizer for Permit {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
}
fn scope() -> Scope {
    Scope {
        tenant: "selection".into(),
        subject: None,
        workspace: Some("project".into()),
        session: None,
        environment: Some("test".into()),
    }
}
fn config(root: &Path) -> EngramConfig {
    EngramConfig::new(
        root.join("missing/parent/cache"),
        root,
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".into(),
            dimensions: 4,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_enable_vector(false)
}
fn helix(mode: HelixMode) -> HelixConfig {
    HelixConfig {
        mode: Some(mode),
        logical_store: "selection-owned".into(),
        ..Default::default()
    }
}
#[test]
fn default_sqlite_selection_and_deep_validation_have_no_early_writes() {
    let root = tempfile::tempdir().unwrap();
    let cfg = config(root.path());
    if cfg!(feature = "sqlite") {
        assert_eq!(validate_selection(&cfg).unwrap(), StorageProvider::Sqlite);
        assert_eq!(validate(&cfg).unwrap(), StorageProvider::Sqlite);
    } else {
        assert!(
            matches!(validate_selection(&cfg),Err(CoreError::CapabilityUnsupported{reason,..}) if reason.contains("BuildFeatureMissing") && reason.contains("sqlite"))
        );
    }
    assert!(!cfg.storage_path.exists());
}
#[cfg(feature = "sqlite")]
#[test]
fn default_sqlite_opens_only_after_validation_and_preserves_legacy_deep_storage() {
    let root = tempfile::tempdir().unwrap();
    let cfg = config(root.path());
    let provider = open(&cfg, &scope(), Arc::new(Permit)).unwrap();
    assert!(cfg.storage_path.is_dir());
    drop(provider);
    assert!(cfg.storage_path.join("memory.db").is_file());
    assert!(!cfg.storage_path.join("helixdb").exists());
}
#[test]
fn legacy_pg_connection_precedence_never_falls_back_to_sqlite() {
    let root = tempfile::tempdir().unwrap();
    let cfg = config(root.path()).with_pgvector("postgres://user:credential-secret@127.0.0.1:9/db");
    let result = validate_selection(&cfg);
    if cfg!(feature = "pgvector") {
        assert_eq!(result.unwrap(), StorageProvider::Pgvector);
    } else {
        let e = result.unwrap_err();
        assert!(matches!(e, CoreError::CapabilityUnsupported { .. }));
        let text = e.to_redacted().to_string();
        assert!(text.contains("pgvector") && !text.contains("credential-secret"));
    }
    assert!(!cfg.storage_path.exists());
}
#[test]
fn conflicts_and_missing_mode_reject_before_local_access() {
    let root = tempfile::tempdir().unwrap();
    let mut cfg = config(root.path()).with_helixdb(helix(HelixMode::Embedded));
    cfg.storage_provider = Some(StorageProvider::Sqlite);
    assert!(validate_selection(&cfg).is_err());
    cfg.storage_provider = Some(StorageProvider::Helixdb);
    cfg.pgvector_connection_string = Some("postgres://private-record@127.0.0.1/db".into());
    assert!(validate_selection(&cfg).is_err());
    cfg.pgvector_connection_string = None;
    cfg.helixdb.as_mut().unwrap().mode = None;
    assert!(matches!(
        open(&cfg, &scope(), Arc::new(Permit)),
        Err(CoreError::InvalidRequest { .. })
    ));
    assert!(!cfg.storage_path.exists());
}
#[test]
fn legacy_sqlite_entry_cannot_substitute_for_explicit_other_storage() {
    let root = tempfile::tempdir().unwrap();
    for provider in [StorageProvider::Helixdb, StorageProvider::Pgvector] {
        let mut cfg = config(root.path()).with_storage_provider(provider);
        cfg.trusted_root = root.path().join("unavailable-root");
        let error = match engram_integration::EngramProvider::open(&cfg) {
            Err(e) => e,
            Ok(_) => panic!("legacy entry substituted SQLite"),
        };
        assert!(
            matches!(error,CoreError::InvalidRequest{reason} if reason.contains("selected provider factory"))
        );
        assert!(!cfg.storage_path.exists() && !cfg.trusted_root.exists());
    }
    let cfg = config(root.path())
        .with_helixdb(helix(HelixMode::Embedded))
        .with_storage_provider(StorageProvider::Sqlite);
    assert!(engram_integration::EngramProvider::open(&cfg).is_err());
    assert!(!cfg.storage_path.exists());
}
#[test]
fn unpackaged_modes_report_build_feature_missing_before_unsafe_root_access() {
    let root = tempfile::tempdir().unwrap();
    for (mode, packaged, feature) in [
        (
            HelixMode::Embedded,
            cfg!(feature = "helixdb-embedded"),
            "helixdb-embedded",
        ),
        (
            HelixMode::Service,
            cfg!(feature = "helixdb-service"),
            "helixdb-service",
        ),
    ] {
        if packaged {
            continue;
        }
        let mut cfg = config(root.path()).with_helixdb(helix(mode));
        cfg.trusted_root = root.path().join("missing-trusted-root");
        let error = match open(&cfg, &scope(), Arc::new(Permit)) {
            Err(e) => e,
            Ok(_) => panic!("unpackaged mode opened"),
        };
        assert!(
            matches!(error,CoreError::CapabilityUnsupported{reason,..} if reason.contains("BuildFeatureMissing") && reason.contains(feature))
        );
        assert!(!cfg.storage_path.exists() && !cfg.trusted_root.exists());
    }
}
#[test]
fn unsafe_paths_and_embedding_config_reject_without_storage_creation() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut cfg = config(root.path());
    cfg.storage_path = outside.path().join("must-not-exist");
    assert!(open(&cfg, &scope(), Arc::new(Permit)).is_err());
    assert!(!cfg.storage_path.exists());
    cfg = config(root.path());
    cfg.enable_vector = true;
    cfg.embedding_provider.provider_type = "invalid-provider-private-marker".into();
    let result = open(&cfg, &scope(), Arc::new(Permit));
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(!e.to_redacted().to_string().contains("private-marker"));
    }
    assert!(!cfg.storage_path.exists());
}
#[test]
fn portable_profiles_preserve_pg_and_explicit_helix_mode_without_access() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("profile.toml");
    std::fs::write(&path,"[backend]\nkind='postgres'\nconnection_string='postgres://user:credential-secret@localhost/db'\n").unwrap();
    let cfg = EngramConfig::from_profile_file(&path).unwrap();
    assert_eq!(cfg.storage_provider, Some(StorageProvider::Pgvector));
    assert!(cfg.pgvector_connection_string.is_some());
    assert!(!cfg.storage_path.exists());
    std::fs::write(&path,format!("[backend]\nkind='helixdb'\ndata_root='{}'\nmode='embedded'\nlogical_store='profile-owned'\n",root.path().join("profile-cache").display())).unwrap();
    let cfg = EngramConfig::from_profile_file(&path).unwrap();
    assert_eq!(cfg.storage_provider, Some(StorageProvider::Helixdb));
    assert_eq!(cfg.helixdb.unwrap().mode, Some(HelixMode::Embedded));
    assert!(!cfg.storage_path.exists());
    std::fs::write(
        &path,
        "[backend]\nkind='postgres'\nconnection_string='credential-secret\n",
    )
    .unwrap();
    assert!(
        !EngramConfig::from_profile_file(&path)
            .unwrap_err()
            .contains("credential-secret")
    );
}

#[cfg(feature = "helixdb-service")]
#[test]
fn service_configuration_denies_unsafe_origin_missing_credentials_and_disabled_vectors_without_fallback()
 {
    let root = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let missing = format!(
        "ENGRAM_SELECTION_MISSING_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    assert!(std::env::var_os(&missing).is_none());
    let mut h = helix(HelixMode::Service);
    h.endpoint = Some(format!(
        "http://user:credential-secret@{}/",
        listener.local_addr().unwrap()
    ));
    h.credential_env = Some(missing);
    let mut cfg = config(root.path()).with_helixdb(h);
    cfg.enable_vector = true;
    let error = match open(&cfg, &scope(), Arc::new(Permit)) {
        Err(e) => e,
        Ok(_) => panic!("credential-bearing origin opened"),
    };
    assert!(
        !error
            .to_redacted()
            .to_string()
            .contains("credential-secret")
    );
    assert!(!cfg.storage_path.exists());
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    cfg.helixdb.as_mut().unwrap().endpoint =
        Some(format!("http://{}/", listener.local_addr().unwrap()));
    assert!(open(&cfg, &scope(), Arc::new(Permit)).is_err());
    assert!(!cfg.storage_path.exists());
    assert!(
        matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
        "missing credentials must not dispatch"
    );
    cfg.enable_vector = false;
    assert!(validate_selection(&cfg).is_err());
    assert!(!cfg.storage_path.exists());
}

#[cfg(feature = "helixdb-service")]
#[test]
fn selected_service_auth_denial_and_unreachable_error_never_open_substitute_storage() {
    use std::io::{Read, Write};
    let root = tempfile::tempdir().unwrap();
    let reference = std::env::var("HELIX_TEST_CREDENTIAL_ENV")
        .expect("owned actual service credential reference required");
    let secret = std::env::var(&reference).expect("owned actual service credential required");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut buffer = [0u8; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    return 1;
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => return 0,
            }
        }
    });
    let mut h = helix(HelixMode::Service);
    h.endpoint = Some(format!("http://{address}/"));
    h.credential_env = Some(reference);
    h.timeout_millis = 1000;
    let mut cfg = config(root.path()).with_helixdb(h);
    cfg.enable_vector = true;
    let error = match open(&cfg, &scope(), Arc::new(Permit)) {
        Err(e) => e,
        Ok(_) => panic!("authentication denial opened provider"),
    };
    assert_eq!(
        server.join().unwrap(),
        1,
        "configured service must receive the rejected request"
    );
    assert!(!error.to_redacted().to_string().contains(&secret));
    assert!(!cfg.storage_path.exists());
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unreachable = reserved.local_addr().unwrap();
    drop(reserved);
    cfg.helixdb.as_mut().unwrap().endpoint = Some(format!("http://{unreachable}/"));
    assert!(matches!(
        open(&cfg, &scope(), Arc::new(Permit)),
        Err(CoreError::Adapter { .. })
    ));
    assert!(!cfg.storage_path.exists());
    eprintln!(
        "SELECTED_SERVICE_NEGATIVE auth_dispatches=1 missing_credential_dispatches=0 substitute_opens=0 unreachable=REJECTED"
    );
}

#[cfg(feature = "test-controls")]
#[test]
fn fixture_identity_is_nondefault_and_rejects_unbound_values() {
    use engram_backend_provider::test_controls::{FixtureHost, FixtureIdentity};
    assert!(
        FixtureIdentity::new(
            "not-a-commit-id",
            FixtureHost::Rust,
            "/".into(),
            "0".repeat(40),
            "0".repeat(64),
            "/".into()
        )
        .is_err()
    );
    assert!(
        FixtureIdentity::new(
            "00000000-0000-0000-0000-000000000001",
            FixtureHost::Native,
            "/".into(),
            "private-source".into(),
            "0".repeat(64),
            "/".into()
        )
        .is_err()
    );
}

#[cfg(all(feature = "helixdb-embedded", feature = "http-embeddings"))]
mod active_embedding_selection {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    struct OwnedRoot(Option<tempfile::TempDir>);
    impl OwnedRoot {
        fn new() -> Self {
            Self(Some(
                tempfile::Builder::new()
                    .prefix("selection-model-")
                    .tempdir()
                    .unwrap(),
            ))
        }
        fn path(&self) -> &Path {
            self.0.as_ref().unwrap().path()
        }
    }
    impl Drop for OwnedRoot {
        fn drop(&mut self) {
            if std::thread::panicking() {
                if let Some(root) = self.0.take() {
                    eprintln!("MODEL_SELECTION_RETAIN {}", root.keep().display());
                }
            }
        }
    }
    struct Probe {
        storage_existed: bool,
        native_existed: bool,
        request: String,
    }
    // This constructor serves exactly one valid canonical dimension probe. It
    // records premature effects but always answers before assertions run.
    fn fixture(storage: std::path::PathBuf) -> (String, std::thread::JoinHandle<Probe>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(120);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("owned embedding fixture accept failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let probe = (storage.exists(), storage.join("helixdb").exists());
            let mut request = Vec::new();
            loop {
                let mut bytes = [0u8; 4096];
                let n = stream.read(&mut bytes).unwrap();
                assert!(n > 0, "embedding request ended before complete body");
                request.extend_from_slice(&bytes[..n]);
                if let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .expect("embedding request content length");
                    if request.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            let body = r#"{"data":[{"index":0,"embedding":[1.0,0.0,0.0,0.0]}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            stream.flush().unwrap();
            drop(stream);
            drop(listener); // A second constructor cannot consume another probe.
            Probe {
                storage_existed: probe.0,
                native_existed: probe.1,
                request: String::from_utf8(request).unwrap(),
            }
        });
        (endpoint, thread)
    }
    fn active_config(root: &Path, endpoint: String) -> EngramConfig {
        let mut cfg = config(root).with_helixdb(helix(HelixMode::Embedded));
        cfg.enable_vector = true;
        cfg.embedding_provider.provider_type = "openai-compatible".into();
        cfg.embedding_provider.model = "fixture-four-dimensional".into();
        cfg.embedding_provider.dimensions = 0;
        cfg.embedding_provider.options.endpoint = Some(endpoint);
        cfg.embedding_provider.options.request_timeout_secs = 2;
        cfg
    }
    #[test]
    fn healthy_active_embedding_is_prepared_once_before_native_partition() {
        let root = OwnedRoot::new();
        let storage = config(root.path()).storage_path;
        let (endpoint, server) = fixture(storage);
        let cfg = active_config(root.path(), endpoint);
        let result = open(&cfg, &scope(), Arc::new(Permit));
        let probe = server.join().expect("owned embedding fixture closed");
        eprintln!(
            "MODEL_SELECTION_PROBE count=1 storage_at_probe={} native_at_probe={} result={}",
            probe.storage_existed,
            probe.native_existed,
            match &result {
                Ok(_) => "opened".into(),
                Err(e) => e.to_redacted().to_string(),
            }
        );
        assert!(probe.request.contains("embedding dimension probe"));
        let provider = match result {
            Ok(provider) => provider,
            Err(error) => panic!(
                "healthy canonical HTTP embedding must open selected Helix: {}",
                error.to_redacted()
            ),
        };
        let mut resolved = cfg.embedding_provider.clone();
        resolved.dimensions = 4;
        assert_eq!(
            provider
                .vectors()
                .expect("active vector handle")
                .embedding_space(),
            &resolved.embedding_space()
        );
        assert!(
            !probe.storage_existed && !probe.native_existed,
            "embedding preparation must precede cache/native path creation"
        );
        drop(provider);
        eprintln!(
            "MODEL_SELECTION_CLOSED provider_dropped=true http_thread_joined=true exact_identity=canonical_resolved_4D"
        );
    }
    #[test]
    fn explicit_embedding_space_mismatch_rejects_before_storage() {
        let root = OwnedRoot::new();
        let (endpoint, server) = fixture(config(root.path()).storage_path);
        let mut cfg = active_config(root.path(), endpoint);
        let mut incompatible = cfg.embedding_provider.clone();
        incompatible.dimensions = 8;
        cfg.helixdb.as_mut().unwrap().embedding_space = Some(incompatible.embedding_space());
        let result = open(&cfg, &scope(), Arc::new(Permit));
        let probe = server.join().expect("owned embedding fixture closed");
        assert!(
            result.is_err(),
            "explicit incompatible identity must reject"
        );
        assert!(!probe.storage_existed && !probe.native_existed);
        assert!(
            !cfg.storage_path.exists(),
            "incompatible identity must reject before storage"
        );
    }
    #[test]
    fn none_and_disabled_models_do_not_probe_http() {
        let root = OwnedRoot::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        for (name, enabled) in [("none", true), ("disabled", false)] {
            let confined = root.path().join(name);
            std::fs::create_dir(&confined).unwrap();
            let mut cfg = active_config(
                &confined,
                format!("http://{}", listener.local_addr().unwrap()),
            );
            cfg.enable_vector = enabled;
            if name == "none" {
                cfg.embedding_provider.provider_type = "none".into();
            }
            let provider =
                open(&cfg, &scope(), Arc::new(Permit)).expect("storage-only configuration opens");
            assert!(provider.vectors().is_none());
            drop(provider);
            assert!(
                matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
            );
        }
        drop(listener);
    }
}
