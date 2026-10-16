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
fn first_legacy_refresh_preserves_sources_and_graph_facts_before_cleanup() {
    assert_first_legacy_cleanup_preserves_history(false);
}

#[test]
fn first_legacy_removed_file_preserves_graph_facts_before_cleanup() {
    assert_first_legacy_cleanup_preserves_history(true);
}

#[test]
fn failed_removed_head_fence_retains_discovered_hash_for_retry() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = options("retry-repository", "w");
    let path = root.path().join("README.md");
    std::fs::write(&path, "# PaymentGateway\n\nCustomerRegistry.\n").unwrap();
    let (first, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(first.errors, 0);
    let hash = manifest["README.md"].clone();
    assert!(opts.manifest.is_empty());
    std::fs::remove_file(path).unwrap();
    store.lock().unwrap().execute_batch("CREATE TRIGGER injected_removed_fence BEFORE UPDATE ON document_heads WHEN NEW.state='incomplete' BEGIN SELECT RAISE(ABORT,'injected failed fence'); END;").unwrap();
    let (failed, retry) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(failed.errors, 1);
    assert_eq!(
        retry.get("README.md"),
        Some(&hash),
        "discovered failed removal needs a retry hash"
    );
    store
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER injected_removed_fence")
        .unwrap();
    opts.manifest = retry;
    let (retried, after) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(retried.errors, 0);
    assert!(!after.contains_key("README.md"));
}

fn assert_first_legacy_cleanup_preserves_history(removed: bool) {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = options("legacy-repository", "w");
    let path = root.path().join("README.md");
    std::fs::write(
        &path,
        "# PaymentGateway\n\nInvoiceProcessor uses CustomerRegistry.\n",
    )
    .unwrap();
    let (first, prior_manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(first.errors, 0);
    let tables = [
        ("knowledge_sources", "source"),
        ("knowledge_documents", "document"),
        ("knowledge_chunks_records", "chunk"),
        ("knowledge_graphs_records", "graph"),
        ("knowledge_entities_records", "entity"),
        ("knowledge_relationships_records", "relationship"),
        ("knowledge_unresolved_refs_records", "unresolved_reference"),
    ];
    let mut old = Vec::new();
    {
        let conn = store.lock().unwrap();
        for (table, kind) in tables {
            let mut stmt = conn
                .prepare(&format!("SELECT id,record_json FROM {table}"))
                .unwrap();
            for row in stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .unwrap()
            {
                let (id, raw) = row.unwrap();
                old.push((table, kind, id, raw));
            }
        }
        // Simulate an actual pre-head retained store, keeping canonical data.
        conn.execute_batch("DELETE FROM document_head_records;DELETE FROM document_heads;DELETE FROM document_head_attempts;DELETE FROM document_head_history;DELETE FROM canonical_document_history;").unwrap();
    }
    assert!(old.iter().any(|(_, kind, _, _)| *kind == "graph"));
    assert!(old.iter().any(|(_, kind, _, _)| *kind == "relationship"));
    if removed {
        std::fs::remove_file(&path).unwrap();
        opts.manifest = prior_manifest;
    } else {
        std::fs::write(
            &path,
            "# PaymentGateway\n\nInvoiceProcessor validates CustomerRegistry before payment.\n",
        )
        .unwrap();
    }
    let (second, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(second.errors, 0, "{second:?}");
    let conn = store.lock().unwrap();
    for (table, kind, id, raw) in old {
        let unchanged: bool = conn
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1 AND record_json=?2)"),
                [id.as_str(), raw.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        let archived:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM canonical_document_history WHERE kind=?1 AND record_id=?2 AND record_json=?3)",[kind,id.as_str(),raw.as_str()],|row|row.get(0)).unwrap();
        assert!(
            unchanged || archived,
            "old {kind} {id} lost before first attribution"
        );
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
