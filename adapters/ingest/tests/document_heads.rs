//! Real scanner lifecycle and retained-history tests for current document authority.
use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_knowledge::{DocumentHeadRepository, document_head_retrieval_authorized};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;

fn options(repository: &str, workspace: &str) -> ScanOptions {
    ScanOptions {
        scope: serde_json::from_value(json!({"tenant":"t","workspace":workspace})).unwrap(),
        policy: serde_json::from_value(
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
        )
        .unwrap(),
        actor: serde_json::from_value(json!({"id":"owner","kind":"agent"})).unwrap(),
        source_name: repository.into(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: Default::default(),
    }
}
fn key(opts: &ScanOptions) -> DocumentHeadKey {
    DocumentHeadKey {
        scope: opts.scope.clone(),
        repository: opts.source_name.clone(),
        source_key: "README.md".into(),
    }
}

#[test]
fn scan_refresh_adopts_missing_authority_and_retains_all_versions() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = options("repo", "w");
    let mut previous = Vec::new();
    for body in ["first", "second", "third", "fourth"] {
        std::fs::write(
            root.path().join("README.md"),
            format!("# Same heading\n\n{body} body\n"),
        )
        .unwrap();
        let (summary, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
        assert_eq!(summary.errors, 0, "{summary:?}");
        assert_eq!(summary.ingested, 1);
        opts.manifest = manifest;
        let head = block_on(store.get_document_head(&key(&opts)))
            .unwrap()
            .unwrap();
        assert_eq!(head.state, CodeSourceState::Active);
        for old in &previous {
            assert!(
                block_on(store.current_document_for_chunk(old, &opts.scope))
                    .unwrap()
                    .is_none()
            );
            let retained: bool = store
                .lock()
                .unwrap()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM knowledge_chunks_records WHERE id=?1)",
                    [old.as_str()],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(retained, "historical canonical bytes remain persisted");
        }
        previous.extend(head.chunk_ids);
    }
    let count: i64 = store
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM knowledge_chunks_records", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count as usize, previous.len());
    let (unchanged, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(unchanged.unchanged, 1);
    // Simulate an existing store whose launch manifest predates head adoption.
    store
        .lock()
        .unwrap()
        .execute("DELETE FROM document_heads", [])
        .unwrap();
    let (adopted, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(adopted.errors, 0, "{adopted:?}");
    assert_eq!(
        adopted.ingested, 1,
        "unchanged bytes must be verified when authority is missing"
    );
    assert_eq!(
        block_on(store.get_document_head(&key(&opts)))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Active
    );
}

#[test]
fn same_path_has_independent_repo_scope_and_live_policy_authority() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("README.md"),
        "# Same heading\n\nprivate candidate\n",
    )
    .unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let owner: Requester =
        serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap();
    let outsider: Requester =
        serde_json::from_value(json!({"actor":{"id":"outsider","kind":"agent"}})).unwrap();
    let mut opts = options("first-repo", "w");
    let mut heads = Vec::new();
    for settings in [
        options("first-repo", "w"),
        options("second-repo", "w"),
        options("first-repo", "other"),
    ] {
        let (summary, _) = scan_repository(root.path(), &settings, &store, |_| {}).unwrap();
        assert_eq!(summary.errors, 0, "{summary:?}");
        heads.push(
            block_on(store.get_document_head(&key(&settings)))
                .unwrap()
                .unwrap(),
        );
    }
    assert_ne!(heads[0].document_id, heads[1].document_id);
    assert_ne!(heads[0].document_id, heads[2].document_id);
    opts.policy.visibility = Visibility::Private;
    let (summary, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(summary.errors, 0, "{summary:?}");
    opts.manifest = manifest;
    let private = block_on(store.get_document_head(&key(&opts)))
        .unwrap()
        .unwrap();
    assert!(document_head_retrieval_authorized(
        &private,
        &opts.scope,
        &owner,
        chrono::Utc::now()
    ));
    assert!(!document_head_retrieval_authorized(
        &private,
        &opts.scope,
        &outsider,
        chrono::Utc::now()
    ));
    opts.policy.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
    let (summary, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(summary.errors, 0, "{summary:?}");
    let expired = block_on(store.get_document_head(&key(&opts)))
        .unwrap()
        .unwrap();
    assert!(!document_head_retrieval_authorized(
        &expired,
        &opts.scope,
        &owner,
        chrono::Utc::now()
    ));
    for sibling in heads.iter().skip(1) {
        assert_eq!(
            block_on(store.get_document_head(&sibling.key)).unwrap(),
            Some(sibling.clone())
        );
    }
}

#[test]
fn deleted_generic_document_is_excluded_without_a_host_manifest() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("README.md"),
        "# Heading\n\nobsolete candidate\n",
    )
    .unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let opts = options("repo", "w");
    let (first, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(first.errors, 0);
    let h = block_on(store.get_document_head(&key(&opts)))
        .unwrap()
        .unwrap();
    std::fs::remove_file(root.path().join("README.md")).unwrap();
    let (removed, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(removed.errors, 0);
    assert_eq!(
        block_on(store.get_document_head(&h.key))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Incomplete
    );
    for id in h.chunk_ids {
        assert!(
            block_on(store.current_document_for_chunk(&id, &opts.scope))
                .unwrap()
                .is_none()
        );
    }
}
