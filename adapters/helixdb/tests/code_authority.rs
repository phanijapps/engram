#![cfg(any(feature = "embedded", feature = "service"))]
#[path = "support/data_fixture.rs"]
mod fixture;
use engram_domain::*;
use engram_knowledge::*;
use fixture::*;
use futures::executor::block_on;
#[cfg(feature = "embedded")]
#[test]
fn embedded_complete_code_storage_ports() {
    exercise(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_complete_code_storage_ports() {
    exercise(Mode::Service)
}
fn exercise(mode: Mode) {
    block_on(async {
        let mut f = Fixture::new(mode);
        let s = &f.store;
        let scope = scope();
        let repo = RepoId("repo".into());
        let foreign = RepoId("foreign".into());
        let mut other = scope.clone();
        other.subject = Some(String::new());
        let registration = RepositoryRegistration {
            repo_id: repo.clone(),
            locator_id: "fixture-locator".into(),
            acquisition: AcquisitionMode::LocalOnly,
        };
        s.register(registration.clone()).await.unwrap();
        assert_eq!(s.get_registration(&repo).await.unwrap(), Some(registration));
        let binding = CodeBinding {
            scope: scope.clone(),
            repo_id: repo.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        };
        s.bind(binding.clone()).await.unwrap();
        assert_eq!(s.bindings(&scope).await.unwrap(), vec![binding.clone()]);
        assert!(s.get_binding(&other, &repo).await.unwrap().is_none());
        let generation = code_generation("g1");
        s.put_generation(generation.clone()).await.unwrap();
        assert!(
            s.get_generation(&scope, &foreign, "g1")
                .await
                .unwrap()
                .is_none()
        );
        let a = s
            .begin_attempt(
                &scope,
                &repo,
                generation.clone(),
                code_attempt("a1", 0, None),
            )
            .await
            .unwrap();
        assert_eq!(a.fencing_token, 1);
        assert_eq!(
            s.begin_attempt(
                &scope,
                &repo,
                generation.clone(),
                code_attempt("a1", 0, None)
            )
            .await
            .unwrap(),
            a
        );
        assert_eq!(
            s.get_attempt(&scope, &repo, "a1").await.unwrap(),
            Some((generation.clone(), a.clone()))
        );
        assert_eq!(
            s.get_generation(&scope, &repo, "g1").await.unwrap(),
            Some(generation.clone())
        );
        assert!(s.get_attempt(&other, &repo, "a1").await.unwrap().is_none());
        s.stage_lineage(&scope, "target-1", &code_lineage("repo", "g1"))
            .await
            .unwrap();
        let mut bad = code_lineage("repo", "g1");
        bad.source_path = "../escape".into();
        assert!(s.stage_lineage(&scope, "bad", &bad).await.is_err());
        let descriptor = IndexedSource {
            repo_id: repo.clone(),
            generation_id: "g1".into(),
            commit_oid: generation.commit_oid.clone(),
            path: "src/lib.rs".into(),
            content_digest: "3".repeat(64),
        };
        s.stage_source(&scope, &descriptor).await.unwrap();
        s.put_source_descriptor(&scope, descriptor.clone())
            .await
            .unwrap();
        assert_eq!(
            s.get_source_descriptor(&scope, &repo, "g1", "src/lib.rs")
                .await
                .unwrap(),
            Some(descriptor.clone())
        );
        assert!(
            s.get_source_descriptor(&scope, &foreign, "g1", "src/lib.rs")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            s.authorize_query(&scope, &["target-1".into()], 10)
                .await
                .is_err()
        );
        assert!(s.mark_ready(&scope, &repo, "a1", 1).await.is_err());
        for lane in ["source", "knowledge"] {
            let receipt = StageReceipt {
                attempt_id: "a1".into(),
                generation_id: "g1".into(),
                lane: lane.into(),
                digest: format!("digest-{lane}"),
            };
            s.record_receipt(receipt.clone()).await.unwrap();
            s.record_receipt(receipt).await.unwrap();
        }
        let ready = s.mark_ready(&scope, &repo, "a1", 1).await.unwrap();
        assert_eq!(ready.state, CodeAttemptState::Ready);
        let published = s.publish(&scope, &repo, "a1", 0).await.unwrap();
        assert_eq!(published.epoch, 1);
        assert_eq!(s.publish(&scope, &repo, "a1", 0).await.unwrap(), published);
        assert!(matches!(
            s.publish(&scope, &repo, "a1", 99).await,
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
        assert_eq!(s.bind(binding).await.unwrap(), published);
        let view = s.eligibility(&scope).await.unwrap();
        assert_eq!(view.epoch, 2);
        assert_eq!(view.generations, vec!["g1"]);
        assert_eq!(
            s.authorize_query(&scope, &[], 1).await.unwrap().target_ids,
            vec!["target-1"]
        );
        assert!(
            s.authorize_query(&scope, &["target-1".into(), "foreign".into()], 10)
                .await
                .is_err()
        );
        assert!(s.authorize_maintenance(&scope, &[]).await.is_err());
        assert_eq!(
            s.record_lineage(&scope, "target-1")
                .await
                .unwrap()
                .unwrap()
                .source_path,
            "src/lib.rs"
        );
        let now = chrono::Utc::now();
        let lease = CodeReadLease {
            id: "lease".into(),
            eligibility: view.clone(),
            fencing_token: 1,
            expires_at: now + chrono::Duration::minutes(5),
        };
        let lease = s.acquire_read_lease(lease, now).await.unwrap();
        assert!(
            s.validate_historical_read_lease(&lease, &scope, "repo", "g1", now)
                .await
                .unwrap()
        );
        assert!(
            !s.validate_historical_read_lease(&lease, &other, "repo", "g1", now)
                .await
                .unwrap()
        );
        assert_eq!(s.leased_generations(&scope, now).await.unwrap(), vec!["g1"]);
        assert!(s.retained_generations(&scope, now, u64::MAX).await.is_err());
        let a2 = ready_code(s, &repo, "g2", "a2", 1, Some("g1")).await;
        s.put_lineage(&scope, "target-2", code_lineage("repo", "g2"))
            .await
            .unwrap();
        assert_eq!(a2.fencing_token, 2);
        // A newer, unpublished attempt does not supersede the current publication.
        assert_eq!(s.publish(&scope, &repo, "a1", 0).await.unwrap(), published);
        s.publish(&scope, &repo, "a2", 1).await.unwrap();
        assert!(matches!(
            s.publish(&scope, &repo, "a1", 0).await,
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
        let renewed = s
            .renew_read_lease(&lease, now, lease.expires_at + chrono::Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(renewed.eligibility, view);
        assert!(
            s.validate_historical_read_lease(&renewed, &scope, "repo", "g1", now)
                .await
                .unwrap(),
            "valid immutable historical lease survives later publication"
        );
        assert!(
            !s.validate_historical_read_lease(&lease, &scope, "repo", "g1", now)
                .await
                .unwrap()
        );
        assert!(s.release_read_lease(&lease).await.is_err());
        s.release_read_lease(&renewed).await.unwrap();
        assert!(s.acquire_read_lease(renewed.clone(), now).await.is_err());
        assert!(s.leased_generations(&scope, now).await.unwrap().is_empty());
        assert_eq!(
            s.retained_generations(&scope, now, 0).await.unwrap(),
            vec!["g1", "g2"]
        );
        let observation = CodeRefObservation {
            branch_ref: "refs/heads/main".into(),
            observed_oid: "4".repeat(40),
            observed_at: now,
            policy: RefObservationPolicy::OnRequest,
        };
        s.put_ref_observation(&scope, &repo, observation.clone())
            .await
            .unwrap();
        assert_eq!(
            s.get_ref_observation(&scope, &repo).await.unwrap(),
            Some(observation)
        );
        assert!(
            s.get_ref_observation(&other, &repo)
                .await
                .unwrap()
                .is_none()
        );
        let ordinary = entity("ordinary", &["other-source"]);
        s.put_entity(ordinary.clone()).await.unwrap();
        let preview = s.preview_reset(&scope, &repo).await.unwrap();
        assert_eq!(preview.targets.len(), 2);
        let journal = CodeResetJournal {
            reset_id: "reset".into(),
            preview_hash: preview.preview_hash.clone(),
            target_fingerprints: preview
                .targets
                .iter()
                .map(|t| (t.record_id.clone(), t.fingerprint.clone()))
                .collect(),
            authorizing_decisions: vec!["fixture-storage-decision".into()],
            checkpoints: vec![],
            preservation_digests: preview.preservation_digests.clone(),
            preview,
        };
        let mut unknown = journal.clone();
        unknown.reset_id = "unregistered-reset".into();
        assert!(s.apply_reset(unknown).await.is_err());
        s.journal_reset(journal.clone()).await.unwrap();
        assert_eq!(s.load_reset_journal("reset").await.unwrap(), journal);
        let complete = s.apply_reset(journal.clone()).await.unwrap();
        assert!(complete.checkpoints.contains(&"completed:target-1".into()));
        let epochs = s.eligibility(&scope).await.unwrap().epoch;
        s.apply_reset(journal).await.unwrap();
        assert_eq!(s.eligibility(&scope).await.unwrap().epoch, epochs);
        assert!(
            s.record_lineage(&scope, "target-1")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        assert!(
            s.authorize_query(&scope, &["target-1".into()], 1)
                .await
                .is_err()
        );
        assert_eq!(
            s.get_entity(&ordinary.id, &scope).await.unwrap(),
            Some(ordinary)
        );
        f.reopen();
        assert!(
            f.store
                .record_lineage(&scope, "target-2")
                .await
                .unwrap()
                .unwrap()
                .retired
        );
        assert_eq!(
            f.store
                .get_source_descriptor(&scope, &repo, "g1", "src/lib.rs")
                .await
                .unwrap(),
            Some(descriptor)
        );
        println!(
            "TASK006 {mode:?}: complete code storage, lease, source-descriptor, journal/default-reset, exact scoped authority and reopen checks completed"
        );
    });
}

#[cfg(feature = "embedded")]
#[test]
#[ignore = "explicit bounded child lifecycle proof with identified failure-retaining fixture"]
fn embedded_identified_open_reopen_close() {
    let mut fixture = Fixture::new(Mode::Embedded);
    fixture.reopen();
    let closed = fixture.store.close();
    eprintln!("TASK006 identified lifecycle explicit final close: {closed:?}");
    assert!(
        closed.is_ok(),
        "explicit native close must succeed; fixture drop preserves failures"
    );
}
