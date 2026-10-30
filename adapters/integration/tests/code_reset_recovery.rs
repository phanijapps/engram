use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeLineage, CodeReceiptKind,
    CodeResetJournal, RepoId, Scope,
};
use engram_knowledge::{
    CodeBindingRepository, CodeReadLease, CodeReadLeases, RepositoryRegistration, RetirementTarget,
};
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;

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
fn add_preserved(store: &SqlCodeBindingRepository, id: &str) {
    block_on(store.put_lineage(
        &scope(),
        id,
        CodeLineage {
            repo_id: RepoId("ordinary-repo".into()),
            generation_id: "ordinary-generation".into(),
            source_path: format!("notes/{id}.md"),
            contributors: vec!["ordinary-source".into()],
            span: None,
        },
    ))
    .expect("preserved lineage");
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
    fn ordinary_preservation_digests_present() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        assert!(!preview.preservation_digests.is_empty());
        assert!(
            preview
                .preservation_digests
                .iter()
                .all(|digest| digest.len() == 64)
        );
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
        .expect("advance epoch");
        assert!(block_on(store.apply_reset(value)).is_err());
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
    fn preservation_digests_revalidated_after_apply() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let applied =
            block_on(store.apply_reset(approved(&store, journal(preview)))).expect("apply");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "preservation-verified")
        );
    }

    #[test]
    fn preserved_record_mutation_between_preview_and_apply_is_rejected() {
        let file = tempfile::NamedTempFile::new().expect("database file");
        let store = file_backed_store(&file);
        add_preserved(&store, "ordinary-record");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = approved(&store, journal(preview));
        let changed = serde_json::to_string(&CodeLineage {
            repo_id: RepoId("ordinary-repo".into()),
            generation_id: "ordinary-generation".into(),
            source_path: "notes/ordinary-record-rewritten.md".into(),
            contributors: vec!["ordinary-source".into()],
            span: None,
        })
        .expect("lineage json");
        rusqlite::Connection::open(file.path())
            .expect("database")
            .execute(
                "UPDATE code_stage_lineage SET record_json=?1 WHERE scope_key=?2 AND record_id=?3",
                rusqlite::params![
                    changed,
                    serde_json::to_string(&scope()).expect("scope key"),
                    "ordinary-record"
                ],
            )
            .expect("durable mutation");
        let error = block_on(store.apply_reset(value)).expect_err("preservation drift denial");
        assert!(format!("{error:?}").contains("ResetPreservationFailed"));
    }

    #[test]
    fn preserved_record_deletion_between_preview_and_apply_is_rejected() {
        let file = tempfile::NamedTempFile::new().expect("database file");
        let store = file_backed_store(&file);
        add_preserved(&store, "ordinary-record");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = approved(&store, journal(preview));
        rusqlite::Connection::open(file.path())
            .expect("database")
            .execute(
                "DELETE FROM code_stage_lineage WHERE scope_key=?1 AND record_id=?2",
                rusqlite::params![
                    serde_json::to_string(&scope()).expect("scope key"),
                    "ordinary-record"
                ],
            )
            .expect("durable deletion");
        let error = block_on(store.apply_reset(value)).expect_err("preservation drift denial");
        assert!(format!("{error:?}").contains("ResetPreservationFailed"));
    }

    #[test]
    fn untouched_preserved_record_is_verified() {
        let store = setup();
        add_preserved(&store, "ordinary-record");
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        assert_eq!(preview.preservation_inventory.records.len(), 1);
        let applied = block_on(store.apply_reset(approved(&store, journal(preview))))
            .expect("preserved record verified");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "preservation-verified")
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
    fn cross_store_failure_is_best_effort_surfaced() {
        // Completed IDs are durable-resume evidence, not a cross-store ACID
        // rollback claim; a later cell failure resumes from this checkpoint.
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let applied =
            block_on(store.apply_reset(approved(&store, journal(preview)))).expect("local apply");
        assert!(
            applied
                .checkpoints
                .iter()
                .any(|item| item == "completed:target-a")
        );
    }
}
