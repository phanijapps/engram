//! TASK-023/TASK-024 PostgreSQL reset recovery proofs.
//!
//! These ignored, environment-gated cases use the real pgvector recipe and its
//! durable PostgreSQL code-binding cell. Run them only through the fixture helper.

use std::path::Path;

use engram_domain::{
    Actor, ActorKind, CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeLineage,
    CodeReceiptKind, CodeResetJournal, RepoId, Scope,
};
use engram_integration::code_binding::ScopeSetAuthorizer;
use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, MigrationMode};
use engram_knowledge::{
    CodeBindingRepository, CodeReadLease, CodeReadLeases, RepositoryRegistration,
};
use engram_runtime::CoreError;
use futures::executor::block_on;

fn url() -> String {
    std::env::var("ENGRAM_TEST_PG_URL").expect("ENGRAM_TEST_PG_URL is required")
}

fn unique() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    )
}

fn scope(id: &str) -> Scope {
    Scope {
        tenant: format!("reset-pg-{id}"),
        subject: None,
        workspace: Some("recovery".into()),
        session: None,
        environment: None,
    }
}
fn repo(id: &str) -> RepoId {
    RepoId(format!("reset-pg-repo-{id}"))
}
fn actor() -> Actor {
    Actor {
        id: "reset-pg-owner".into(),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}
fn config(dir: &Path) -> EngramConfig {
    let mut value = EngramConfig::new(
        dir.join("storage"),
        dir,
        engram_domain::types::ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".into(),
            model: "test".into(),
            dimensions: 384,
            prompt_profile: "query".into(),
            normalization: None,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_pgvector(&url());
    value.enable_vector = false;
    value
}
fn journal(preview: engram_domain::CodeResetPreview, id: &str) -> CodeResetJournal {
    CodeResetJournal {
        reset_id: format!("reset-pg-{id}"),
        preview_hash: preview.preview_hash.clone(),
        target_fingerprints: preview
            .targets
            .iter()
            .map(|target| (target.record_id.clone(), target.fingerprint.clone()))
            .collect(),
        authorizing_decisions: vec!["fixture-approved".into()],
        checkpoints: vec!["authorized".into()],
        preservation_digests: preview.preservation_digests.clone(),
        preview,
    }
}

/// Seed a published target through the recipe's own durable coordinator store.
fn seeded() -> (
    engram_integration::code_binding::CodeBindingCoordinator,
    Scope,
    RepoId,
    String,
) {
    let id = unique();
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("storage")).expect("storage");
    // Keep the path alive after return; recipe construction only needs it for retained-source setup.
    let provider = engram_backend_pgvector::open(&config(dir.path())).expect("open real pg recipe");
    std::mem::forget(dir);
    let coordinator = provider
        .code_bindings()
        .expect("recipe binding coordinator")
        .with_authorizer(std::sync::Arc::new(ScopeSetAuthorizer::new(vec![scope(
            &id,
        )])));
    let store = coordinator.store();
    let scope = scope(&id);
    let repo = repo(&id);
    block_on(store.register(RepositoryRegistration {
        repo_id: repo.clone(),
        locator_id: "pg-reset-fixture".into(),
        acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
    }))
    .expect("register");
    block_on(store.bind(CodeBinding {
        scope: scope.clone(),
        repo_id: repo.clone(),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }))
    .expect("bind");
    let generation = CodeGeneration {
        id: format!("generation-{id}"),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    };
    let attempt = CodeAttempt {
        id: format!("attempt-{id}"),
        idempotency_key: format!("key-{id}"),
        binding_version: 1,
        expected_epoch: 0,
        expected_active_generation: None,
        fencing_token: 1,
        state: CodeAttemptState::Created,
        receipt_kinds: vec![CodeReceiptKind::Knowledge],
    };
    block_on(store.begin_attempt(&scope, &repo, generation.clone(), attempt.clone()))
        .expect("attempt");
    block_on(store.record_receipt(engram_knowledge::StageReceipt {
        attempt_id: attempt.id.clone(),
        generation_id: generation.id.clone(),
        lane: "knowledge".into(),
        digest: "fixture-receipt".into(),
    }))
    .expect("receipt");
    block_on(store.mark_ready(&scope, &repo, &attempt.id, 1)).expect("ready");
    block_on(store.publish(&scope, &repo, &attempt.id, 0)).expect("publish");
    block_on(store.put_lineage(
        &scope,
        "target-a",
        CodeLineage {
            repo_id: repo.clone(),
            generation_id: generation.id.clone(),
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into(), "src/mod.rs".into()],
            span: None,
        },
    ))
    .expect("target lineage");
    (coordinator, scope, repo, id)
}

