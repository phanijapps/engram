use async_trait::async_trait;
use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeLineage, CodeReceiptKind,
    CodeResetJournal, RepoId, Scope,
};
use engram_integration::LexicalFeed;
use engram_integration::code_binding::{
    CodeBindingCoordinator, PreservationRecordSource, RecipeResetLaneLifecycle, ResetLane,
    ResetLaneLifecycle, ResetLaneOutcome, ScopeSetAuthorizer,
};
use engram_knowledge::{
    CodeBindingRepository, CodeReadLease, CodeReadLeases, RepositoryRegistration, RetirementTarget,
};
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;
use std::sync::{Arc, Mutex};

fn scope() -> Scope {
    Scope {
        tenant: "reset-tenant".into(),
        subject: None,
        workspace: Some("reset-workspace".into()),
        session: None,
        environment: None,
    }
}
fn repo() -> RepoId {
    RepoId("reset-repo".into())
}
fn generation() -> CodeGeneration {
    CodeGeneration {
        id: "reset-generation".into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser-v1".into(),
        filter_fingerprint: "filter-v1".into(),
    }
}
fn setup() -> SqlCodeBindingRepository {
    let store = SqlCodeBindingRepository::open(":memory:").expect("store");
    let binding = CodeBinding {
        scope: scope(),
        repo_id: repo(),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    };
    block_on(store.register(RepositoryRegistration {
        repo_id: repo(),
        locator_id: "fixture".into(),
        acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
    }))
    .expect("register");
    block_on(store.bind(binding)).expect("bind");
    let generation = generation();
    block_on(store.begin_attempt(
        &scope(),
        &repo(),
        generation.clone(),
        CodeAttempt {
            id: "reset-attempt".into(),
            idempotency_key: "reset-key".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Created,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        },
    ))
    .expect("attempt");
    block_on(store.put_lineage(
        &scope(),
        "target-a",
        CodeLineage {
            repo_id: repo(),
            generation_id: generation.id,
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into(), "src/mod.rs".into()],
            span: None,
        },
    ))
    .expect("lineage");
    store
}
fn setup_active() -> SqlCodeBindingRepository {
    let store = setup();
    block_on(store.record_receipt(engram_knowledge::StageReceipt {
        attempt_id: "reset-attempt".into(),
        generation_id: generation().id,
        lane: "knowledge".into(),
        digest: "receipt".into(),
    }))
    .expect("receipt");
    block_on(store.mark_ready(&scope(), &repo(), "reset-attempt", 1)).expect("ready");
    block_on(store.publish(&scope(), &repo(), "reset-attempt", 0)).expect("published");
    store
}
fn journal(preview: engram_domain::CodeResetPreview) -> CodeResetJournal {
    CodeResetJournal {
        reset_id: "reset-1".into(),
        preview_hash: preview.preview_hash.clone(),
        target_fingerprints: preview
            .targets
            .iter()
            .map(|target| (target.record_id.clone(), target.fingerprint.clone()))
            .collect(),
        authorizing_decisions: vec!["approved-by-fixture".into()],
        checkpoints: vec!["authorized".into()],
        preservation_digests: preview.preservation_digests.clone(),
        preview,
    }
}
fn approved(store: &SqlCodeBindingRepository, value: CodeResetJournal) -> CodeResetJournal {
    block_on(store.journal_reset(value.clone())).expect("owner-approved journal");
    value
}
fn rehash(value: &mut CodeResetJournal) {
    value.preview.preview_hash = value.preview.content_hash().expect("preview hash");
    value.preview_hash = value.preview.preview_hash.clone();
}
fn add_target(store: &SqlCodeBindingRepository, id: &str) {
    block_on(store.put_lineage(
        &scope(),
        id,
        CodeLineage {
            repo_id: repo(),
            generation_id: generation().id,
            source_path: format!("src/{id}.rs"),
            contributors: vec!["durable-lineage".into()],
            span: None,
        },
    ))
    .expect("additional lineage");
}
#[derive(Default)]
struct TestPreservationSource {
    records: Arc<Mutex<Vec<String>>>,
}

