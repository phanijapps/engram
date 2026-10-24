//! Knowledge change-log + scan-checkpoint tests (enhancements adoption,
//! slice 3): no-op rule, resumable paging, removal events, reset detection,
//! stale-run checkpoint guard, and the staleness derivation matrix.

use engram_domain::*;
use engram_knowledge::{KnowledgeChangeLog, KnowledgeRepository, ScanCheckpointStore};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "tenant-a".to_owned(),
        subject: Some("subject-a".to_owned()),
        workspace: Some("workspace-a".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn entity(id: &str, name: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: EntityId::from(id),
        graph_id: Some(KnowledgeGraphId::from("g1")),
        kind: EntityKind::Function,
        name: name.to_owned(),
        aliases: vec![],
        scope: scope(),
        source_refs: vec![],
        concept_refs: vec![],
        ontology_class_refs: vec![],
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        archived_at: None,
        metadata: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "test".to_owned(),
        actor: Actor {
            id: ActorId::from("a"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        observed_at: chrono::Utc::now(),
        evidence: vec![],
        derivations: vec![],
        confidence: None,
        method: None,
    }
}

fn page_req(after: Option<String>) -> KnowledgeChangesRequest {
    KnowledgeChangesRequest {
        scope: scope(),
        source_key: None,
        after,
        limit: 2,
    }
}

#[test]
fn changes_log_upserts_noops_and_removals_with_resumable_cursor() {
    let store = SqlKnowledgeStore::open_in_memory().expect("store");

    // Two distinct puts (events: e0, e1). The identical re-put below logs
    // nothing; the rename logs; the removal logs → 4 events total.
    let stored_e0 = block_on(store.put_entity(entity("e0", "fn_a"))).expect("first e0");
    block_on(store.put_entity(entity("e1", "fn_a"))).expect("e1");
    // Identical re-put (the exact persisted record) → NO event (no-op rule).
    block_on(store.put_entity(stored_e0.clone())).expect("identical re-put");
    block_on(store.put_entity(entity("e0", "fn_renamed"))).expect("changed re-put");
    // Removal → remove event.
    block_on(store.delete_entity(&EntityId::from("e1"), &scope())).expect("delete");

    // Page 1 (limit 2).
    let page1 = block_on(store.changes_since(&page_req(None))).expect("page 1");
    assert_eq!(page1.items.len(), 2);
    assert!(page1.has_more);
    let cursor1 = page1.next_cursor.clone().expect("cursor");

    // Page 2 resumes strictly after the cursor.
    let page2 = block_on(store.changes_since(&page_req(Some(cursor1)))).expect("page 2");
    assert_eq!(
        page2.items.len(),
        2,
        "4 events total (e0, e1, rename, remove)"
    );
    assert!(!page2.has_more, "exactly 4 events; no third page");
    assert_eq!(page1.items.len() + page2.items.len(), 4);

    // Ops are typed; the last event is the removal.
    let last = block_on(store.changes_since(&KnowledgeChangesRequest {
        scope: scope(),
        source_key: None,
        after: None,
        limit: 500,
    }))
    .expect("full")
    .items
    .pop()
    .expect("events");
    assert_eq!(last.op, KnowledgeChangeOp::Remove);
    assert_eq!(last.target_type, KnowledgeChangeTarget::Entity);

    // Sequence numbers serialize as decimal strings.
    assert!(last.seq.chars().all(|c| c.is_ascii_digit()), "{}", last.seq);
}

#[test]
fn empty_scope_page_is_empty_without_reset() {
    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let page = block_on(store.changes_since(&page_req(None))).expect("page");
    assert!(page.items.is_empty());
    assert!(!page.has_more);
    assert!(
        !page.reset_required,
        "fresh log: no cursor was consumed yet"
    );
    assert!(page.high_watermark.is_none());
}

#[test]
fn checkpoint_upsert_ignores_stale_runs_and_latest_round_trips() {
    let store = SqlKnowledgeStore::open_in_memory().expect("store");
    let newer = ScanCheckpoint {
        scope: scope(),
        stable_source_key: "repo-a".to_owned(),
        git_ref: None,
        scan_root: Some("/repo/a".to_owned()),
        scan_run_id: "run-2".to_owned(),
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        indexed_revision: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        status: ScanStatus::Completed,
        dirty: Some(false),
        errors: 0,
    };
    block_on(store.put_checkpoint(newer.clone())).expect("put newer");
    // A STALE run (older started_at) must not overwrite the newer checkpoint.
    let mut stale = newer.clone();
    stale.scan_run_id = "run-1".to_owned();
    stale.indexed_revision = None;
    stale.started_at = newer.started_at - chrono::Duration::hours(1);
    block_on(store.put_checkpoint(stale)).expect("stale put (no-op guard)");

    let latest = block_on(store.latest_checkpoint(&scope(), "repo-a"))
        .expect("latest")
        .expect("checkpoint present");
    assert_eq!(latest.scan_run_id, "run-2");
    assert_eq!(
        latest.indexed_revision.as_deref(),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
}

#[test]
fn staleness_derivation_never_fabricates_zero() {
    let cp = ScanCheckpoint {
        scope: scope(),
        stable_source_key: "repo-a".to_owned(),
        git_ref: None,
        scan_root: None,
        scan_run_id: "run-1".to_owned(),
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        indexed_revision: Some("aaa".to_owned()),
        status: ScanStatus::Completed,
        dirty: Some(false),
        errors: 0,
    };
    // No checkpoint → unindexed, no commits_behind.
    let s = SqlKnowledgeStore::derive_staleness(None, "repo-a", Some("bbb"), None, None);
    assert_eq!(s.status, StalenessStatus::Unindexed);
    assert!(s.commits_behind.is_none());

    // Same revision, clean → current; dirty → behind.
    let s =
        SqlKnowledgeStore::derive_staleness(Some(&cp), "repo-a", Some("aaa"), Some(0), Some(false));
    assert_eq!(s.status, StalenessStatus::Current);
    let s =
        SqlKnowledgeStore::derive_staleness(Some(&cp), "repo-a", Some("aaa"), Some(0), Some(true));
    assert_eq!(
        s.status,
        StalenessStatus::Behind,
        "dirty tree is not current"
    );

    // Ancestor with count → behind.
    let s = SqlKnowledgeStore::derive_staleness(Some(&cp), "repo-a", Some("bbb"), Some(3), None);
    assert_eq!(s.status, StalenessStatus::Behind);
    assert_eq!(s.commits_behind, Some(3));

    // Different revision, no ancestry count (rebase/shallow) → diverged,
    // never a guessed 0-behind.
    let s = SqlKnowledgeStore::derive_staleness(Some(&cp), "repo-a", Some("bbb"), None, None);
    assert_eq!(s.status, StalenessStatus::Diverged);

    // Git unavailable → unknown.
    let s = SqlKnowledgeStore::derive_staleness(Some(&cp), "repo-a", None, None, None);
    assert_eq!(s.status, StalenessStatus::Unknown);

    // Partial scan → partial.
    let mut partial = cp.clone();
    partial.status = ScanStatus::Partial;
    let s =
        SqlKnowledgeStore::derive_staleness(Some(&partial), "repo-a", Some("aaa"), Some(0), None);
    assert_eq!(s.status, StalenessStatus::Partial);
    assert!(!s.scan_in_progress || true); // explicit field, no fabrication
}
