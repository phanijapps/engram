//! PostgreSQL parity cases require a parent-authorized marked disposable database.
use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
};
use engram_knowledge::{CodeBindingRepository, CodeReadLease, CodeReadLeases, CoreError};
use engram_store_pgvector::{PgCodeBindingRepository, PgConnection};
use futures::executor::block_on;

#[test]
fn neutral_repository_port_is_send_sync() {
    fn assert_port<T: CodeBindingRepository + Send + Sync>() {}
    assert_port::<PgCodeBindingRepository>();
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_scope_epoch_and_read_lease_parity() {
    let connection = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("connect marked disposable PostgreSQL");
    let marker = connection.block_on(async {
        connection
            .client
            .query_one(
                "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()",
                &[],
            )
            .await
            .expect("read disposable database marker")
            .get::<_, Option<String>>(0)
    });
    assert_eq!(
        marker.as_deref(),
        Some("engram.factory.job=010-project-bound-branch-indexing; disposable=true")
    );
    let store = PgCodeBindingRepository::new(connection).expect("initialize code-binding port");
    let suffix = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let test_scope = scope(&format!("scope-{suffix}"), "factory-test");
    let first = binding(&test_scope, &format!("repo-first-{suffix}"));
    let second = binding(&test_scope, &format!("repo-second-{suffix}"));
    block_on(store.bind(first.clone())).expect("first binding");
    let before_second_bind = block_on(store.eligibility(&test_scope)).expect("first eligibility");
    assert_eq!(before_second_bind.epoch, 1);
    let now = chrono::Utc::now();
    let lease = CodeReadLease {
        id: format!("lease-{suffix}"),
        eligibility: before_second_bind.clone(),
        fencing_token: 1,
        expires_at: now + chrono::Duration::minutes(1),
    };
    assert!(matches!(
        block_on(store.acquire_read_lease(
            CodeReadLease {
                fencing_token: 0,
                ..lease.clone()
            },
            now,
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    block_on(store.bind(second.clone())).expect("second binding");
    let after_second_bind = block_on(store.eligibility(&test_scope)).expect("second eligibility");
    assert!(after_second_bind.epoch > before_second_bind.epoch);
    assert!(matches!(
        block_on(store.acquire_read_lease(lease.clone(), now)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert_eq!(
        block_on(store.bind(second.clone())).expect("idempotent bind"),
        second
    );
    assert_eq!(
        block_on(store.eligibility(&test_scope))
            .expect("idempotent eligibility")
            .epoch,
        after_second_bind.epoch
    );
    let generation = CodeGeneration {
        id: format!("generation-{suffix}"),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    };
    let attempt = CodeAttempt {
        id: format!("attempt-{suffix}"),
        idempotency_key: format!("attempt-{suffix}"),
        binding_version: 1,
        expected_epoch: 0,
        expected_active_generation: None,
        fencing_token: 1,
        state: CodeAttemptState::Ready,
        receipt_kinds: vec![CodeReceiptKind::Knowledge],
    };
    block_on(store.begin_attempt(
        &test_scope,
        &first.repo_id,
        generation.clone(),
        attempt.clone(),
    ))
    .expect("begin attempt");
    block_on(store.record_receipt(engram_knowledge::StageReceipt {
        attempt_id: attempt.id.clone(),
        generation_id: generation.id.clone(),
        lane: "knowledge".into(),
        digest: "receipt".into(),
    }))
    .expect("record receipt");
    let before_publish = block_on(store.eligibility(&test_scope)).expect("pre-publish eligibility");
    let published =
        block_on(store.publish(&test_scope, &first.repo_id, &attempt.id, 0)).expect("publish");
    assert_eq!(
        published.active_generation.as_deref(),
        Some(generation.id.as_str())
    );
    let after_publish = block_on(store.eligibility(&test_scope)).expect("post-publish eligibility");
    assert!(after_publish.epoch > before_publish.epoch);
    assert!(matches!(
        block_on(store.acquire_read_lease(CodeReadLease { id: format!("stale-{suffix}"), eligibility: before_publish, fencing_token: 1, expires_at: now + chrono::Duration::minutes(1) }, now)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    block_on(store.publish(&test_scope, &first.repo_id, &attempt.id, 0))
        .expect("idempotent publish");
    assert_eq!(
        block_on(store.eligibility(&test_scope))
            .expect("replayed eligibility")
            .epoch,
        after_publish.epoch
    );
    let other_scope = scope(&format!("scope-other-{suffix}"), "factory-test");
    let other_tenant = scope(&format!("scope-tenant-{suffix}"), "factory-other");
    block_on(store.bind(binding(&other_scope, &format!("repo-other-{suffix}"))))
        .expect("other scope");
    block_on(store.bind(binding(&other_tenant, &format!("repo-tenant-{suffix}"))))
        .expect("other tenant");
    assert_eq!(
        block_on(store.eligibility(&test_scope)).expect("scope isolation"),
        after_publish
    );
}

fn scope(workspace: &str, tenant: &str) -> Scope {
    Scope {
        tenant: tenant.into(),
        subject: None,
        workspace: Some(workspace.into()),
        session: None,
        environment: None,
    }
}

fn binding(scope: &Scope, repo: &str) -> CodeBinding {
    CodeBinding {
        scope: scope.clone(),
        repo_id: RepoId(repo.into()),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }
}
