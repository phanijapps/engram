#[test]
fn last_association_substitution_must_reject_before_any_effect() {
    let exposed = std::panic::catch_unwind(|| {
        let expected = [
            ("first", "repo", "generation"),
            ("last", "repo", "generation"),
        ];
        let durable = [
            ("first", "repo", "generation"),
            ("last", "repo", "substituted"),
        ];
        let unsafe_any_match = expected.iter().any(|target| durable.contains(target));
        let owned_effects = usize::from(unsafe_any_match);
        assert_eq!(
            owned_effects, 0,
            "partial membership accepted a substituted final target"
        );
    });
    assert!(exposed.is_err());
}
#[cfg(any(feature = "embedded", feature = "service"))]
#[path = "support/data_fixture.rs"]
mod fixture;
#[cfg(any(feature = "embedded", feature = "service"))]
use engram_domain::*;
#[cfg(any(feature = "embedded", feature = "service"))]
use engram_knowledge::*;
#[cfg(any(feature = "embedded", feature = "service"))]
use fixture::*;
#[cfg(any(feature = "embedded", feature = "service"))]
use futures::executor::block_on;
#[cfg(feature = "service")]
static SERVICE: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(feature = "embedded")]
#[test]
fn embedded_complete_set_epoch_lease_and_repeat_retirement() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_complete_set_epoch_lease_and_repeat_retirement() {
    let _serial = SERVICE.lock().unwrap();
    exercise(Mode::Service)
}
#[cfg(any(feature = "embedded", feature = "service"))]
async fn setup(store: &engram_store_helixdb::HelixStore) -> RepoId {
    let repo = RepoId("repo".into());
    store
        .register(RepositoryRegistration {
            repo_id: repo.clone(),
            locator_id: "fixture".into(),
            acquisition: AcquisitionMode::LocalOnly,
        })
        .await
        .unwrap();
    store
        .bind(CodeBinding {
            scope: scope(),
            repo_id: repo.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            epoch: 0,
            active_generation: None,
        })
        .await
        .unwrap();
    ready_code(store, &repo, "g1", "a1", 0, None).await;
    store.publish(&scope(), &repo, "a1", 0).await.unwrap();
    for id in ["first", "last"] {
        store
            .stage_lineage(&scope(), id, &code_lineage("repo", "g1"))
            .await
            .unwrap();
    }
    repo
}
#[cfg(any(feature = "embedded", feature = "service"))]
fn targets() -> Vec<RetirementTarget> {
    ["first", "last"]
        .iter()
        .map(|id| RetirementTarget {
            repo_id: "repo".into(),
            generation_id: "g1".into(),
            record_id: (*id).into(),
        })
        .collect()
}
#[cfg(any(feature = "embedded", feature = "service"))]
fn exercise(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let s = &f.store;
        let repo = setup(s).await;
        let ordinary = entity("ordinary", &["other-source"]);
        s.put_entity(ordinary.clone()).await.unwrap();
        let mut t = targets();
        t[1].generation_id = "substituted".into();
        assert!(
            s.retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &t)
                .await
                .unwrap_err()
                .to_string()
                .contains("GenerationChanged")
        );
        assert!(
            !s.record_lineage(&scope(), "first")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        assert_eq!(
            s.get_binding(&scope(), &repo).await.unwrap().unwrap().epoch,
            1
        );
        assert!(
            s.retire_lineage_targets(&scope(), &[(repo.clone(), 0)], &targets())
                .await
                .is_err()
        );
        let now = chrono::Utc::now();
        let lease = s
            .acquire_read_lease(
                CodeReadLease {
                    id: "live".into(),
                    eligibility: s.eligibility(&scope()).await.unwrap(),
                    fencing_token: 1,
                    expires_at: now + chrono::Duration::minutes(3),
                },
                now,
            )
            .await
            .unwrap();
        assert!(
            s.retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &targets())
                .await
                .unwrap_err()
                .to_string()
                .contains("ResetLeaseActive")
        );
        assert!(
            !s.record_lineage(&scope(), "first")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        s.release_read_lease(&lease).await.unwrap();
        let mut contradictory = targets();
        contradictory.push(RetirementTarget {
            record_id: "first".into(),
            repo_id: "repo".into(),
            generation_id: "other".into(),
        });
        assert!(
            s.retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &contradictory)
                .await
                .is_err()
        );
        assert!(
            s.retire_lineage_targets(
                &scope(),
                &[(repo.clone(), 1), (repo.clone(), 2)],
                &targets()
            )
            .await
            .is_err()
        );
        s.retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &targets())
            .await
            .unwrap();
        let binding = s.get_binding(&scope(), &repo).await.unwrap().unwrap();
        assert_eq!(binding.epoch, 2);
        let view = s.eligibility(&scope()).await.unwrap();
        s.retire_lineage_targets(&scope(), &[(repo.clone(), 2)], &targets())
            .await
            .unwrap();
        assert_eq!(s.eligibility(&scope()).await.unwrap(), view);
        assert_eq!(s.get_binding(&scope(), &repo).await.unwrap(), Some(binding));
        assert_eq!(
            s.get_entity(&ordinary.id, &scope()).await.unwrap(),
            Some(ordinary)
        );
        f.reopen();
        assert!(
            f.store
                .record_lineage(&scope(), "last")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        println!(
            "TASK006 {mode:?}: complete-set last-association/epoch rejection, lease protection, contradictory tuple rejection and exact no-op epochs verified"
        );
    });
}
#[cfg(feature = "service")]
#[test]
fn service_post_commit_retirement_ack_loss_reconciles_without_replay() {
    let _serial = SERVICE.lock().unwrap();
    block_on(async {
        let mut f = Fixture::new(Mode::Service);
        let repo = setup(&f.store).await;
        let control = std::path::PathBuf::from(
            std::env::var("HELIX_TEST_CONTROL_DIR")
                .expect("owned actual gateway controls required"),
        );
        let journal_rows = || {
            std::fs::read_to_string(control.join("requests.jsonl"))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .collect::<Vec<_>>()
        };
        let baseline = journal_rows().len();
        let partition = f.partition.discriminator().unwrap();
        // The unchanged real gateway drops only an actual write body containing these bytes.
        // Initialization is complete; this exact partition identifies the next retirement write.
        std::fs::write(control.join("drop-next-write-ack"), &partition).unwrap();
        let error = f
            .store
            .retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &targets())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Helix operation outcome unknown:"));
        let id = error.split_whitespace().last().unwrap();
        uuid::Uuid::parse_str(id).unwrap();
        let after_effect = journal_rows();
        let dropped = after_effect[baseline..]
            .iter()
            .filter(|row| row["ack_dropped_after_actual_response"] == true)
            .collect::<Vec<_>>();
        assert_eq!(
            dropped.len(),
            1,
            "exactly one actual committed response must be dropped"
        );
        assert_eq!(dropped[0]["write"], true);
        assert_eq!(dropped[0]["status"], 200);
        assert_eq!(dropped[0]["durable"], "true");
        assert!(
            !control.join("drop-next-write-ack").exists(),
            "marker present in exact actual body was consumed"
        );
        assert!(f.store.eligibility(&scope()).await.is_err());
        assert_eq!(
            journal_rows().len(),
            after_effect.len(),
            "cleanup-only admission sends no request"
        );
        let receipt = f.store.reconcile_operation(id).unwrap();
        assert_eq!(receipt["id"], id);
        assert_eq!(receipt["partition"], partition);
        assert_eq!(receipt["fingerprint"].as_str().unwrap().len(), 64);
        assert!(receipt["owner"].as_str().is_some());
        assert!(receipt["epoch"].as_u64().is_some());
        let after_receipt_read = journal_rows();
        assert_eq!(
            after_receipt_read.len(),
            after_effect.len() + 1,
            "one exact receipt lookup; request_type write preserves writer consistency but does not mutate"
        );
        println!(
            "TASK006 actual lost-ack logical-retirement operation={id} full-receipt={receipt}; baseline={baseline}; after-effect={}; after-readonly-reconciliation={}",
            after_effect.len(),
            after_receipt_read.len()
        );
        assert!(
            f.store
                .retire_lineage_targets(&scope(), &[(repo.clone(), 1)], &targets())
                .await
                .is_err()
        );
        assert_eq!(
            journal_rows().len(),
            after_receipt_read.len(),
            "denied second logical retirement must send no request"
        );
        // Reopen performs owned close and startup, which may make ownership writes.
        // They are outside the no-replay interval recorded above.
        f.reopen();
        assert_eq!(f.store.operation_receipt(id).unwrap(), Some(receipt));
        assert!(
            f.store
                .record_lineage(&scope(), "first")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        assert_eq!(
            f.store
                .get_binding(&scope(), &repo)
                .await
                .unwrap()
                .unwrap()
                .epoch,
            2
        );
    });
}
