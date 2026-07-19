//! Integration tests for the knowledge-graph-retraction spec (T3, T4, T5).
//!
//! Verifies:
//! - Re-ingesting the same `(stable_source_key, path)` with changed content
//!   deletes the prior graph before writing the new one — no orphaned prior
//!   graph records remain (AC-4 / T3).
//! - A path present in the prior manifest but absent from the current scan has
//!   its graph deleted after the scan (AC-5 / T4).
//! - The per-source `EntityKind::Repository` node survives while at least one
//!   document graph remains for the key and is deleted once the last one is
//!   removed (AC-6 / T5).
//!
//! All tests use non-git directories so the stable-source-key is derived from
//! the (normalized) source name — deterministic without needing a git remote.

use std::fs;

use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-retract".to_owned(),
        subject: None,
        workspace: Some("test".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("retract-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

/// Creates `ScanOptions` with the given prior manifest and source_name.
fn scan_opts(
    source_name: &str,
    manifest: std::collections::HashMap<String, String>,
) -> ScanOptions {
    ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: source_name.to_owned(),
        max_bytes: 0,
        manifest,
    }
}

// ---------------------------------------------------------------------------
// T3: Re-ingest replaces a changed file — no orphaned prior graph records
// ---------------------------------------------------------------------------

#[test]
fn re_ingest_replaces_changed_file_graph() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-t3-{}-{}",
        std::process::id(),
        "reingest"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    // Stable source key for a non-git directory is the normalized source name.
    let source_name = "retract-test-source";
    let source_key = source_name; // stable_source_key(None, source_name) == source_name.lowercase()

    // First scan: file contains "alpha".
    fs::write(root.join("a.rs"), "fn alpha() {}\n").expect("write a.rs v1");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    let graphs_after_scan1 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after scan 1");
    let graph_count_1 = graphs_after_scan1.len();
    assert!(graph_count_1 >= 1, "at least one graph after first scan");

    // Verify "alpha" entity exists.
    let entities_1 = block_on(store.list_entities(&scope())).expect("list entities 1");
    let has_alpha = entities_1.iter().any(|e| e.name == "alpha");
    assert!(has_alpha, "entity 'alpha' must exist after first scan");

    // Second scan: change the file so "alpha" → "beta".
    fs::write(root.join("a.rs"), "fn beta() {}\n").expect("write a.rs v2");
    let (_, _manifest2) =
        scan_repository(&root, &scan_opts(source_name, manifest1), &store, |_| {}).expect("scan 2");

    // Exactly one graph for (source_key, "a.rs") after reconcile.
    let graphs_after_scan2 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after scan 2");
    assert_eq!(
        graphs_after_scan2.len(),
        graph_count_1,
        "graph count must be the same (old replaced by new, not duplicated)"
    );

    // "alpha" is gone; "beta" is present.
    let entities_2 = block_on(store.list_entities(&scope())).expect("list entities 2");
    assert!(
        !entities_2.iter().any(|e| e.name == "alpha"),
        "entity 'alpha' must be gone after re-ingest"
    );
    assert!(
        entities_2.iter().any(|e| e.name == "beta"),
        "entity 'beta' must exist after re-ingest"
    );

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// T4: Removed-path convergence — graph deleted when file absent from new scan
// ---------------------------------------------------------------------------

#[test]
fn removed_file_graph_is_deleted_after_rescan() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-t4-{}-{}",
        std::process::id(),
        "removed"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    let source_name = "retract-t4-source";
    let source_key = source_name;

    // First scan: two files a.rs and b.rs.
    fs::write(root.join("a.rs"), "fn alpha() {}\n").expect("write a.rs");
    fs::write(root.join("b.rs"), "fn beta() {}\n").expect("write b.rs");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    let graphs1 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after scan 1");
    assert_eq!(
        graphs1.len(),
        2,
        "two graphs after first scan (one per file)"
    );

    // Second scan: remove a.rs — only b.rs is present.
    fs::remove_file(root.join("a.rs")).expect("remove a.rs");
    let (_, manifest2) =
        scan_repository(&root, &scan_opts(source_name, manifest1), &store, |_| {}).expect("scan 2");

    // Only one graph remains: for b.rs.
    let graphs2 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after scan 2");
    assert_eq!(
        graphs2.len(),
        1,
        "one graph must remain after a.rs is removed"
    );

    // The remaining graph is for b.rs.
    let remaining_path = graphs2[0]
        .metadata
        .as_ref()
        .and_then(|m| m.get("path"))
        .and_then(|v| v.as_str());
    assert_eq!(
        remaining_path,
        Some("b.rs"),
        "remaining graph must be for b.rs"
    );

    // a.rs is gone from the emitted manifest.
    assert!(
        !manifest2.contains_key("a.rs"),
        "removed file must not appear in the emitted manifest"
    );
    assert!(
        manifest2.contains_key("b.rs"),
        "surviving file must appear in the emitted manifest"
    );

    // Entities from a.rs are gone; entities from b.rs survive.
    let entities = block_on(store.list_entities(&scope())).expect("list entities");
    assert!(
        !entities.iter().any(|e| e.name == "alpha"),
        "entity 'alpha' from a.rs must be gone"
    );
    assert!(
        entities.iter().any(|e| e.name == "beta"),
        "entity 'beta' from b.rs must survive"
    );

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// T5: Repository-node lifecycle
// ---------------------------------------------------------------------------

#[test]
fn repository_node_deleted_when_last_doc_graph_removed() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-t5-{}-{}",
        std::process::id(),
        "repodel"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    let source_name = "retract-t5-del-source";
    let source_key = source_name;

    // First scan: one file.
    fs::write(root.join("a.rs"), "fn alpha() {}\n").expect("write a.rs");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    // Repository node exists after first scan.
    let entities_1 = block_on(store.list_entities(&scope())).expect("list entities 1");
    let repo_nodes_1: Vec<_> = entities_1
        .iter()
        .filter(|e| e.kind == EntityKind::Repository)
        .collect();
    assert_eq!(
        repo_nodes_1.len(),
        1,
        "Repository node must exist after first scan"
    );

    // Second scan: remove a.rs — now no files remain.
    fs::remove_file(root.join("a.rs")).expect("remove a.rs");
    let (_, _manifest2) =
        scan_repository(&root, &scan_opts(source_name, manifest1), &store, |_| {}).expect("scan 2");

    // No document graphs remain.
    let graphs2 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list graphs after del");
    assert!(
        graphs2.is_empty(),
        "no document graphs must remain after all files removed"
    );

    // Repository node is deleted.
    let entities_2 = block_on(store.list_entities(&scope())).expect("list entities 2");
    let repo_nodes_2: Vec<_> = entities_2
        .iter()
        .filter(|e| e.kind == EntityKind::Repository)
        .collect();
    assert!(
        repo_nodes_2.is_empty(),
        "Repository node must be deleted when last document graph is removed"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn repository_node_persists_while_doc_graphs_remain() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-t5-{}-{}",
        std::process::id(),
        "repopersist"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    let source_name = "retract-t5-persist-source";
    let source_key = source_name;

    // First scan: two files.
    fs::write(root.join("a.rs"), "fn alpha() {}\n").expect("write a.rs");
    fs::write(root.join("b.rs"), "fn beta() {}\n").expect("write b.rs");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    // Second scan: remove a.rs — b.rs remains.
    fs::remove_file(root.join("a.rs")).expect("remove a.rs");
    let (_, _manifest2) =
        scan_repository(&root, &scan_opts(source_name, manifest1), &store, |_| {}).expect("scan 2");

    // One document graph remains (b.rs).
    let graphs = block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list graphs");
    assert_eq!(graphs.len(), 1, "b.rs graph must remain");

    // Repository node still exists.
    let entities = block_on(store.list_entities(&scope())).expect("list entities");
    let repo_nodes: Vec<_> = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Repository)
        .collect();
    assert_eq!(
        repo_nodes.len(),
        1,
        "Repository node must persist while at least one document graph remains"
    );

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// FIX 2: Pre-pass delete failure surfaces in summary.errors and does not
//         duplicate graphs or drop the path from the manifest.
// ---------------------------------------------------------------------------

/// A store wrapper whose `delete_graph` always returns an error.
/// All other methods delegate to the inner `SqlKnowledgeStore`.
struct FailingDeleteStore {
    inner: SqlKnowledgeStore,
}

#[async_trait::async_trait]
impl engram_knowledge::KnowledgeRepository for FailingDeleteStore {
    async fn put_source(
        &self,
        source: engram_domain::KnowledgeSource,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeSource> {
        self.inner.put_source(source).await
    }
    async fn put_document(
        &self,
        document: engram_domain::SourceDocument,
    ) -> engram_knowledge::CoreResult<engram_domain::SourceDocument> {
        self.inner.put_document(document).await
    }
    async fn put_chunk(
        &self,
        chunk: engram_domain::KnowledgeChunk,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeChunk> {
        self.inner.put_chunk(chunk).await
    }
    async fn get_chunk(
        &self,
        id: &engram_domain::ChunkId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeChunk>> {
        self.inner.get_chunk(id, scope).await
    }
    async fn put_entity(
        &self,
        entity: engram_domain::KnowledgeEntity,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeEntity> {
        self.inner.put_entity(entity).await
    }
    async fn put_relationship(
        &self,
        relationship: engram_domain::KnowledgeRelationship,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeRelationship> {
        self.inner.put_relationship(relationship).await
    }
    async fn delete_entity(
        &self,
        id: &engram_domain::EntityId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<bool> {
        self.inner.delete_entity(id, scope).await
    }
}

#[async_trait::async_trait]
impl engram_knowledge::KnowledgeGraphRepository for FailingDeleteStore {
    async fn put_graph(
        &self,
        graph: engram_domain::KnowledgeGraph,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeGraph> {
        self.inner.put_graph(graph).await
    }
    async fn get_graph(
        &self,
        id: &engram_domain::KnowledgeGraphId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeGraph>> {
        self.inner.get_graph(id, scope).await
    }
    async fn neighbors(
        &self,
        graph_id: &engram_domain::KnowledgeGraphId,
        node_id: &engram_domain::EntityId,
        scope: &engram_domain::Scope,
        limit: Option<u32>,
    ) -> engram_knowledge::CoreResult<Vec<engram_domain::KnowledgeRelationship>> {
        self.inner.neighbors(graph_id, node_id, scope, limit).await
    }
    /// Always fails — forces the pre-pass delete path to record an error.
    async fn delete_graph(
        &self,
        _id: &engram_domain::KnowledgeGraphId,
        _scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<bool> {
        Err(engram_knowledge::CoreError::Adapter {
            adapter: "test-failing-delete".to_owned(),
            message: "forced delete failure for FIX 2 test".to_owned(),
        })
    }
    async fn list_graphs_by_source(
        &self,
        scope: &engram_domain::Scope,
        stable_source_key: &str,
    ) -> engram_knowledge::CoreResult<Vec<engram_domain::KnowledgeGraph>> {
        self.inner
            .list_graphs_by_source(scope, stable_source_key)
            .await
    }
}

#[test]
fn pre_pass_delete_failure_surfaces_error_no_duplicate_no_drop() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-fix2-{}-{}",
        std::process::id(),
        "delfail"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    let source_name = "retract-fix2-source";
    let source_key = source_name;

    // First scan with a real store: creates a graph for a.rs.
    fs::write(root.join("a.rs"), "fn alpha() {}\n").expect("write a.rs v1");
    let real_store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &real_store,
        |_| {},
    )
    .expect("scan 1");

    let graphs_after_scan1 =
        block_on(real_store.list_graphs_by_source(&scope(), source_key)).expect("list after 1");
    assert_eq!(graphs_after_scan1.len(), 1, "one graph after scan 1");

    // Second scan: change content (so hash differs → pre-pass tries to delete).
    // Use FailingDeleteStore so delete_graph always errors.
    fs::write(root.join("a.rs"), "fn beta() {}\n").expect("write a.rs v2");
    let failing_store = FailingDeleteStore { inner: real_store };
    let (summary2, manifest2) = scan_repository(
        &root,
        &scan_opts(source_name, manifest1.clone()),
        &failing_store,
        |_| {},
    )
    .expect("scan 2 must not panic");

    // Error must be surfaced (FIX 2 requirement i).
    assert!(
        summary2.errors > 0,
        "delete failure must increment summary.errors (got {})",
        summary2.errors
    );

    // No duplicate graph: prior graph still exists, no new one written (FIX 2 ii / AC-4).
    let graphs_after_scan2 = block_on(
        failing_store
            .inner
            .list_graphs_by_source(&scope(), source_key),
    )
    .expect("list after 2");
    assert_eq!(
        graphs_after_scan2.len(),
        1,
        "graph count must stay at 1 — no duplicate (prior graph preserved, new not written)"
    );

    // Path NOT dropped from manifest: old hash retained for retry (FIX 2 iii).
    assert!(
        manifest2.contains_key("a.rs"),
        "a.rs must remain in manifest for retry after delete failure"
    );
    assert_eq!(
        manifest2.get("a.rs"),
        manifest1.get("a.rs"),
        "manifest must carry OLD hash (unchanged — new graph was not written)"
    );

    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// FIX 3: Oversize file present on disk is observed, so its graph is NOT
//         deleted by the removed-path post-pass.
// ---------------------------------------------------------------------------

#[test]
fn oversize_file_present_on_disk_keeps_its_graph() {
    let root = std::env::temp_dir().join(format!(
        "engram-retract-fix3-{}-{}",
        std::process::id(),
        "oversize"
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create root");

    let source_name = "retract-fix3-source";
    let source_key = source_name;

    // First scan: a.rs is within the default 1 MiB limit — gets ingested.
    fs::write(root.join("a.rs"), "fn hello() {}\n").expect("write a.rs");
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let (_, manifest1) = scan_repository(
        &root,
        &scan_opts(source_name, Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    let graphs_after_scan1 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after 1");
    assert_eq!(graphs_after_scan1.len(), 1, "graph present after scan 1");
    assert!(
        manifest1.contains_key("a.rs"),
        "a.rs in manifest after scan 1"
    );

    // Second scan: max_bytes = 1 byte — a.rs is now "oversize" but STILL
    // present on disk.  With FIX 3 it must be in `observed_paths` so it is
    // NOT classified as a removal, and its graph must survive.
    let opts2 = ScanOptions {
        max_bytes: 1,
        manifest: manifest1,
        ..scan_opts(source_name, Default::default())
    };
    let (_, _manifest2) = scan_repository(&root, &opts2, &store, |_| {}).expect("scan 2");

    let graphs_after_scan2 =
        block_on(store.list_graphs_by_source(&scope(), source_key)).expect("list after 2");
    assert_eq!(
        graphs_after_scan2.len(),
        1,
        "graph must be preserved when the file is oversize-but-present (FIX 3)"
    );

    let _ = fs::remove_dir_all(&root);
}
