use engram_store_helixdb::{HelixStore, codec::Partition, config::EmbeddedConfig, error::Error};
fn partition() -> Partition {
    Partition {
        logical_store: "primitive-lifecycle".into(),
        embedding_space: None,
        scope: engram_domain::Scope {
            tenant: "tenant".into(),
            subject: None,
            workspace: Some("workspace".into()),
            session: None,
            environment: None,
        },
    }
}

#[cfg(feature = "embedded")]
#[test]
fn primitive_embedded_durable_reopen_retained_handles_and_fresh_reader() {
    let temporary = tempfile::tempdir().unwrap();
    let config = EmbeddedConfig {
        trusted_root: temporary.path().into(),
        path: temporary.path().join("store"),
        read_only: false,
        queue_capacity: 2,
    };
    let writer = HelixStore::open_embedded(config.clone(), partition()).unwrap();
    writer.put_sentinel("stable-public-id", "durable").unwrap();
    let retained = writer.clone();
    drop(writer);
    assert_eq!(
        retained
            .get_sentinel("stable-public-id")
            .unwrap()
            .as_deref(),
        Some("durable")
    );
    assert!(matches!(
        HelixStore::open_embedded(config.clone(), partition()),
        Err(Error::Contention)
    ));
    let mut read_config = config.clone();
    read_config.read_only = true;
    let reader = HelixStore::open_embedded(read_config, partition()).unwrap();
    retained
        .put_sentinel("acknowledged-later", "fresh")
        .unwrap();
    assert_eq!(
        reader
            .get_sentinel("acknowledged-later")
            .unwrap()
            .as_deref(),
        Some("fresh")
    );
    assert!(matches!(
        reader.put_sentinel("forbidden", "write"),
        Err(Error::ReadOnly)
    ));
    reader.close().unwrap();
    drop(reader);
    drop(retained); // final-handle Drop must synchronously close/join/release.
    let reopened = HelixStore::open_embedded(config, partition()).unwrap();
    assert_eq!(
        reopened
            .get_sentinel("stable-public-id")
            .unwrap()
            .as_deref(),
        Some("durable")
    );
    assert_eq!(
        reopened
            .get_sentinel("acknowledged-later")
            .unwrap()
            .as_deref(),
        Some("fresh")
    );
    reopened.close().unwrap();
}

#[cfg(feature = "embedded")]
#[test]
fn primitive_incompatible_marker_preflight_preserves_native_epoch_and_marker() {
    let temporary = tempfile::tempdir().unwrap();
    let config = EmbeddedConfig {
        trusted_root: temporary.path().into(),
        path: temporary.path().join("store"),
        read_only: false,
        queue_capacity: 2,
    };
    let mut original = partition();
    original.embedding_space = Some(engram_domain::EmbeddingSpace::new(
        "external",
        "model-one",
        384,
        "query",
        None::<String>,
    ));
    let store = HelixStore::open_embedded(config.clone(), original.clone()).unwrap();
    store.put_sentinel("preserved", "committed").unwrap();
    store.close().unwrap();
    drop(store);
    let before = native_marker_metadata(&config, &original);
    let mut incompatible = original.clone();
    incompatible.embedding_space = Some(engram_domain::EmbeddingSpace::new(
        "external",
        "model-two",
        768,
        "query",
        None::<String>,
    ));
    assert!(matches!(
        HelixStore::open_embedded(config.clone(), incompatible),
        Err(Error::IncompatibleSchema)
    ));
    let after = native_marker_metadata(&config, &original);
    assert_eq!(
        before, after,
        "native writer epoch and persisted marker must remain unchanged"
    );
    let mut read_only = config;
    read_only.read_only = true;
    let reader = HelixStore::open_embedded(read_only, original).unwrap();
    assert_eq!(
        reader.get_sentinel("preserved").unwrap().as_deref(),
        Some("committed")
    );
    reader.close().unwrap();
    // Read validation/native reader leases may have their own housekeeping; no whole-filesystem claim.
}

#[cfg(feature = "embedded")]
fn native_marker_metadata(
    config: &EmbeddedConfig,
    partition: &Partition,
) -> (u64, serde_json::Value) {
    let path = config.path.clone();
    let key = partition.record_key("EngramSchemaV1", "schema").unwrap();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                use helix_db::dsl::prelude::*;
                let source = helix_db::HelixDbSource::Disk {
                    root: path,
                    database: "engram-v1".into(),
                };
                let reader = helix_db::HelixDB::open_reader_for_server(
                    source.clone(),
                    source.embedded_default_config(),
                )
                .await
                .unwrap();
                let epoch = reader.storage_writer_epoch().unwrap().get();
                let request = QueryRequest::read(
                    read_batch()
                        .var_as(
                            "marker",
                            g().n_with_label_where(
                                "EngramSchemaV1",
                                SourcePredicate::eq("key", key),
                            )
                            .value_map(Some(vec!["payload"])),
                        )
                        .returning(["marker"]),
                );
                let bytes = reader
                    .query_json(&serde_json::to_vec(&request).unwrap())
                    .await
                    .unwrap();
                let marker = serde_json::from_slice(&bytes).unwrap();
                reader.close().await.unwrap();
                (epoch, marker)
            })
        })
        .unwrap()
        .join()
        .unwrap()
}