impl TestPreservationSource {
    fn inventory(&self) -> engram_domain::PreservationInventory {
        let records = self
            .records
            .lock()
            .expect("preservation records")
            .iter()
            .enumerate()
            .map(|(index, value)| engram_domain::PreservationRecord {
                record_class: "ordinary".into(),
                record_id: format!("ordinary-{index}"),
                content_digest: format!("{value:0>64}"),
                contributor_digest: "c".repeat(64),
            })
            .collect();
        let mut inventory = engram_domain::PreservationInventory {
            records,
            digest: String::new(),
        };
        inventory.digest = inventory.content_hash().expect("inventory hash");
        inventory
    }
}

#[async_trait]
impl PreservationRecordSource for TestPreservationSource {
    async fn inventory(
        &self,
        _scope: &Scope,
    ) -> engram_runtime::CoreResult<engram_domain::PreservationInventory> {
        Ok(self.inventory())
    }
}

fn preservation_source(records: Arc<Mutex<Vec<String>>>) -> Arc<dyn PreservationRecordSource> {
    Arc::new(TestPreservationSource { records })
}

fn empty_preservation_source() -> Arc<dyn PreservationRecordSource> {
    preservation_source(Arc::new(Mutex::new(Vec::new())))
}
fn file_backed_store(file: &tempfile::NamedTempFile) -> SqlCodeBindingRepository {
    let store = SqlCodeBindingRepository::open(file.path()).expect("store");
    let binding = CodeBinding {
        scope: scope(),
        repo_id: repo(),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    };
    block_on(store.register(RepositoryRegistration {
        repo_id: repo(),
        locator_id: "fixture".into(),
        acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
    }))
    .expect("register");
    block_on(store.bind(binding)).expect("bind");
    let generation = generation();
    block_on(store.begin_attempt(
        &scope(),
        &repo(),
        generation.clone(),
        CodeAttempt {
            id: "reset-attempt".into(),
            idempotency_key: "reset-key".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Created,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        },
    ))
    .expect("attempt");
    block_on(store.put_lineage(
        &scope(),
        "target-a",
        CodeLineage {
            repo_id: repo(),
            generation_id: generation.id,
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into(), "src/mod.rs".into()],
            span: None,
        },
    ))
    .expect("lineage");
    store
}

mod task_023 {
    use super::*;
    #[test]
    fn preview_replay_mismatch_denied() {
        let store = setup();
        let mut value = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        value.preview_hash = "0".repeat(64);
        assert!(block_on(store.journal_reset(journal(value))).is_err());
    }
    #[test]
    fn substituted_target_denied() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut value = journal(preview);
        value.target_fingerprints[0].1 = "0".repeat(64);
        assert!(block_on(store.journal_reset(value)).is_err());
    }
    #[test]
    fn subset_approval_denied() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut value = journal(preview);
        value.target_fingerprints.clear();
        assert!(block_on(store.journal_reset(value)).is_err());
    }
    #[test]
    fn ambiguous_lineage_blocks_preview() {
        let store = setup();
        let mut preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        preview.ambiguity.push(engram_domain::AmbiguousOwnership {
            record_id: "legacy-row".into(),
            reason: "record-level code ownership cannot be proven".into(),
        });
        assert!(
            preview.validate().is_err(),
            "an ambiguity report cannot carry a deletion list"
        );
    }
    #[test]
    fn forged_progress_is_rejected_before_journal_persistence() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut forged = journal(preview);
        forged.checkpoints = vec![
            "completed:target-a".into(),
            "reset-lane:lexical:done".into(),
            "preservation:verified".into(),
        ];
        let error = block_on(store.journal_reset(forged)).expect_err("forged progress denial");
        assert!(format!("{error:?}").contains("ResetProgressSupplied"));
        assert!(block_on(store.load_reset_journal("reset-1")).is_err());
    }

    #[test]
    fn journal_bound_to_preview_hash() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = journal(preview.clone());
        assert_eq!(value.preview_hash, preview.preview_hash);
        assert!(block_on(store.journal_reset(value)).is_ok());
    }
}

mod task_024 {
    use super::*;

