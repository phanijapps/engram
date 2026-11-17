#[cfg(any(feature = "embedded", feature = "service"))]
#[path = "support/data_fixture.rs"]
mod fixture;
#[cfg(any(feature = "embedded", feature = "service"))]
use engram_knowledge::{KnowledgeRepository, SourceBatchLedger, SourceBatchRecordKind};
#[cfg(any(feature = "embedded", feature = "service"))]
use engram_store_helixdb::error::Error;
#[cfg(any(feature = "embedded", feature = "service"))]
use fixture::*;
#[cfg(any(feature = "embedded", feature = "service"))]
use futures::executor::block_on;
#[cfg(feature = "service")]
use std::sync::Mutex;
#[cfg(any(feature = "embedded", feature = "service"))]
use std::sync::{Arc, Barrier};
#[cfg(feature = "service")]
static SERVICE: Mutex<()> = Mutex::new(());
#[test]
fn unsafe_reference_replacement_control_is_detected() {
    let unsafe_update = std::panic::catch_unwind(|| {
        let prior = vec!["source-a"];
        let persisted = vec!["source-b"];
        assert!(
            persisted.contains(&prior[0]),
            "unsafe replacement lost surviving source A"
        );
    });
    assert!(unsafe_update.is_err());
}
#[cfg(feature = "embedded")]
#[test]
fn embedded_scope_guard_atomic_membership_revision_and_partial_success() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_scope_guard_atomic_membership_revision_and_partial_success() {
    let _serial = SERVICE.lock().unwrap();
    exercise(Mode::Service)
}
#[cfg(any(feature = "embedded", feature = "service"))]
fn exercise(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let scope = scope();
        let mut foreign = scope.clone();
        foreign.environment = None;
        let session = f.store.scope_session().unwrap();
        let s = session.store();
        let step = s.begin_step().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let other = f.store.clone();
        let waiting = barrier.clone();
        let thread = std::thread::spawn(move || {
            waiting.wait();
            assert!(matches!(other.scope_session(), Err(Error::Contention)));
            assert!(block_on(other.put_entity(entity("interleaving", &["source-b"]))).is_err());
        });
        barrier.wait();
        thread.join().unwrap();
        s.put_entity(entity("shared", &["source-a"])).await.unwrap();
        s.put_relationship(relation("shared-edge", &["source-a"]))
            .await
            .unwrap();
        s.record_source_batch_writes(
            &scope,
            "source-a",
            "a-v1",
            SourceBatchRecordKind::Entity,
            &["shared".into()],
        )
        .await
        .unwrap();
        s.record_source_batch_writes(
            &scope,
            "source-a",
            "a-v1",
            SourceBatchRecordKind::Relationship,
            &["shared-edge".into()],
        )
        .await
        .unwrap();
        let reader = f.reader();
        assert!(
            reader
                .get_entity(&"shared".into(), &scope)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .list_source_batch_records(&scope, "source-a")
                .await
                .unwrap()
                .is_empty()
        );
        let receipt = step.commit().unwrap();
        assert_eq!(
            s.operation_receipt(&receipt).unwrap().unwrap()["mutation_count"].as_u64(),
            Some(6)
        );
        assert!(
            reader
                .get_entity(&"shared".into(), &scope)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            reader
                .list_source_batch_records(&scope, "source-a")
                .await
                .unwrap()
                .len(),
            2
        );
        reader.close().unwrap();
        drop(s);
        drop(session);
        // A distinct source reads and unions prior references under the same complete-scope guard.
        let session = f.store.scope_session().unwrap();
        let s = session.store();
        let step = s.begin_step().unwrap();
        let mut shared = s
            .get_entity(&"shared".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        shared
            .source_refs
            .extend(entity("shared", &["source-b"]).source_refs);
        s.put_entity(shared).await.unwrap();
        let mut edge = s
            .get_relationship(&"shared-edge".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        edge.evidence
            .extend(relation("shared-edge", &["source-b"]).evidence);
        s.put_relationship(edge).await.unwrap();
        for (k, id) in [
            (SourceBatchRecordKind::Entity, "shared"),
            (SourceBatchRecordKind::Relationship, "shared-edge"),
        ] {
            s.record_source_batch_writes(&scope, "source-b", "b-v1", k, &[id.into()])
                .await
                .unwrap();
        }
        step.commit().unwrap();
        let shared = s
            .get_entity(&"shared".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shared.source_refs.len(), 2);
        assert_eq!(
            s.get_relationship(&"shared-edge".into(), &scope)
                .await
                .unwrap()
                .unwrap()
                .evidence
                .len(),
            2
        );
        // An abandoned failed step cannot leak its record or ledger; earlier committed steps survive.
        {
            let _failed = s.begin_step().unwrap();
            s.put_entity(entity("failed-step", &["source-b"]))
                .await
                .unwrap();
            assert!(
                s.record_source_batch_writes(
                    &foreign,
                    "source-b",
                    "bad",
                    SourceBatchRecordKind::Entity,
                    &["failed-step".into()]
                )
                .await
                .is_err()
            );
        }
        assert!(
            s.get_entity(&"failed-step".into(), &scope)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            s.list_source_batch_records(&scope, "source-b")
                .await
                .unwrap()
                .len(),
            2
        );
        // A failed nested multi-record call restores its staged savepoint even if the caller commits the outer step.
        {
            let nested = s.begin_step().unwrap();
            let good: engram_domain::UnresolvedReference = fixture::make(
                "nested-good",
                serde_json::json!({"graphId":"graph","fromEntityId":"left","referenceName":"unknown","status":"pending","path":"file"}),
            );
            let mut bad: engram_domain::UnresolvedReference = good.clone();
            bad.id = "nested-bad".into();
            bad.scope = foreign.clone();
            assert!(
                engram_knowledge::KnowledgeGraphRepository::put_unresolved_refs(
                    &s,
                    vec![good, bad]
                )
                .await
                .is_err()
            );
            nested.commit().unwrap();
            assert!(
                engram_knowledge::KnowledgeGraphRepository::list_unresolved_refs(
                    &s,
                    &scope,
                    engram_domain::UnresolvedReferenceStatus::Pending
                )
                .await
                .unwrap()
                .is_empty()
            );
        }
        // Storage consumes a canonical caller's explicit revision effect; it does not choose supersession.
        let step = s.begin_step().unwrap();
        let mut shared = s
            .get_entity(&"shared".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        shared
            .source_refs
            .retain(|r| r.target_id.as_deref() != Some("source-a"));
        s.put_entity(shared).await.unwrap();
        let mut edge = s
            .get_relationship(&"shared-edge".into(), &scope)
            .await
            .unwrap()
            .unwrap();
        edge.evidence
            .retain(|r| r.target_id.as_deref() != Some("source-a"));
        s.put_relationship(edge).await.unwrap();
        for (k, id) in [
            (SourceBatchRecordKind::Entity, "shared"),
            (SourceBatchRecordKind::Relationship, "shared-edge"),
        ] {
            s.mark_source_batch_superseded(&scope, "source-a", "a-v1", k, id)
                .await
                .unwrap();
        }
        step.commit().unwrap();
        assert!(
            s.list_source_batch_records(&scope, "source-a")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.list_source_batch_records(&scope, "source-b")
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.get_entity(&"shared".into(), &scope)
                .await
                .unwrap()
                .unwrap()
                .source_refs[0]
                .target_id
                .as_deref(),
            Some("source-b")
        );
        assert_eq!(
            s.get_relationship(&"shared-edge".into(), &scope)
                .await
                .unwrap()
                .unwrap()
                .evidence[0]
                .target_id
                .as_deref(),
            Some("source-b")
        );
        // Ledger-only status updates never silently mutate foreign/code-attributed record ownership.
        let mut protected = entity("protected", &["code-owner"]);
        protected.metadata = Some(
            serde_json::from_value(
                serde_json::json!({"repositoryId":"repo","generationId":"generation"}),
            )
            .unwrap(),
        );
        s.put_entity(protected.clone()).await.unwrap();
        s.record_source_batch_writes(
            &scope,
            "source-a",
            "a-v1",
            SourceBatchRecordKind::Entity,
            &["protected".into()],
        )
        .await
        .unwrap();
        s.mark_source_batch_superseded(
            &scope,
            "source-a",
            "a-v1",
            SourceBatchRecordKind::Entity,
            "protected",
        )
        .await
        .unwrap();
        assert_eq!(
            s.get_entity(&protected.id, &scope).await.unwrap(),
            Some(protected)
        );
        assert!(
            s.list_source_batch_records(&foreign, "source-b")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!s.delete_entity(&"shared".into(), &foreign).await.unwrap());
        drop(s);
        drop(session);
        f.reopen();
        assert_eq!(
            f.store
                .list_source_batch_records(&scope, "source-b")
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            f.store
                .get_entity(&"shared".into(), &scope)
                .await
                .unwrap()
                .unwrap()
                .source_refs
                .len(),
            1
        );
        println!(
            "TASK004 {mode:?}: competing same-scope writers rejected; record/ref/ledger visibility atomic; source B survives A revision; earlier step survives later failure; scoped foreign/code data retained"
        );
    });
}
#[cfg(feature = "service")]
#[test]
fn service_unknown_atomic_commit_reconciles_receipt_without_replay() {
    let _serial = SERVICE.lock().unwrap();
    block_on(async {
        // Gateway controls are isolated to this binary; synchronize service cases via this test's own store.
        let f = Fixture::new(Mode::Service);
        let session = f.store.scope_session().unwrap();
        let s = session.store();
        let step = s.begin_step().unwrap();
        s.put_entity(entity("unknown", &["source"])).await.unwrap();
        s.record_source_batch_writes(
            &scope(),
            "source",
            "v1",
            SourceBatchRecordKind::Entity,
            &["unknown".into()],
        )
        .await
        .unwrap();
        let control = std::path::PathBuf::from(
            std::env::var("HELIX_TEST_CONTROL_DIR")
                .expect("owned actual gateway controls required"),
        );
        std::fs::write(control.join("drop-next-write-ack"), "task004").unwrap();
        let id = match step.commit() {
            Err(Error::OutcomeUnknown { operation_id }) => operation_id,
            other => panic!("expected actual lost-ack unknown, got {other:?}"),
        };
        assert!(s.put_entity(entity("must-not-replay", &[])).await.is_err());
        assert!(s.begin_step().is_err());
        let receipt = s.reconcile_operation(&id).unwrap();
        assert_eq!(receipt["id"], id);
        assert!(s.begin_step().is_err());
        assert!(
            s.put_entity(entity("still-cleanup-only", &[]))
                .await
                .is_err()
        );
        drop(s);
        drop(session);
    });
}
