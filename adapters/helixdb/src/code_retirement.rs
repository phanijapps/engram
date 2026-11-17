//! Complete-set engine-level code retirement fencing, independent of host authority.
use crate::{
    HelixStore,
    code_binding::{changed, increment, tuple},
    records::*,
    transaction::Fence,
};
use engram_domain::*;
use engram_knowledge::*;
use helix_db::dsl::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
impl HelixStore {
    pub(crate) fn retire_code(
        &self,
        scope: &Scope,
        epochs: &[(RepoId, u64)],
        targets: &[RetirementTarget],
        progress: Option<(&str, &[String])>,
    ) -> CoreResult<()> {
        self.check_scope(scope).map_err(core)?;
        if epochs.is_empty() || targets.is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "fenced maintenance requires explicit repos and targets".into(),
            });
        }
        let mut expected = BTreeMap::new();
        for (repo, epoch) in epochs {
            if expected
                .insert(repo.0.clone(), *epoch)
                .is_some_and(|old| old != *epoch)
            {
                return Err(changed());
            }
        }
        let mut exact = BTreeMap::new();
        for target in targets {
            if !expected.contains_key(&target.repo_id) {
                return Err(changed());
            }
            if exact
                .insert(target.record_id.clone(), target.clone())
                .is_some_and(|old| old != *target)
            {
                return Err(changed());
            }
        }
        if let Some((_, ids)) = progress {
            if ids.iter().any(|id| !exact.contains_key(id)) {
                return Err(CoreError::InvalidRequest {
                    reason: "reset progress must name retired targets".into(),
                });
            }
        }
        self.code_write(|s|{
   let mut bindings=vec![];let mut predicates=vec![];
   for(repo,epoch)in &expected{let binding=s.code_bound(&RepoId(repo.clone()))?;if binding.epoch!=*epoch{return Err(changed());}predicates.push(SourcePredicate::and(vec![SourcePredicate::eq("key",s.inner.partition.record_key(CODE_BINDING,repo).map_err(core)?),SourcePredicate::eq("repoId",repo.clone()),SourcePredicate::eq("epoch_key",epoch.to_string())]));bindings.push(binding);}
   s.code_fence(Fence{family:CODE_BINDING.into(),predicate:SourcePredicate::or(predicates),minimum:expected.len(),empty:false,reason:"GenerationChanged"})?;
   let mut rows=vec![];let mut predicates=vec![];
   for target in exact.values(){let lineage=s.code_get::<TargetLineage>(CODE_LINEAGE,&target.record_id)?.ok_or_else(changed)?;if lineage.repo_id!=target.repo_id||lineage.generation_id!=target.generation_id{return Err(changed());}predicates.push(SourcePredicate::and(vec![SourcePredicate::eq("key",s.inner.partition.record_key(CODE_LINEAGE,&target.record_id).map_err(core)?),SourcePredicate::eq("repoId",target.repo_id.clone()),SourcePredicate::eq("generationId",target.generation_id.clone())]));rows.push(lineage);}
   s.code_fence(Fence{family:CODE_LINEAGE.into(),predicate:SourcePredicate::or(predicates),minimum:exact.len(),empty:false,reason:"GenerationChanged"})?;
   if rows.iter().all(|l|l.retired){return Ok(());}
   s.code_all::<String>(CODE_LEASE_COVERAGE)?;
   let now=chrono::Utc::now();let generations=exact.values().map(|t|SourcePredicate::eq("generationId",t.generation_id.clone())).collect();
   s.code_fence(Fence{family:CODE_LEASE_COVERAGE.into(),predicate:SourcePredicate::and(vec![crate::code_leases::live_lease_at(now),SourcePredicate::or(generations)]),minimum:0,empty:true,reason:"ResetLeaseActive"})?;
   for mut binding in bindings{binding.epoch=increment(binding.epoch)?;s.code_put(CODE_BINDING,&binding.repo_id.0,&binding,json!({"repoId":binding.repo_id,"epoch_key":binding.epoch.to_string()}))?;}
   for mut lineage in rows{if !lineage.retired{lineage.retired=true;s.code_put(CODE_LINEAGE,&lineage.record_id,&lineage,json!({"repoId":lineage.repo_id,"generationId":lineage.generation_id,"retired":true}))?;}}
   if let Some((reset,ids))=progress{let journal:CodeResetJournal=s.code_get(CODE_RESET,reset)?.ok_or_else(changed)?;if journal.preview.binding.scope!=*scope||ids.iter().any(|id|!journal.preview.target_ids.contains(id)){return Err(changed());}for id in ids{s.code_put(CODE_RESET_PROGRESS,&tuple(&[reset,id]),&format!("completed:{id}"),json!({"resetId":reset,"recordId":id}))?;}}
   s.advance_code_epoch()?;Ok(())
 })
    }
}