    struct RealLexicalLane(Arc<engram_store_lexical::LexicalIndex>);
    #[async_trait]
    impl LexicalFeed for RealLexicalLane {
        async fn upsert(&self, target_id: &str, text: &str) -> engram_runtime::CoreResult<()> {
            self.0
                .upsert(target_id, text)
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }
        async fn upsert_batch(
            &self,
            entries: &[(String, String)],
        ) -> engram_runtime::CoreResult<()> {
            self.0
                .upsert_batch(entries)
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }
        async fn upsert_code_target(
            &self,
            target_id: &str,
            text: &str,
            generation: &str,
        ) -> engram_runtime::CoreResult<()> {
            self.0
                .upsert_stamped(
                    target_id,
                    text,
                    &engram_store_lexical::code_contributor_key(generation),
                )
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }
        async fn retire_generation(&self, generation: &str) -> engram_runtime::CoreResult<usize> {
            self.0
                .retire_contributor(&engram_store_lexical::code_contributor_key(generation))
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }
        async fn retire_code_target(
            &self,
            target_id: &str,
            generation_id: &str,
        ) -> engram_runtime::CoreResult<usize> {
            self.0
                .retire_target_contributor(
                    target_id,
                    &engram_store_lexical::code_contributor_key(generation_id),
                )
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }

        async fn delete_target(&self, target_id: &str) -> engram_runtime::CoreResult<()> {
            self.0
                .delete(target_id)
                .map_err(|error| engram_runtime::CoreError::Adapter {
                    adapter: "test-lexical".into(),
                    message: error.to_string(),
                })
        }
    }

    struct FailingLane {
        fail_vector_once: Mutex<bool>,
        calls: Mutex<Vec<ResetLane>>,
    }
    #[async_trait]
    impl ResetLaneLifecycle for FailingLane {
        async fn cleanup_and_rebuild(
            &self,
            lane: ResetLane,
            _scope: &Scope,
            _retired: &[engram_domain::CodeResetTarget],
            _preserved: &[engram_domain::PreservationRecord],
        ) -> engram_runtime::CoreResult<ResetLaneOutcome> {
            self.calls.lock().expect("calls").push(lane);
            if lane == ResetLane::Vector && *self.fail_vector_once.lock().expect("failure") {
                *self.fail_vector_once.lock().expect("failure") = false;
                return Err(engram_runtime::CoreError::Adapter {
                    adapter: "failing-lane-double".into(),
                    message: "forced vector failure".into(),
                });
            }
            Ok(if lane == ResetLane::Source {
                ResetLaneOutcome::NotRequired
            } else {
                ResetLaneOutcome::Completed
            })
        }
    }

    fn coordinator(
        store: SqlCodeBindingRepository,
        lanes: Arc<dyn ResetLaneLifecycle>,
    ) -> CodeBindingCoordinator {
        coordinator_with_source(store, lanes, empty_preservation_source())
    }

    fn coordinator_with_source(
        store: SqlCodeBindingRepository,
        lanes: Arc<dyn ResetLaneLifecycle>,
        source: Arc<dyn PreservationRecordSource>,
    ) -> CodeBindingCoordinator {
        CodeBindingCoordinator::new(
            Arc::new(store),
            Arc::new(ScopeSetAuthorizer::new(vec![scope()])),
            vec![],
        )
        .with_reset_lanes(lanes)
        .with_preservation_source(source)
    }

    fn actor() -> engram_domain::Actor {
        engram_domain::Actor {
            id: "reset-owner".into(),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        }
    }