mod task_023 {
    use super::*;

    #[test]
    #[ignore = "requires marked disposable PostgreSQL fixture"]
    fn authorization_hash_and_ambiguous_ownership_are_behavioral() {
        let (coordinator, scope, repo, id) = seeded();
        let preview =
            block_on(coordinator.preview_reset(&actor(), &scope, &repo)).expect("real preview");
        assert_eq!(
            preview.targets[0].lineage.contributors,
            vec!["src/lib.rs", "src/mod.rs"]
        );
        let mut replay = journal(preview.clone(), &id);
        replay.preview_hash = "0".repeat(64);
        assert!(
            block_on(coordinator.journal_reset(&actor(), replay, "fixture-approved".into()))
                .is_err()
        );
        let mut ambiguous = preview;
        ambiguous.targets[0].lineage.source_path.clear();
        assert!(
            ambiguous.validate().is_err(),
            "ambiguous ownership cannot authorize deletion"
        );
    }

    #[test]
    #[ignore = "requires marked disposable PostgreSQL fixture"]
    fn substitution_subset_and_invented_decision_are_denied() {
        let (coordinator, scope, repo, id) = seeded();
        let preview =
            block_on(coordinator.preview_reset(&actor(), &scope, &repo)).expect("preview");
        let approved = block_on(coordinator.journal_reset(
            &actor(),
            journal(preview, &id),
            "fixture-approved".into(),
        ))
        .expect("approve");
        let mut substitute = approved.clone();
        substitute.preview.targets[0].fingerprint = "0".repeat(64);
        assert!(
            block_on(coordinator.apply_reset(&actor(), substitute)).is_err(),
            "substituted document is denied before mutation"
        );
        let mut subset = approved.clone();
        subset.preview.targets.clear();
        subset.preview.target_ids.clear();
        subset.target_fingerprints.clear();
        assert!(block_on(coordinator.apply_reset(&actor(), subset)).is_err());
        let mut invented = approved;
        invented.authorizing_decisions.push("invented".into());
        assert!(matches!(
            block_on(coordinator.apply_reset(&actor(), invented)),
            Err(CoreError::Conflict { .. })
        ));
    }
}

mod task_024 {
    use super::*;

    #[test]
    #[ignore = "requires marked disposable PostgreSQL fixture"]
    fn durable_journal_resume_and_recipe_lane_completion() {
        let (coordinator, scope, repo, id) = seeded();
        let preview =
            block_on(coordinator.preview_reset(&actor(), &scope, &repo)).expect("preview");
        let approved = block_on(coordinator.journal_reset(
            &actor(),
            journal(preview, &id),
            "fixture-approved".into(),
        ))
        .expect("approve");
        let applied = block_on(coordinator.apply_reset(&actor(), approved.clone()))
            .expect("apply through real recipe");
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
                .any(|item| item == "reset-lane:source:done")
        );
        let resumed =
            block_on(coordinator.apply_reset(&actor(), approved)).expect("durable resume");
        assert_eq!(resumed.checkpoints, applied.checkpoints);
    }

    #[test]
    #[ignore = "requires marked disposable PostgreSQL fixture"]
    fn live_lease_is_denied_inside_pg_retirement_fence() {
        let (coordinator, scope, repo, id) = seeded();
        let store = coordinator.store();
        let eligibility = block_on(store.eligibility(&scope)).expect("eligibility");
        let now = chrono::Utc::now();
        block_on(store.acquire_read_lease(
            CodeReadLease {
                id: format!("lease-{id}"),
                eligibility,
                fencing_token: 1,
                expires_at: now + chrono::Duration::minutes(5),
            },
            now,
        ))
        .expect("lease");
        let preview =
            block_on(coordinator.preview_reset(&actor(), &scope, &repo)).expect("preview");
        let approved = block_on(coordinator.journal_reset(
            &actor(),
            journal(preview, &id),
            "fixture-approved".into(),
        ))
        .expect("approve");
        let error = block_on(coordinator.apply_reset(&actor(), approved))
            .expect_err("live lease blocks reset");
        assert!(format!("{error:?}").contains("ResetLeaseActive"));
    }
}
