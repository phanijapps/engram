#[cfg(unix)]
#[test]
fn primitive_escaping_path_and_second_writer_reject() {
    use engram_store_helixdb::{config::confined_path, ownership::WriterLock};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("trusted");
    let outside = temporary.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
    let escape_rejected = confined_path(&root, &root.join("escape/missing/deep")).is_err();
    let _first = WriterLock::acquire(&root.join("writer.lock")).unwrap();
    let second_writer_rejected = WriterLock::acquire(&root.join("writer.lock")).is_err();
    assert!(
        escape_rejected && second_writer_rejected,
        "escaping path rejected={escape_rejected}; second writer rejected={second_writer_rejected}"
    );
}

#[cfg(feature = "embedded")]
#[test]
fn primitive_invalid_capacity_scope_and_readonly_never_create_store() {
    use engram_store_helixdb::{HelixStore, codec::Partition, config::EmbeddedConfig};
    let temporary = tempfile::tempdir().unwrap();
    let mut config = EmbeddedConfig {
        trusted_root: temporary.path().into(),
        path: temporary.path().join("absent"),
        read_only: false,
        queue_capacity: 0,
    };
    let mut partition = Partition {
        logical_store: "store".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
    };
    assert!(HelixStore::open_embedded(config.clone(), partition.clone()).is_err());
    assert!(!config.path.exists());
    config.queue_capacity = 2;
    partition.logical_store.clear();
    assert!(HelixStore::open_embedded(config.clone(), partition.clone()).is_err());
    assert!(!config.path.exists());
    partition.logical_store = "store".into();
    config.read_only = true;
    assert!(HelixStore::open_embedded(config.clone(), partition).is_err());
    assert!(!config.path.exists());
}

#[cfg(all(unix, feature = "embedded"))]
#[test]
fn primitive_engine_subtree_and_lock_symlinks_reject_before_outside_access() {
    use engram_store_helixdb::{HelixStore, codec::Partition, config::EmbeddedConfig};
    let temporary = tempfile::tempdir().unwrap();
    let outside = temporary.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), "unchanged").unwrap();
    for leaf in ["engram-v1", "engram-writer.lock"] {
        let path = temporary.path().join(format!("store-{leaf}"));
        std::fs::create_dir(&path).unwrap();
        std::os::unix::fs::symlink(&outside, path.join(leaf)).unwrap();
        let config = EmbeddedConfig {
            trusted_root: path.clone(),
            path,
            read_only: false,
            queue_capacity: 2,
        };
        let partition = Partition {
            logical_store: "store".into(),
            embedding_space: None,
            scope: engram_domain::Scope {
                tenant: "tenant".into(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
        };
        assert!(HelixStore::open_embedded(config, partition).is_err());
    }
    assert_eq!(
        std::fs::read_to_string(outside.join("sentinel")).unwrap(),
        "unchanged"
    );
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 1);
}

#[test]
fn primitive_partition_encoding_preserves_complete_scope_and_none() {
    use engram_store_helixdb::codec::Partition;
    let mut partition = Partition {
        logical_store: "a:b".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "c".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        },
    };
    let none = partition.discriminator().unwrap();
    partition.scope.subject = Some(String::new());
    assert_ne!(none, partition.discriminator().unwrap());
    partition.scope.subject = None;
    partition.logical_store = "a".into();
    partition.scope.tenant = "b:c".into();
    assert_ne!(none, partition.discriminator().unwrap());
    let base = partition.discriminator().unwrap();
    partition.scope.environment = Some("prod".into());
    assert_ne!(base, partition.discriminator().unwrap());
}