    #[test]
    fn real_recipe_bundle_retires_code_and_preserves_ordinary_lexical_records() {
        let store = setup();
        let index = Arc::new(engram_store_lexical::LexicalIndex::new().expect("index"));
        index
            .upsert_stamped("target-a", "retired signal", "code:reset-generation")
            .expect("retired code");
        index
            .upsert_stamped("target-a", "surviving signal", "code:surviving-generation")
            .expect("surviving code contribution");
        index
            .upsert("ordinary-record", "preserved signal")
            .expect("ordinary record");
        let lanes = Arc::new(RecipeResetLaneLifecycle::new(
            Some(Arc::new(RealLexicalLane(index.clone()))),
            None,
            Arc::new(engram_retrieval::InMemoryGraphCache::new()),
        ));
        let coordinator = coordinator(store, lanes);
        let preview = block_on(coordinator.preview_reset(&actor(), &scope(), &repo()))
            .expect("coordinator preview");
        let approved_journal = block_on(coordinator.journal_reset(
            &actor(),
            journal(preview),
            "approved-by-fixture".into(),
        ))
        .expect("owner-approved journal");
        let applied = block_on(coordinator.apply_reset(&actor(), approved_journal))
            .expect("real composed reset");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "reset-lane:lexical:done")
        );
        assert!(
            index.search("retired", 10).expect("search").is_empty(),
            "retired code must not serve"
        );
        assert_eq!(
            index.search("preserved", 10).expect("search")[0].0,
            "ordinary-record"
        );
        assert_eq!(
            index.search("surviving", 10).expect("search")[0].0,
            "target-a",
            "targeted retirement keeps a shared target's surviving contributor document"
        );
    }

    #[test]
    fn interrupted_journal_resumes_exact_ids() {
        let store = setup();
        add_target(&store, "target-b");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = approved(&store, journal(preview));
        let applied = block_on(store.apply_reset(value.clone())).expect("first apply");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "completed:target-a")
        );
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "completed:target-b")
        );
        let resumed = block_on(store.apply_reset(value)).expect("resume from durable checkpoints");
        assert_eq!(resumed.checkpoints, applied.checkpoints);
    }

    #[test]
    fn unrelated_records_preserved() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let result =
            block_on(store.apply_reset(approved(&store, journal(preview)))).expect("apply");
        assert!(
            result
                .checkpoints
                .iter()
                .any(|item| item == "completed:target-a")
        );
        assert!(
            !result
                .checkpoints
                .iter()
                .any(|item| item == "completed:ordinary-record")
        );
    }

    #[test]
    fn leased_snapshot_not_deleted() {
        let store = setup_active();
        let now = chrono::Utc::now();
        let eligibility = block_on(store.eligibility(&scope())).expect("eligibility");
        block_on(store.acquire_read_lease(
            CodeReadLease {
                id: "live-reset-lease".into(),
                eligibility,
                fencing_token: 1,
                expires_at: now + chrono::Duration::minutes(5),
            },
            now,
        ))
        .expect("lease");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let error = block_on(store.apply_reset(approved(&store, journal(preview))))
            .expect_err("lease denial");
        assert!(format!("{error:?}").contains("ResetLeaseActive"));
    }

    #[test]
    fn stale_preview_rejected_at_apply() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = approved(&store, journal(preview));
        block_on(store.retire_lineage_targets(
            &scope(),
            &[(repo(), 0)],
            &[RetirementTarget {
                repo_id: repo().0,
                generation_id: generation().id,
                record_id: "target-a".into(),
            }],
        ))
        .expect("unrelated retirement");
        let durable = block_on(store.load_reset_journal("reset-1")).expect("journal");
        assert!(
            !durable
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint == "completed:target-a"),
            "retirement without reset context must not create trusted progress"
        );
        let error = block_on(store.apply_reset(value)).expect_err("stale retirement denial");
        assert!(format!("{error:?}").contains("ResetTargetSubstituted"));
    }

    #[test]
    fn target_substitution_rejected_at_apply() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut value = journal(preview);
        value.preview.targets[0].fingerprint = "0".repeat(64);
        assert!(block_on(store.apply_reset(value)).is_err());
    }

    #[test]
    fn unapproved_journal_subset_rejected_without_mutation() {
        let store = setup();
        add_target(&store, "target-b");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let approved_value = approved(&store, journal(preview));
        let mut supplied = approved_value.clone();
        supplied.preview.target_ids.truncate(1);
        supplied.preview.targets.truncate(1);
        supplied.target_fingerprints.truncate(1);
        rehash(&mut supplied);
        let error = block_on(store.apply_reset(supplied)).expect_err("subset denial");
        assert!(format!("{error:?}").contains("ResetJournalUnauthorized"));
        assert!(
            !block_on(store.record_lineage(&scope(), "target-a"))
                .expect("target a")
                .expect("lineage a")
                .retired
        );
        assert!(
            !block_on(store.record_lineage(&scope(), "target-b"))
                .expect("target b")
                .expect("lineage b")
                .retired
        );
    }

    #[test]
    fn invented_decision_rejected_without_mutation() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let approved_value = approved(&store, journal(preview));
        let mut supplied = approved_value.clone();
        supplied
            .authorizing_decisions
            .push("invented-decision".into());
        let error = block_on(store.apply_reset(supplied)).expect_err("decision denial");
        assert!(format!("{error:?}").contains("ResetJournalUnauthorized"));
        assert!(
            !block_on(store.record_lineage(&scope(), "target-a"))
                .expect("target")
                .expect("lineage")
                .retired
        );
    }

    #[test]
    fn preservation_is_verified_by_the_coordinator_and_persisted() {
        let records = Arc::new(Mutex::new(vec!["a".into()]));
        let coordinator = coordinator_with_source(
            setup(),
            Arc::new(FailingLane {
                fail_vector_once: Mutex::new(false),
                calls: Mutex::new(Vec::new()),
            }),
            preservation_source(records),
        );
        let preview = block_on(coordinator.preview_reset(&actor(), &scope(), &repo()))
            .expect("coordinator preview");
        assert_eq!(preview.preservation_inventory.records.len(), 1);
        let approved = block_on(coordinator.journal_reset(
            &actor(),
            journal(preview),
            "approved-by-fixture".into(),
        ))
        .expect("approval");
        let applied = block_on(coordinator.apply_reset(&actor(), approved)).expect("apply");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "preservation:verified")
        );
    }

    #[test]
    fn preservation_mutation_deletion_foreign_content_and_retry_are_denied_by_coordinator() {
        for replacement in [
            Vec::new(),
            vec!["changed".into()],
            vec!["a".into(), "foreign".into()],
        ] {
            let records = Arc::new(Mutex::new(vec!["a".into()]));
            let coordinator = coordinator_with_source(
                setup(),
                Arc::new(FailingLane {
                    fail_vector_once: Mutex::new(false),
                    calls: Mutex::new(Vec::new()),
                }),
                preservation_source(records.clone()),
            );
            let preview = block_on(coordinator.preview_reset(&actor(), &scope(), &repo()))
                .expect("coordinator preview");
            let approved = block_on(coordinator.journal_reset(
                &actor(),
                journal(preview),
                "approved-by-fixture".into(),
            ))
            .expect("approval");
            *records.lock().expect("records") = replacement;
            let error = block_on(coordinator.apply_reset(&actor(), approved.clone()))
                .expect_err("preservation drift denial");
            assert!(format!("{error:?}").contains("ResetPreservationFailed"));
            let durable = block_on(coordinator.store().load_reset_journal("reset-1"))
                .expect("failed journal persists");
            assert!(
                durable
                    .checkpoints
                    .iter()
                    .any(|item| item == "preservation:failed")
            );
            let retry = block_on(coordinator.apply_reset(&actor(), approved))
                .expect_err("drift retry remains denied");
            assert!(format!("{retry:?}").contains("ResetPreservationFailed"));
        }
    }

    #[test]
    fn missing_preservation_source_denies_preview_and_apply() {
        let store = setup();
        let coordinator = CodeBindingCoordinator::new(
            Arc::new(store),
            Arc::new(ScopeSetAuthorizer::new(vec![scope()])),
            vec![],
        );
        let preview_error = block_on(coordinator.preview_reset(&actor(), &scope(), &repo()))
            .expect_err("missing source denies preview");
        assert!(format!("{preview_error:?}").contains("PreservationSourceUnavailable"));

        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("cell preview");
        let approved = approved(&store, journal(preview));
        let coordinator = CodeBindingCoordinator::new(
            Arc::new(store),
            Arc::new(ScopeSetAuthorizer::new(vec![scope()])),
            vec![],
        );
        let apply_error = block_on(coordinator.apply_reset(&actor(), approved))
            .expect_err("missing source denies apply");
        assert!(format!("{apply_error:?}").contains("PreservationSourceUnavailable"));
        assert!(
            !block_on(coordinator.store().record_lineage(&scope(), "target-a"))
                .expect("target")
                .expect("lineage")
                .retired,
            "apply denial must happen before binding-cell retirement"
        );
    }

    #[test]
    fn durable_contributor_set_is_previewed_and_drift_is_rejected() {
        let file = tempfile::NamedTempFile::new().expect("database file");
        let store = file_backed_store(&file);
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        assert_eq!(
            preview.targets[0].lineage.contributors,
            vec!["src/lib.rs", "src/mod.rs"]
        );
        let value = approved(&store, journal(preview));
        let substituted = serde_json::to_string(&CodeLineage {
            repo_id: repo(),
            generation_id: generation().id,
            source_path: "src/lib.rs".into(),
            contributors: vec!["substituted-contributor".into(), "src/mod.rs".into()],
            span: None,
        })
        .expect("lineage json");
        rusqlite::Connection::open(file.path())
            .expect("database")
            .execute(
                "UPDATE code_stage_lineage SET record_json=?1 WHERE scope_key=?2 AND record_id=?3",
                rusqlite::params![
                    substituted,
                    serde_json::to_string(&scope()).expect("scope key"),
                    "target-a"
                ],
            )
            .expect("durable contributor substitution");
        let error = block_on(store.apply_reset(value)).expect_err("stale contributor denial");
        assert!(format!("{error:?}").contains("ResetPreviewStale"));
    }

    #[test]
    fn retirement_commit_carries_checkpoint_across_crash_before_return() {
        let file = tempfile::NamedTempFile::new().expect("database file");
        let store = file_backed_store(&file);
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let approved_journal = approved(&store, journal(preview));
        block_on(store.retire_lineage_targets_with_reset_progress(
            &scope(),
            &[(repo(), 0)],
            &[RetirementTarget {
                repo_id: repo().0,
                generation_id: generation().id,
                record_id: "target-a".into(),
            }],
            Some((&approved_journal.reset_id, &["target-a".into()])),
        ))
        .expect("atomic retirement and checkpoint commit");
        drop(store);

        // Model termination after the store transaction committed and before
        // its caller received a response or saved any coordinator lane state.
        let recovered_store = file_backed_store(&file);
        let recovered = block_on(
            coordinator(
                recovered_store,
                Arc::new(FailingLane {
                    fail_vector_once: Mutex::new(false),
                    calls: Mutex::new(Vec::new()),
                }),
            )
            .apply_reset(
                &engram_domain::Actor {
                    id: "reset-owner".into(),
                    kind: engram_domain::ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                approved_journal,
            ),
        )
        .expect("recovery uses committed completion checkpoint");
        assert!(
            recovered
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint == "completed:target-a")
        );
    }

    #[test]
    fn cross_store_failure_is_best_effort_surfaced() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let approved_journal = approved(&store, journal(preview));
        let lanes = Arc::new(FailingLane {
            fail_vector_once: Mutex::new(true),
            calls: Mutex::new(Vec::new()),
        });
        let coordinator = coordinator(store, lanes.clone());
        let actor = engram_domain::Actor {
            id: "reset-owner".into(),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        };
        let partial = block_on(coordinator.apply_reset(&actor, approved_journal.clone()))
            .expect("best effort result");
        assert!(
            partial
                .checkpoints
                .iter()
                .any(|item| item == "completed:target-a")
        );
        assert!(
            partial
                .checkpoints
                .iter()
                .any(|item| item == "reset-lane:lexical:done")
        );
        assert!(
            partial
                .checkpoints
                .iter()
                .any(|item| item == "reset-lane:vector:failed")
        );
        let resumed = block_on(coordinator.apply_reset(&actor, approved_journal)).expect("resume");
        assert!(
            resumed
                .checkpoints
                .iter()
                .any(|item| item == "reset-lane:vector:done")
        );
        assert!(
            resumed
                .checkpoints
                .iter()
                .all(|item| !item.ends_with(":failed"))
        );
        assert!(
            resumed
                .checkpoints
                .iter()
                .any(|item| item == "reset-lane:source:not-required"),
            "immutable retained snapshots are explicitly not-required, never falsely done"
        );
        assert!(
            lanes
                .calls
                .lock()
                .expect("calls")
                .contains(&ResetLane::Vector)
        );
    }
}
