//! PostgreSQL parity cases require a parent-authorized marked disposable database.
use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
};
use engram_knowledge::{CodeBindingRepository, CodeReadLease, CodeReadLeases, CoreError};
use engram_store_pgvector::{PgCodeBindingRepository, PgConnection};
use futures::executor::block_on;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
};

struct BoundedChild {
    child: std::process::Child,
    output: BufReader<std::process::ChildStdout>,
}

impl Drop for BoundedChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn neutral_repository_port_is_send_sync() {
    fn assert_port<T: CodeBindingRepository + Send + Sync>() {}
    assert_port::<PgCodeBindingRepository>();
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_scope_epoch_and_read_lease_parity() {
    let store = repository();
    let suffix = unique();
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
    let generation = generation(&format!("generation-{suffix}"));
    let attempt = attempt(&format!("attempt-{suffix}"), 0, None, 1);
    block_on(store.begin_attempt(
        &test_scope,
        &first.repo_id,
        generation.clone(),
        attempt.clone(),
    ))
    .expect("begin attempt");
    receipt(&store, &attempt, &generation);
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

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_cross_process_binding_publication_and_read_lease_fencing() {
    if let Ok(mode) = std::env::var("TERRA_PG_RACE_MODE") {
        race_child(&mode);
        return;
    }
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("race-{suffix}"), "factory-test");
    let first = binding(&test_scope, &format!("race-first-{suffix}"));
    let second = binding(&test_scope, &format!("race-second-{suffix}"));
    block_on(store.bind(first.clone())).expect("bind first");
    let old_generation = generation(&format!("race-old-{suffix}"));
    let old_attempt = attempt(&format!("race-old-attempt-{suffix}"), 0, None, 1);
    block_on(store.begin_attempt(
        &test_scope,
        &first.repo_id,
        old_generation.clone(),
        old_attempt.clone(),
    ))
    .expect("begin old publication");
    receipt(&store, &old_attempt, &old_generation);
    block_on(store.publish(&test_scope, &first.repo_id, &old_attempt.id, 0)).expect("publish old");

    let before_child_bind =
        block_on(store.eligibility(&test_scope)).expect("eligibility before child bind");
    let mut bind_reader = spawn_race_child("read-stale", &test_scope, &first.repo_id.0, "", "");
    wait_ready(&mut bind_reader);
    run_race_child(
        "bind",
        &test_scope,
        &second.repo_id.0,
        "",
        "",
        &format!(
            "BOUND|{}|{}|{}|{}",
            test_scope.tenant,
            test_scope.workspace.as_deref().expect("workspace"),
            second.repo_id.0,
            before_child_bind.epoch + 1
        ),
    );
    release_reader(bind_reader, "STALE");
    let after_bind =
        block_on(store.eligibility(&test_scope)).expect("eligibility after child bind");
    assert_eq!(after_bind.epoch, before_child_bind.epoch + 1);
    assert_eq!(
        block_on(store.get_binding(&test_scope, &second.repo_id)).expect("child-bound record"),
        Some(second.clone())
    );
    assert_eq!(after_bind.repository_ids, vec![first.repo_id.0.clone()]);
    assert_eq!(after_bind.generations, vec![old_generation.id.clone()]);

    let next_generation = generation(&format!("race-next-{suffix}"));
    let next_attempt = attempt(
        &format!("race-next-attempt-{suffix}"),
        1,
        Some(old_generation.id.clone()),
        2,
    );
    block_on(store.begin_attempt(
        &test_scope,
        &first.repo_id,
        next_generation.clone(),
        next_attempt.clone(),
    ))
    .expect("prepare next publication");
    receipt(&store, &next_attempt, &next_generation);
    let lease = block_on(store.acquire_read_lease(
        CodeReadLease {
            id: format!("race-lease-{suffix}"),
            eligibility: after_bind.clone(),
            fencing_token: 1,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(2),
        },
        chrono::Utc::now(),
    ))
    .expect("acquire valid lease");
    let renewed = block_on(store.renew_read_lease(
        &lease,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::minutes(3),
    ))
    .expect("renew valid lease");
    assert!(matches!(
        block_on(store.release_read_lease(&lease)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));

    let mut publication_reader =
        spawn_race_child("read-stale", &test_scope, &first.repo_id.0, "", "");
    wait_ready(&mut publication_reader);
    run_race_child(
        "publish",
        &test_scope,
        &first.repo_id.0,
        &next_attempt.id,
        "1",
        "PUBLISHED",
    );
    release_reader(publication_reader, "STALE");
    let after_publish =
        block_on(store.eligibility(&test_scope)).expect("eligibility after child publish");
    assert!(after_publish.epoch > after_bind.epoch);
    assert!(after_publish.generations.contains(&next_generation.id));
    assert!(matches!(
        block_on(store.renew_read_lease(
            &renewed,
            chrono::Utc::now(),
            chrono::Utc::now() + chrono::Duration::minutes(4),
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    let retained = block_on(store.retained_generations(&test_scope, chrono::Utc::now(), 0))
        .expect("retained generations");
    assert!(retained.contains(&old_generation.id));
    assert!(retained.contains(&next_generation.id));
    block_on(store.release_read_lease(&renewed)).expect("release current fence");
}

fn race_child(mode: &str) {
    let test_scope = scope(
        &std::env::var("TERRA_PG_RACE_WORKSPACE").expect("race workspace"),
        &std::env::var("TERRA_PG_RACE_TENANT").expect("race tenant"),
    );
    let repo = RepoId(std::env::var("TERRA_PG_RACE_REPO").expect("race repo"));
    let store = repository();
    match mode {
        "read-stale" => {
            let eligibility = block_on(store.eligibility(&test_scope)).expect("child eligibility");
            println!("READY");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            let result = block_on(store.acquire_read_lease(
                CodeReadLease {
                    id: format!("child-stale-{}", unique()),
                    eligibility,
                    fencing_token: 1,
                    expires_at: chrono::Utc::now() + chrono::Duration::minutes(1),
                },
                chrono::Utc::now(),
            ));
            assert!(
                matches!(result, Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
            );
            println!("STALE");
        }
        "bind" => {
            let before = block_on(store.eligibility(&test_scope)).expect("child bind before");
            let bound = binding(&test_scope, &repo.0);
            block_on(store.bind(bound.clone())).expect("child bind");
            let after = block_on(store.eligibility(&test_scope)).expect("child bind after");
            assert!(after.epoch > before.epoch);
            assert_eq!(
                block_on(store.bind(bound)).expect("child idempotent bind"),
                binding(&test_scope, &repo.0)
            );
            assert_eq!(
                block_on(store.eligibility(&test_scope))
                    .expect("child idempotent epoch")
                    .epoch,
                after.epoch
            );
            println!(
                "BOUND|{}|{}|{}|{}",
                test_scope.tenant,
                test_scope.workspace.as_deref().expect("workspace"),
                repo.0,
                after.epoch
            );
        }
        "publish" => {
            let attempt = std::env::var("TERRA_PG_RACE_ATTEMPT").expect("race attempt");
            let epoch = std::env::var("TERRA_PG_RACE_EPOCH")
                .expect("race epoch")
                .parse()
                .expect("race epoch number");
            block_on(store.publish(&test_scope, &repo, &attempt, epoch)).expect("child publish");
            println!("PUBLISHED");
        }
        _ => panic!("unknown race mode"),
    }
}

fn spawn_race_child(
    mode: &str,
    scope: &Scope,
    repo: &str,
    attempt: &str,
    epoch: &str,
) -> BoundedChild {
    let mut child = Command::new("timeout")
        .arg("30s")
        .arg(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "postgres_cross_process_binding_publication_and_read_lease_fencing",
            "--ignored",
            "--nocapture",
        ])
        .env("TERRA_PG_RACE_MODE", mode)
        .env("TERRA_PG_RACE_TENANT", &scope.tenant)
        .env(
            "TERRA_PG_RACE_WORKSPACE",
            scope.workspace.as_deref().expect("workspace"),
        )
        .env("TERRA_PG_RACE_REPO", repo)
        .env("TERRA_PG_RACE_ATTEMPT", attempt)
        .env("TERRA_PG_RACE_EPOCH", epoch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fresh child process");
    let output = BufReader::new(child.stdout.take().expect("child stdout"));
    BoundedChild { child, output }
}

fn wait_ready(child: &mut BoundedChild) {
    for _ in 0..20 {
        let mut line = String::new();
        assert!(
            child
                .output
                .read_line(&mut line)
                .expect("read child barrier")
                > 0,
            "child exited before barrier"
        );
        if line.contains("READY") {
            return;
        }
    }
    panic!("child did not reach bounded barrier");
}

fn release_reader(mut child: BoundedChild, expected: &str) {
    child
        .child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(b"G")
        .expect("release child");
    let mut output = String::new();
    child
        .output
        .read_to_string(&mut output)
        .expect("read child output");
    assert!(
        child.child.wait().expect("wait child").success(),
        "child failed"
    );
    assert!(
        output.lines().any(|line| line == expected),
        "missing child result"
    );
}

fn run_race_child(
    mode: &str,
    scope: &Scope,
    repo: &str,
    attempt: &str,
    epoch: &str,
    expected: &str,
) {
    let mut child = spawn_race_child(mode, scope, repo, attempt, epoch);
    drop(child.child.stdin.take());
    let mut text = String::new();
    child
        .output
        .read_to_string(&mut text)
        .expect("read child output");
    assert!(
        child.child.wait().expect("wait child").success(),
        "child failed"
    );
    assert!(
        text.split_whitespace().any(|token| token == expected),
        "missing child result"
    );
}

fn repository() -> PgCodeBindingRepository {
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
    PgCodeBindingRepository::new(connection).expect("initialize code-binding port")
}

fn receipt(store: &PgCodeBindingRepository, attempt: &CodeAttempt, generation: &CodeGeneration) {
    block_on(store.record_receipt(engram_knowledge::StageReceipt {
        attempt_id: attempt.id.clone(),
        generation_id: generation.id.clone(),
        lane: "knowledge".into(),
        digest: "receipt".into(),
    }))
    .expect("record receipt");
}

fn unique() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
        .to_string()
}

fn generation(id: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    }
}

fn attempt(
    id: &str,
    expected_epoch: u64,
    expected_active_generation: Option<String>,
    fencing_token: u64,
) -> CodeAttempt {
    CodeAttempt {
        id: id.into(),
        idempotency_key: id.into(),
        binding_version: 1,
        expected_epoch,
        expected_active_generation,
        fencing_token,
        state: CodeAttemptState::Ready,
        receipt_kinds: vec![CodeReceiptKind::Knowledge],
    }
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