#[cfg(all(test, feature = "embedded", feature = "service"))]
mod engine_fence_controls {
    use super::*;
    use crate::{
        StoreInner,
        codec::Partition,
        error::{Error, Result},
        transport::{Operation, Transport},
    };
    use futures::executor::block_on;
    use serde_json::Value;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    struct Boundary {
        actual: Arc<dyn Transport>,
        capture: bool,
        pending: Mutex<Option<Operation>>,
        injection: Mutex<Option<Operation>>,
        fired: AtomicBool,
    }
    impl Transport for Boundary {
        fn execute(&self, operation: &Operation) -> Result<Value> {
            if operation.mutates {
                if self.capture {
                    *self.pending.lock().unwrap() = Some(operation.clone());
                    return Err(Error::Backpressure);
                }
                if let Some(injection) = self.injection.lock().unwrap().take() {
                    let result = self.actual.execute(&injection)?;
                    assert!(
                        result.as_object().is_some(),
                        "actual boundary operation acknowledged"
                    );
                    self.fired.store(true, Ordering::SeqCst);
                }
            }
            self.actual.execute(operation)
        }
        fn close(&self) -> Result<()> {
            self.actual.close()
        }
        fn reconcile_read(&self, o: &Operation) -> Result<Value> {
            self.actual.reconcile_read(o)
        }
        fn confirm_operation(&self, id: &str) -> Result<()> {
            self.actual.confirm_operation(id)
        }
        fn register_cleanup(&self, l: crate::ownership::ScopeLease) -> Result<()> {
            self.actual.register_cleanup(l)
        }
    }
    fn wrap(actual: &HelixStore, transport: Arc<dyn Transport>) -> HelixStore {
        HelixStore {
            inner: Arc::new(StoreInner {
                transport,
                partition: actual.inner.partition.clone(),
                lease: actual.inner.lease.clone(),
                reservation: AtomicBool::new(false),
                cleanup_only: AtomicBool::new(false),
                uncertain_receipts: Mutex::new(BTreeMap::new()),
            }),
            session: None,
            reentrant: false,
            step_id: None,
        }
    }
    fn open(service: bool, root: &std::path::Path, partition: &Partition) -> HelixStore {
        if service {
            HelixStore::open_service(
                crate::config::ServiceConfig {
                    endpoint: std::env::var("HELIX_TEST_ENDPOINT")
                        .expect("actual pinned fixture required"),
                    credential_env: std::env::var("HELIX_TEST_CREDENTIAL_ENV")
                        .expect("explicit credential reference required"),
                    vectors_enabled: true,
                    read_only: false,
                    timeout: std::time::Duration::from_secs(120),
                },
                partition.clone(),
            )
            .unwrap()
        } else {
            HelixStore::open_embedded(
                crate::config::EmbeddedConfig {
                    trusted_root: root.into(),
                    path: root.join("store"),
                    queue_capacity: 8,
                    read_only: false,
                },
                partition.clone(),
            )
            .unwrap()
        }
    }
    fn generation(id: &str) -> CodeGeneration {
        CodeGeneration {
            id: id.into(),
            binding_version: 1,
            commit_oid: "1".repeat(40),
            snapshot_digest: "2".repeat(64),
            parser_fingerprint: "parser".into(),
            filter_fingerprint: "filter".into(),
        }
    }
    async fn ready(s: &HelixStore, id: &str, epoch: u64, active: Option<&str>) {
        let a = s
            .begin_attempt(
                &s.inner.partition.scope,
                &RepoId("repo".into()),
                generation(id),
                CodeAttempt {
                    id: format!("a-{id}"),
                    idempotency_key: format!("a-{id}"),
                    binding_version: 1,
                    expected_epoch: epoch,
                    expected_active_generation: active.map(str::to_owned),
                    fencing_token: 0,
                    state: CodeAttemptState::Created,
                    receipt_kinds: vec![CodeReceiptKind::Knowledge],
                },
            )
            .await
            .unwrap();
        s.record_receipt(StageReceipt {
            attempt_id: a.id.clone(),
            generation_id: id.into(),
            lane: "knowledge".into(),
            digest: "digest".into(),
        })
        .await
        .unwrap();
        s.mark_ready(
            &s.inner.partition.scope,
            &RepoId("repo".into()),
            &a.id,
            a.fencing_token,
        )
        .await
        .unwrap();
    }
    fn exercise(service: bool) {
        block_on(async {
            for scenario in ["publication", "last-association", "lease"] {
                let temporary = tempfile::tempdir().unwrap();
                let scope = Scope {
                    tenant: "tenant".into(),
                    subject: None,
                    workspace: Some("workspace".into()),
                    session: None,
                    environment: None,
                };
                let partition = Partition {
                    logical_store: format!("code-fence-{}", uuid::Uuid::new_v4()),
                    scope: scope.clone(),
                    embedding_space: None,
                };
                let actual = open(service, temporary.path(), &partition);
                let repo = RepoId("repo".into());
                actual
                    .register(RepositoryRegistration {
                        repo_id: repo.clone(),
                        locator_id: "fixture".into(),
                        acquisition: AcquisitionMode::LocalOnly,
                    })
                    .await
                    .unwrap();
                actual
                    .bind(CodeBinding {
                        scope: scope.clone(),
                        repo_id: repo.clone(),
                        branch_ref: "refs/heads/main".into(),
                        version: 1,
                        active_generation: None,
                        epoch: 0,
                    })
                    .await
                    .unwrap();
                ready(&actual, "g1", 0, None).await;
                actual.publish(&scope, &repo, "a-g1", 0).await.unwrap();
                let targets = ["first", "last"]
                    .iter()
                    .map(|id| RetirementTarget {
                        repo_id: "repo".into(),
                        generation_id: "g1".into(),
                        record_id: (*id).into(),
                    })
                    .collect::<Vec<_>>();
                for t in &targets {
                    actual
                        .put_lineage(
                            &scope,
                            &t.record_id,
                            CodeLineage {
                                repo_id: repo.clone(),
                                generation_id: "g1".into(),
                                source_path: "file.rs".into(),
                                contributors: vec!["source".into()],
                                span: None,
                            },
                        )
                        .await
                        .unwrap();
                }
                ready(&actual, "g2", 1, Some("g1")).await;
                let preview = actual.preview_reset(&scope, &repo).await.unwrap();
                actual
                    .journal_reset(CodeResetJournal {
                        reset_id: "race-reset".into(),
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
                    })
                    .await
                    .unwrap();
                let injection = if scenario == "last-association" {
                    let mut row = actual.get::<Value>(CODE_LINEAGE, "last").unwrap().unwrap();
                    row["record"]["generationId"] = "substituted".into();
                    row["generationId"] = "substituted".into();
                    Operation::new(QueryRequest::write(
                        write_batch()
                            .var_as(
                                "changed",
                                g().n_with_label_where(
                                    CODE_LINEAGE,
                                    SourcePredicate::and(vec![
                                        SourcePredicate::eq(
                                            "partition",
                                            partition.discriminator().unwrap(),
                                        ),
                                        SourcePredicate::eq(
                                            "key",
                                            partition.record_key(CODE_LINEAGE, "last").unwrap(),
                                        ),
                                    ]),
                                )
                                .set_property("generationId", "substituted")
                                .set_property("payload", serde_json::to_string(&row).unwrap()),
                            )
                            .returning(["changed"]),
                    ))
                    .unwrap()
                } else {
                    // Capture the public port's exact operation before effects; execute those bytes on the actual engine at the retirement boundary.
                    let capture = Arc::new(Boundary {
                        actual: actual.inner.transport.clone(),
                        capture: true,
                        pending: Mutex::new(None),
                        injection: Mutex::new(None),
                        fired: AtomicBool::new(false),
                    });
                    let builder = wrap(&actual, capture.clone());
                    if scenario == "publication" {
                        assert!(builder.publish(&scope, &repo, "a-g2", 1).await.is_err());
                    } else {
                        let now = chrono::Utc::now();
                        assert!(
                            builder
                                .acquire_read_lease(
                                    CodeReadLease {
                                        id: "racing-lease".into(),
                                        eligibility: actual.eligibility(&scope).await.unwrap(),
                                        fencing_token: 1,
                                        expires_at: now + chrono::Duration::minutes(5)
                                    },
                                    now
                                )
                                .await
                                .is_err()
                        );
                    }
                    let captured = capture
                        .pending
                        .lock()
                        .unwrap()
                        .take()
                        .expect("public port produced one bounded conditional operation");
                    drop(builder);
                    captured
                };
                let before = actual.eligibility(&scope).await.unwrap().epoch;
                let boundary = Arc::new(Boundary {
                    actual: actual.inner.transport.clone(),
                    capture: false,
                    pending: Mutex::new(None),
                    injection: Mutex::new(Some(injection)),
                    fired: AtomicBool::new(false),
                });
                let retiring = wrap(&actual, boundary.clone());
                let ids = vec!["first".into(), "last".into()];
                let error = retiring
                    .retire_lineage_targets_with_reset_progress(
                        &scope,
                        &[(repo.clone(), 1)],
                        &targets,
                        Some(("race-reset", &ids)),
                    )
                    .await
                    .unwrap_err();
                assert!(boundary.fired.load(Ordering::SeqCst));
                assert!(
                    matches!(error, CoreError::Conflict { .. }),
                    "fence rejection stays typed: {error}"
                );
                assert!(
                    !actual
                        .record_lineage(&scope, "first")
                        .await
                        .unwrap()
                        .unwrap()
                        .retired
                );
                assert!(
                    !actual
                        .record_lineage(&scope, "last")
                        .await
                        .unwrap()
                        .unwrap()
                        .retired
                );
                assert!(
                    actual
                        .load_reset_journal("race-reset")
                        .await
                        .unwrap()
                        .checkpoints
                        .is_empty()
                );
                let binding = actual.get_binding(&scope, &repo).await.unwrap().unwrap();
                assert_eq!(binding.epoch, if scenario == "publication" { 2 } else { 1 });
                assert_eq!(
                    actual.eligibility(&scope).await.unwrap().epoch,
                    before + u64::from(scenario == "publication")
                );
                if scenario == "publication" {
                    assert_eq!(binding.active_generation.as_deref(), Some("g2"));
                }
                if scenario == "last-association" {
                    assert_eq!(
                        actual
                            .record_lineage(&scope, "last")
                            .await
                            .unwrap()
                            .unwrap()
                            .generation_id,
                        "substituted"
                    );
                }
                println!(
                    "TASK006 actual engine fence service={service} scenario={scenario}: injected committed state change after validation; zero owned lineage/reset-progress/epoch effects; {error}"
                );
                retiring.close().unwrap();
                drop(retiring);
                drop(boundary);
                drop(actual);
            }
        });
    }
    #[test]
    fn embedded_engine_fence_rejects_publication_association_and_lease_interleavings() {
        exercise(false)
    }
    #[test]
    fn service_engine_fence_rejects_publication_association_and_lease_interleavings() {
        exercise(true)
    }
}
