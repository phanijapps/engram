//! PostgreSQL parity cases require a parent-authorized marked disposable database.
use async_trait::async_trait;
use engram_domain::{
    Actor, ActorKind, CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeReceiptKind,
    Id, RepoId, Scope,
};
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator, CodeGenerationStages,
    CodeRebuildRequest,
};
use engram_knowledge::{
    CodeBindingRepository, CodeReadLease, CodeReadLeases, CoreError, CoreResult, StageReceipt,
};
use engram_store_pgvector::{PgCodeBindingRepository, PgConnection};
use futures::executor::block_on;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

const LEASE_ID_ADVISORY_NAMESPACE: i32 = 0x454e_4752;

struct BoundedChild {
    child: std::process::Child,
    output: BufReader<std::process::ChildStdout>,
    backend_pid: Option<i32>,
}

impl Drop for BoundedChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct FixtureLeaseInsertBarrier {
    connection: PgConnection,
    trigger: String,
    function: String,
    key: String,
}

impl FixtureLeaseInsertBarrier {
    fn install(url: &str, lease_id: &str, suffix: &str) -> Self {
        let trigger = format!("terra_lease_insert_trigger_{suffix}");
        let function = format!("terra_lease_insert_function_{suffix}");
        let barrier_key = format!("terra-lease-insert-barrier-{suffix}");
        let connection = PgConnection::connect(url).expect("fixture barrier connection");
        let statement = format!(
            "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.lease_id = '{lease_id}' THEN PERFORM pg_advisory_xact_lock(hashtextextended('{barrier_key}', 0)); END IF; RETURN NEW; END; $$; CREATE TRIGGER {trigger} BEFORE INSERT ON code_read_leases FOR EACH ROW EXECUTE FUNCTION {function}()"
        );
        block_on(connection.client.batch_execute(&statement))
            .expect("install fixture insert barrier");
        Self {
            connection,
            trigger,
            function,
            key: barrier_key,
        }
    }

    fn cleanup(&self) {
        let statement = format!(
            "DROP TRIGGER IF EXISTS {} ON code_read_leases; DROP FUNCTION IF EXISTS {}()",
            self.trigger, self.function
        );
        block_on(self.connection.client.batch_execute(&statement)).expect("remove fixture barrier");
        let absent: bool = block_on(self.connection.client.query_one(
            "SELECT NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname=$1) AND NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname=$2)",
            &[&self.trigger, &self.function],
        ))
        .expect("verify fixture barrier removal")
        .get(0);
        assert!(absent, "fixture barrier objects remain");
    }
}

impl Drop for FixtureLeaseInsertBarrier {
    fn drop(&mut self) {
        let statement = format!(
            "DROP TRIGGER IF EXISTS {} ON code_read_leases; DROP FUNCTION IF EXISTS {}()",
            self.trigger, self.function
        );
        let _ = block_on(self.connection.client.batch_execute(&statement));
    }
}

struct FixtureLeaseReleaseBarrier {
    connection: PgConnection,
    trigger: String,
    function: String,
    key: String,
}

impl FixtureLeaseReleaseBarrier {
    fn install(url: &str, lease_id: &str, suffix: &str) -> Self {
        let trigger = format!("terra_lease_release_trigger_{suffix}");
        let function = format!("terra_lease_release_function_{suffix}");
        let barrier_key = format!("terra-lease-release-barrier-{suffix}");
        let connection = PgConnection::connect(url).expect("fixture release barrier connection");
        let statement = format!(
            "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.lease_id = '{lease_id}' THEN PERFORM pg_advisory_xact_lock(hashtextextended('{barrier_key}', 0)); END IF; RETURN OLD; END; $$; CREATE TRIGGER {trigger} BEFORE DELETE ON code_read_leases FOR EACH ROW EXECUTE FUNCTION {function}()"
        );
        block_on(connection.client.batch_execute(&statement))
            .expect("install fixture release barrier");
        Self {
            connection,
            trigger,
            function,
            key: barrier_key,
        }
    }

    fn cleanup(&self) {
        let statement = format!(
            "DROP TRIGGER IF EXISTS {} ON code_read_leases; DROP FUNCTION IF EXISTS {}()",
            self.trigger, self.function
        );
        block_on(self.connection.client.batch_execute(&statement))
            .expect("remove fixture release barrier");
        let absent: bool = block_on(self.connection.client.query_one(
            "SELECT NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname=$1) AND NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname=$2)",
            &[&self.trigger, &self.function],
        ))
        .expect("verify fixture release barrier removal")
        .get(0);
        assert!(absent, "fixture release barrier objects remain");
    }
}

impl Drop for FixtureLeaseReleaseBarrier {
    fn drop(&mut self) {
        let statement = format!(
            "DROP TRIGGER IF EXISTS {} ON code_read_leases; DROP FUNCTION IF EXISTS {}()",
            self.trigger, self.function
        );
        let _ = block_on(self.connection.client.batch_execute(&statement));
    }
}

#[test]
fn neutral_repository_port_is_send_sync() {
    fn assert_port<T: CodeBindingRepository + Send + Sync>() {}
    assert_port::<PgCodeBindingRepository>();
}

struct AllowRebuild;

#[async_trait]
impl CodeBindingAuthorizer for AllowRebuild {
    async fn authorize(
        &self,
        _: &Actor,
        _: &Scope,
        _: &RepoId,
        _: CodeAuthorityAction,
    ) -> CoreResult<()> {
        Ok(())
    }
}

struct ReceiptStages;

#[async_trait]
impl CodeGenerationStages for ReceiptStages {
    async fn stage(
        &self,
        lane: &CodeReceiptKind,
        _: &CodeBinding,
        generation: &CodeGeneration,
        attempt: &CodeAttempt,
    ) -> CoreResult<StageReceipt> {
        Ok(StageReceipt {
            attempt_id: attempt.id.clone(),
            generation_id: generation.id.clone(),
            lane: serde_json::to_value(lane)
                .expect("serialize lane")
                .as_str()
                .expect("lane name")
                .into(),
            digest: "c".repeat(64),
        })
    }
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_cross_process_zero_fence_contenders_allocate_distinct_fences() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("fence-race-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("fence-race-repo-{suffix}"));
    block_on(store.bind(bound.clone())).expect("bind");
    let mut holder = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("holder connection");
    let observer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("observer connection");
    let scope_key = serde_json::to_string(&test_scope).expect("scope key");
    let holder_pid: i32 = block_on(holder.client.query_one("SELECT pg_backend_pid()", &[]))
        .expect("holder backend pid")
        .get(0);
    let tx = block_on(holder.client.transaction()).expect("holder transaction");
    block_on(tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("{}:{scope_key}", test_scope.tenant)],
    ))
    .expect("hold authority");
    let mut first = spawn_race_child(
        "begin-zero",
        &test_scope,
        &bound.repo_id.0,
        &format!("fence-race-a-{suffix}"),
        "",
    );
    let mut second = spawn_race_child(
        "begin-zero",
        &test_scope,
        &bound.repo_id.0,
        &format!("fence-race-b-{suffix}"),
        "",
    );
    wait_ready(&mut first);
    wait_ready(&mut second);
    let first_pid = first.backend_pid.expect("first child backend pid");
    let second_pid = second.backend_pid.expect("second child backend pid");
    let mut waiting = false;
    for _ in 0..1_000 {
        let row = block_on(observer.client.query_one(
            "SELECT count(DISTINCT waiter.pid) FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid IN ($2,$3) AND NOT waiter.granted",
            &[&holder_pid, &first_pid, &second_pid],
        ))
        .expect("observe exact contender authority");
        if row.get::<_, i64>(0) >= 2 {
            waiting = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        waiting,
        "both independent child processes wait on the scope authority"
    );
    block_on(tx.commit()).expect("release authority");
    let outputs = [collect_child(first), collect_child(second)];
    let mut fences: Vec<u64> = outputs
        .iter()
        .map(|output| {
            output
                .lines()
                .find_map(|line| line.strip_prefix("FENCE|"))
                .expect("child fence")
                .parse()
                .expect("fence number")
        })
        .collect();
    fences.sort();
    assert_eq!(fences, vec![1, 2]);
    for (attempt_id, generation_id) in [
        (
            format!("fence-race-a-{suffix}"),
            format!("race-generation-fence-race-a-{suffix}"),
        ),
        (
            format!("fence-race-b-{suffix}"),
            format!("race-generation-fence-race-b-{suffix}"),
        ),
    ] {
        let stored = block_on(store.get_attempt(&test_scope, &bound.repo_id, &attempt_id))
            .expect("stored contender")
            .expect("attempt row");
        assert_eq!(stored.0.id, generation_id);
        assert!(stored.1.fencing_token > 0);
    }
}

fn collect_child(mut child: BoundedChild) -> String {
    drop(child.child.stdin.take());
    let mut output = String::new();
    child
        .output
        .read_to_string(&mut output)
        .expect("read child output");
    assert!(
        child.child.wait().expect("wait child").success(),
        "child failed"
    );
    output
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_zero_fence_replay_and_superseded_attempt_rejection() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("fence-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("fence-repo-{suffix}"));
    block_on(store.bind(bound.clone())).expect("bind");
    let generation_a = generation(&format!("fence-generation-a-{suffix}"));
    let request_a = attempt(&format!("fence-attempt-a-{suffix}"), 0, None, 0);
    let allocated_a = block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        generation_a.clone(),
        request_a.clone(),
    ))
    .expect("allocate A");
    assert_eq!(allocated_a.fencing_token, 1);
    assert_eq!(
        block_on(store.begin_attempt(
            &test_scope,
            &bound.repo_id,
            generation_a.clone(),
            request_a.clone()
        ))
        .expect("zero replay"),
        allocated_a
    );
    let mut wrong_positive = request_a.clone();
    wrong_positive.fencing_token = allocated_a.fencing_token + 1;
    assert!(matches!(
        block_on(store.begin_attempt(
            &test_scope,
            &bound.repo_id,
            generation_a.clone(),
            wrong_positive
        )),
        Err(CoreError::InvalidRequest { .. })
    ));
    assert!(matches!(
        block_on(store.begin_attempt(
            &test_scope,
            &bound.repo_id,
            generation(&format!("fence-altered-{suffix}")),
            request_a.clone()
        )),
        Err(CoreError::InvalidRequest { .. })
    ));
    let generation_b = generation(&format!("fence-generation-b-{suffix}"));
    let request_b = attempt(&format!("fence-attempt-b-{suffix}"), 0, None, 0);
    let allocated_b =
        block_on(store.begin_attempt(&test_scope, &bound.repo_id, generation_b.clone(), request_b))
            .expect("allocate B");
    assert_eq!(allocated_b.fencing_token, 2);
    receipt(&store, &allocated_a, &generation_a);
    assert!(
        matches!(block_on(store.mark_ready(&test_scope, &bound.repo_id, &allocated_a.id, allocated_a.fencing_token)), Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
    );
    assert!(
        matches!(block_on(store.publish(&test_scope, &bound.repo_id, &allocated_a.id, 0)), Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
    );
    receipt(&store, &allocated_b, &generation_b);
    block_on(store.mark_ready(
        &test_scope,
        &bound.repo_id,
        &allocated_b.id,
        allocated_b.fencing_token,
    ))
    .expect("ready latest B");
    assert_eq!(
        block_on(store.publish(&test_scope, &bound.repo_id, &allocated_b.id, 0))
            .expect("publish latest B")
            .active_generation
            .as_deref(),
        Some(generation_b.id.as_str())
    );
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_unchanged_coordinator_allocates_zero_fence_and_replays() {
    let store = std::sync::Arc::new(repository());
    let suffix = unique();
    let test_scope = scope(&format!("coordinator-{suffix}"), "factory-test");
    let repo = RepoId(format!("coordinator-repo-{suffix}"));
    block_on(store.bind(binding(&test_scope, &repo.0))).expect("bind");
    let coordinator = CodeBindingCoordinator::new(store, std::sync::Arc::new(AllowRebuild), vec![]);
    let request = CodeRebuildRequest {
        scope: test_scope,
        repo_id: repo,
        generation: generation(&format!("coordinator-generation-{suffix}")),
        attempt_id: format!("coordinator-attempt-{suffix}"),
        idempotency_key: format!("coordinator-request-{suffix}"),
    };
    let actor = Actor {
        id: Id::from("factory-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    };
    let stages = ReceiptStages;
    let published =
        block_on(coordinator.rebuild(&actor, request, &stages)).expect("zero allocation");
    assert!(published.active_generation.is_some());
    let replay = CodeRebuildRequest {
        scope: published.scope.clone(),
        repo_id: published.repo_id.clone(),
        generation: generation(published.active_generation.as_deref().expect("generation")),
        attempt_id: format!("coordinator-attempt-{suffix}"),
        idempotency_key: format!("coordinator-request-{suffix}"),
    };
    assert_eq!(
        block_on(coordinator.rebuild(&actor, replay, &stages)).expect("replay"),
        published
    );
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[tokio::test(flavor = "current_thread")]
async fn postgres_current_thread_tokio_progresses_while_scope_authority_waits() {
    let (store, mut holder, observer, waiter_pid) = std::thread::spawn(|| {
        let (store, waiter_pid) = repository_with_pid();
        let store = Arc::new(store);
        let holder = PgConnection::connect(
            &std::env::var("ENGRAM_TEST_PG_URL")
                .expect("marked disposable PostgreSQL URL required"),
        )
        .expect("holder connection");
        let observer = PgConnection::connect(
            &std::env::var("ENGRAM_TEST_PG_URL")
                .expect("marked disposable PostgreSQL URL required"),
        )
        .expect("observer connection");
        (store, holder, observer, waiter_pid)
    })
    .join()
    .expect("construct connections outside Tokio");
    let suffix = unique();
    let test_scope = scope(&format!("tokio-wait-{suffix}"), "factory-test");
    store
        .bind(binding(&test_scope, &format!("tokio-wait-repo-{suffix}")))
        .await
        .expect("bind");
    let eligibility = store.eligibility(&test_scope).await.expect("eligibility");
    let scope_key = serde_json::to_string(&test_scope).expect("scope key");
    let holder_pid: i32 = holder
        .client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .expect("holder backend pid")
        .get(0);
    let tx = holder
        .client
        .transaction()
        .await
        .expect("holder transaction");
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("{}:{scope_key}", test_scope.tenant)],
    )
    .await
    .expect("hold scope authority");
    let waiter_store = store.clone();
    let lease = CodeReadLease {
        id: format!("tokio-wait-lease-{suffix}"),
        eligibility,
        fencing_token: 1,
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(1),
    };
    let waiter = tokio::spawn(async move {
        waiter_store
            .acquire_read_lease(lease, chrono::Utc::now())
            .await
    });
    let observed_wait = Arc::new(AtomicBool::new(false));
    for _ in 0..1_000 {
        let waiting: bool = observer
            .client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
                &[&holder_pid, &waiter_pid],
            )
            .await
            .expect("observe advisory wait")
            .get(0);
        if waiting {
            observed_wait.store(true, Ordering::SeqCst);
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        observed_wait.load(Ordering::SeqCst),
        "repository operation did not wait on the held advisory authority"
    );
    let progressed = Arc::new(AtomicBool::new(false));
    let progressed_task = progressed.clone();
    tokio::spawn(async move {
        progressed_task.store(true, Ordering::SeqCst);
    })
    .await
    .expect("independent Tokio task");
    assert!(
        progressed.load(Ordering::SeqCst),
        "current-thread Tokio task did not progress while repository operation waited"
    );
    tx.commit().await.expect("release scope authority");
    let acquired = waiter
        .await
        .expect("waiter task")
        .expect("acquire after barrier release");
    store
        .release_read_lease(&acquired)
        .await
        .expect("lease cleanup");
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[tokio::test(flavor = "current_thread")]
async fn postgres_current_thread_tokio_coordinator_and_typed_follow_up_do_not_panic() {
    let store = std::sync::Arc::new(
        std::thread::spawn(repository)
            .join()
            .expect("construct repository outside Tokio"),
    );
    let suffix = unique();
    let test_scope = scope(&format!("tokio-coordinator-{suffix}"), "factory-test");
    let repo = RepoId(format!("tokio-coordinator-repo-{suffix}"));
    store
        .bind(binding(&test_scope, &repo.0))
        .await
        .expect("bind");
    let coordinator =
        CodeBindingCoordinator::new(store.clone(), std::sync::Arc::new(AllowRebuild), vec![]);
    let actor = Actor {
        id: Id::from("factory-test"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    };
    let generation = generation(&format!("tokio-coordinator-generation-{suffix}"));
    let published = coordinator
        .rebuild(
            &actor,
            CodeRebuildRequest {
                scope: test_scope.clone(),
                repo_id: repo.clone(),
                generation: generation.clone(),
                attempt_id: format!("tokio-coordinator-attempt-{suffix}"),
                idempotency_key: format!("tokio-coordinator-request-{suffix}"),
            },
            &ReceiptStages,
        )
        .await
        .expect("coordinator from current-thread Tokio");
    assert_eq!(
        published.active_generation.as_deref(),
        Some(generation.id.as_str())
    );
    let now = chrono::Utc::now();
    assert!(matches!(
        store.acquire_read_lease(
            CodeReadLease {
                id: format!("tokio-invalid-{suffix}"),
                eligibility: store.eligibility(&test_scope).await.expect("eligibility"),
                fencing_token: 0,
                expires_at: now + chrono::Duration::minutes(1),
            },
            now,
        ).await,
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert_eq!(
        store
            .eligibility(&test_scope)
            .await
            .expect("follow-up after typed error"),
        store
            .eligibility(&test_scope)
            .await
            .expect("repeat eligibility"),
    );
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[tokio::test]
async fn postgres_tokio_binding_and_lease_operations_do_not_panic() {
    let store = std::thread::spawn(repository)
        .join()
        .expect("construct repository outside Tokio");
    let suffix = unique();
    let test_scope = scope(&format!("tokio-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("tokio-repo-{suffix}"));
    store.bind(bound).await.expect("bind from Tokio");
    let eligibility = store
        .eligibility(&test_scope)
        .await
        .expect("eligibility from Tokio");
    let now = chrono::Utc::now();
    let lease = store
        .acquire_read_lease(
            CodeReadLease {
                id: format!("tokio-lease-{suffix}"),
                eligibility,
                fencing_token: 1,
                expires_at: now + chrono::Duration::minutes(1),
            },
            now,
        )
        .await
        .expect("acquire from Tokio");
    store
        .release_read_lease(&lease)
        .await
        .expect("follow-up release from Tokio");
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_global_lease_id_race_has_one_durable_owner() {
    let store = repository();
    let suffix = unique();
    let first_scope = scope(&format!("global-id-first-{suffix}"), "factory-test");
    let second_scope = scope(&format!("global-id-second-{suffix}"), "factory-test");
    let first_repo = format!("global-id-first-repo-{suffix}");
    let second_repo = format!("global-id-second-repo-{suffix}");
    block_on(store.bind(binding(&first_scope, &first_repo))).expect("bind first scope");
    block_on(store.bind(binding(&second_scope, &second_repo))).expect("bind second scope");
    let first_generation = generation(&format!("global-id-first-generation-one-{suffix}"));
    let second_generation = generation(&format!("global-id-second-generation-one-{suffix}"));
    publish_generation(
        &store,
        &first_scope,
        &first_repo,
        first_generation.clone(),
        0,
        None,
        &format!("global-id-first-attempt-one-{suffix}"),
    );
    publish_generation(
        &store,
        &second_scope,
        &second_repo,
        second_generation.clone(),
        0,
        None,
        &format!("global-id-second-attempt-one-{suffix}"),
    );
    let lease_id = format!("global-lease-id-{suffix}");
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(5);
    let first_lease = CodeReadLease {
        id: lease_id.clone(),
        eligibility: block_on(store.eligibility(&first_scope)).expect("first eligibility"),
        fencing_token: 1,
        expires_at,
    };
    let second_lease = CodeReadLease {
        id: lease_id.clone(),
        eligibility: block_on(store.eligibility(&second_scope)).expect("second eligibility"),
        fencing_token: 1,
        expires_at,
    };
    let url =
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required");
    let mut barrier = FixtureLeaseInsertBarrier::install(&url, &lease_id, &suffix);
    let observer = PgConnection::connect(&url).expect("fixture barrier observer");
    let holder_pid: i32 = block_on(
        barrier
            .connection
            .client
            .query_one("SELECT pg_backend_pid()", &[]),
    )
    .expect("fixture barrier backend pid")
    .get(0);
    let barrier_tx =
        block_on(barrier.connection.client.transaction()).expect("fixture barrier transaction");
    block_on(barrier_tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&barrier.key],
    ))
    .expect("hold fixture insert barrier");
    let mut first = spawn_race_child(
        "acquire-global-id",
        &first_scope,
        &first_repo,
        &lease_id,
        &expires_at.to_rfc3339(),
    );
    wait_ready(&mut first);
    let first_pid = first.backend_pid.expect("first global-id child pid");
    first
        .child
        .stdin
        .take()
        .expect("first child stdin")
        .write_all(b"G")
        .expect("release first global-id child");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut first_waiting_on_fixture = false;
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&holder_pid, &first_pid],
        ))
        .expect("observe first fixture-barrier wait")
        .get(0);
        if waiting {
            first_waiting_on_fixture = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        first_waiting_on_fixture,
        "first child did not reach fixture insert barrier"
    );
    let mut second = spawn_race_child(
        "acquire-global-id",
        &second_scope,
        &second_repo,
        &lease_id,
        &expires_at.to_rfc3339(),
    );
    wait_ready(&mut second);
    let second_pid = second.backend_pid.expect("second global-id child pid");
    second
        .child
        .stdin
        .take()
        .expect("second child stdin")
        .write_all(b"G")
        .expect("release second global-id child");
    let mut second_waiting_on_first_global_id = false;
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&first_pid, &second_pid],
        ))
        .expect("observe second global-ID wait")
        .get(0);
        if waiting {
            second_waiting_on_first_global_id = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        second_waiting_on_first_global_id,
        "second child did not wait on first child's production lease-ID authority"
    );
    block_on(barrier_tx.commit()).expect("release fixture insert barrier");
    let first_output = collect_child(first);
    let second_output = collect_child(second);
    let first_won = first_output.lines().any(|line| line == "LEASE_OWNER");
    let second_won = second_output.lines().any(|line| line == "LEASE_OWNER");
    assert_ne!(first_won, second_won, "exactly one child owns the lease ID");
    assert!(
        first_output.lines().any(|line| line == "LEASE_CONFLICT")
            || second_output.lines().any(|line| line == "LEASE_CONFLICT"),
        "the losing child returned typed conflict"
    );
    let winner = if first_won {
        &first_lease
    } else {
        &second_lease
    };
    let loser = if first_won {
        &second_lease
    } else {
        &first_lease
    };
    publish_generation(
        &store,
        &first_scope,
        &first_repo,
        generation(&format!("global-id-first-generation-two-{suffix}")),
        1,
        Some(first_generation.id.clone()),
        &format!("global-id-first-attempt-two-{suffix}"),
    );
    publish_generation(
        &store,
        &first_scope,
        &first_repo,
        generation(&format!("global-id-first-generation-three-{suffix}")),
        2,
        Some(format!("global-id-first-generation-two-{suffix}")),
        &format!("global-id-first-attempt-three-{suffix}"),
    );
    publish_generation(
        &store,
        &second_scope,
        &second_repo,
        generation(&format!("global-id-second-generation-two-{suffix}")),
        1,
        Some(second_generation.id.clone()),
        &format!("global-id-second-attempt-two-{suffix}"),
    );
    publish_generation(
        &store,
        &second_scope,
        &second_repo,
        generation(&format!("global-id-second-generation-three-{suffix}")),
        2,
        Some(format!("global-id-second-generation-two-{suffix}")),
        &format!("global-id-second-attempt-three-{suffix}"),
    );
    let first_retained = block_on(store.retained_generations(&first_scope, now, 0))
        .expect("first retained generations");
    let second_retained = block_on(store.retained_generations(&second_scope, now, 0))
        .expect("second retained generations");
    assert_eq!(
        first_retained.contains(&first_generation.id),
        first_won,
        "only the durable winner retains its lease-only generation"
    );
    assert_eq!(
        second_retained.contains(&second_generation.id),
        second_won,
        "only the durable winner retains its lease-only generation"
    );
    block_on(store.release_read_lease(winner)).expect("release durable winner");
    assert!(matches!(
        block_on(store.release_read_lease(loser)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    barrier.cleanup();
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_scope_and_lease_id_authorities_use_disjoint_domains() {
    let store = repository();
    let suffix = unique();
    let tenant = "engram-code-read-lease-id";
    let first_scope = scope(&format!("namespace-a-{suffix}"), tenant);
    let second_scope = scope(&format!("namespace-b-{suffix}"), tenant);
    let first_repo = format!("namespace-a-repo-{suffix}");
    let second_repo = format!("namespace-b-repo-{suffix}");
    block_on(store.bind(binding(&first_scope, &first_repo))).expect("bind first scope");
    block_on(store.bind(binding(&second_scope, &second_repo))).expect("bind second scope");
    let first_eligibility = block_on(store.eligibility(&first_scope)).expect("first eligibility");
    let second_eligibility =
        block_on(store.eligibility(&second_scope)).expect("second eligibility");
    let first_scope_key = serde_json::to_string(&first_scope).expect("first scope key");
    let second_scope_key = serde_json::to_string(&second_scope).expect("second scope key");
    let url =
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required");
    let mut holder = PgConnection::connect(&url).expect("namespace row holder");
    let observer = PgConnection::connect(&url).expect("namespace observer");
    let holder_pid: i32 = block_on(holder.client.query_one("SELECT pg_backend_pid()", &[]))
        .expect("namespace holder backend pid")
        .get(0);
    let holder_tx = block_on(holder.client.transaction()).expect("namespace holder transaction");
    for key in [&first_scope_key, &second_scope_key] {
        block_on(holder_tx.query_one(
            "SELECT epoch FROM code_epochs WHERE tenant=$1 AND scope_key=$2 FOR UPDATE",
            &[&tenant, key],
        ))
        .expect("lock fresh owned epoch row");
    }
    let expiry = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    let mut first = spawn_namespace_child(
        &first_scope,
        &first_repo,
        &second_scope_key,
        first_eligibility.epoch,
        &expiry,
    );
    let mut second = spawn_namespace_child(
        &second_scope,
        &second_repo,
        &first_scope_key,
        second_eligibility.epoch,
        &expiry,
    );
    wait_ready(&mut first);
    wait_ready(&mut second);
    let first_pid = first.backend_pid.expect("first namespace child pid");
    let second_pid = second.backend_pid.expect("second namespace child pid");
    for child in [&mut first, &mut second] {
        child
            .child
            .stdin
            .take()
            .expect("namespace child stdin")
            .write_all(b"G")
            .expect("start namespace child");
    }
    wait_for_scope_authority(&observer, first_pid, &first_scope, "first scope authority");
    wait_for_scope_authority(
        &observer,
        second_pid,
        &second_scope,
        "second scope authority",
    );
    wait_for_blocker(&observer, holder_pid, first_pid, "first owned epoch row");
    wait_for_blocker(&observer, holder_pid, second_pid, "second owned epoch row");
    block_on(holder_tx.commit()).expect("release owned epoch rows");
    assert!(collect_child(first)
        .lines()
        .any(|line| line == "LEASE_OWNER"));
    assert!(collect_child(second)
        .lines()
        .any(|line| line == "LEASE_OWNER"));
    for lease_id in [&second_scope_key, &first_scope_key] {
        let state = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM code_read_leases WHERE lease_id=$1), NOT EXISTS (SELECT 1 FROM code_read_lease_tombstones WHERE lease_id=$1)",
            &[lease_id],
        ))
        .expect("inspect namespace lease");
        assert!(state.get::<_, bool>(0));
        assert!(state.get::<_, bool>(1));
    }
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_release_first_blocks_acquire_and_retires_global_lease_id() {
    for same_scope_replay in [true, false] {
        let store = repository();
        let suffix = unique();
        let first_scope = scope(&format!("release-first-a-{suffix}"), "factory-test");
        let second_scope = if same_scope_replay {
            first_scope.clone()
        } else {
            scope(&format!("release-first-b-{suffix}"), "factory-test")
        };
        let first_repo = format!("release-first-a-repo-{suffix}");
        let second_repo = if same_scope_replay {
            first_repo.clone()
        } else {
            format!("release-first-b-repo-{suffix}")
        };
        block_on(store.bind(binding(&first_scope, &first_repo))).expect("bind first scope");
        if !same_scope_replay {
            block_on(store.bind(binding(&second_scope, &second_repo))).expect("bind second scope");
        }
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(5);
        let lease_id = format!("release-first-id-{suffix}");
        let original = block_on(store.acquire_read_lease(
            CodeReadLease {
                id: lease_id.clone(),
                eligibility: block_on(store.eligibility(&first_scope)).expect("first eligibility"),
                fencing_token: 1,
                expires_at,
            },
            now,
        ))
        .expect("acquire original lease");
        let url =
            std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required");
        let mut barrier = FixtureLeaseReleaseBarrier::install(&url, &lease_id, &suffix);
        let observer = PgConnection::connect(&url).expect("release barrier observer");
        let holder_pid: i32 = block_on(
            barrier
                .connection
                .client
                .query_one("SELECT pg_backend_pid()", &[]),
        )
        .expect("fixture release barrier backend pid")
        .get(0);
        let barrier_tx =
            block_on(barrier.connection.client.transaction()).expect("fixture release transaction");
        block_on(barrier_tx.execute(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&barrier.key],
        ))
        .expect("hold fixture release barrier");
        let mut releasing = spawn_race_child(
            "release-global-id",
            &first_scope,
            &first_repo,
            &lease_id,
            &expires_at.to_rfc3339(),
        );
        wait_ready(&mut releasing);
        let release_pid = releasing.backend_pid.expect("release child backend pid");
        releasing
            .child
            .stdin
            .take()
            .expect("release child stdin")
            .write_all(b"G")
            .expect("start release child");
        wait_for_advisory_wait(
            &observer,
            holder_pid,
            release_pid,
            "release fixture barrier",
        );
        wait_for_lease_authority(
            &observer,
            release_pid,
            &lease_id,
            "release global-ID authority",
        );
        let mut acquiring = spawn_race_child(
            "acquire-global-id",
            &second_scope,
            &second_repo,
            &lease_id,
            &expires_at.to_rfc3339(),
        );
        wait_ready(&mut acquiring);
        let acquire_pid = acquiring.backend_pid.expect("acquire child backend pid");
        acquiring
            .child
            .stdin
            .take()
            .expect("acquire child stdin")
            .write_all(b"G")
            .expect("start acquire child");
        wait_for_advisory_wait(
            &observer,
            release_pid,
            acquire_pid,
            "production global-ID authority",
        );
        block_on(barrier_tx.commit()).expect("release fixture barrier");
        assert!(collect_child(releasing)
            .lines()
            .any(|line| line == "LEASE_RELEASED"));
        assert!(collect_child(acquiring)
            .lines()
            .any(|line| line == "LEASE_CONFLICT"));
        let state = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM code_read_lease_tombstones WHERE lease_id=$1), EXISTS (SELECT 1 FROM code_read_leases WHERE lease_id=$1)",
            &[&lease_id],
        ))
        .expect("inspect retired lease state");
        assert!(
            state.get::<_, bool>(0),
            "released lease tombstone is durable"
        );
        assert!(!state.get::<_, bool>(1), "released lease has no live row");
        assert!(matches!(
            block_on(store.release_read_lease(&original)),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
        assert!(matches!(
            block_on(store.renew_read_lease(
                &original,
                now,
                expires_at + chrono::Duration::minutes(1),
            )),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
        barrier.cleanup();
    }
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_acquire_first_blocks_release_and_retires_global_lease_id() {
    for same_scope_replay in [true, false] {
        let store = repository();
        let suffix = unique();
        let first_scope = scope(&format!("acquire-first-a-{suffix}"), "factory-test");
        let second_scope = if same_scope_replay {
            first_scope.clone()
        } else {
            scope(&format!("acquire-first-b-{suffix}"), "factory-test")
        };
        let first_repo = format!("acquire-first-a-repo-{suffix}");
        let second_repo = if same_scope_replay {
            first_repo.clone()
        } else {
            format!("acquire-first-b-repo-{suffix}")
        };
        block_on(store.bind(binding(&first_scope, &first_repo))).expect("bind first scope");
        if !same_scope_replay {
            block_on(store.bind(binding(&second_scope, &second_repo))).expect("bind second scope");
        }
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(5);
        let lease_id = format!("acquire-first-id-{suffix}");
        let original = block_on(store.acquire_read_lease(
            CodeReadLease {
                id: lease_id.clone(),
                eligibility: block_on(store.eligibility(&first_scope)).expect("first eligibility"),
                fencing_token: 1,
                expires_at,
            },
            now,
        ))
        .expect("acquire original lease");
        let url =
            std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required");
        let mut row_holder = PgConnection::connect(&url).expect("owned row pause connection");
        let observer = PgConnection::connect(&url).expect("acquire-first observer");
        let holder_pid: i32 = block_on(row_holder.client.query_one("SELECT pg_backend_pid()", &[]))
            .expect("owned row pause backend pid")
            .get(0);
        let row_tx =
            block_on(row_holder.client.transaction()).expect("owned row pause transaction");
        block_on(row_tx.query_one(
            "SELECT lease_id FROM code_read_leases WHERE lease_id=$1 FOR UPDATE",
            &[&lease_id],
        ))
        .expect("lock fresh owned live lease row");
        let mut acquiring = spawn_race_child(
            "acquire-global-id",
            &second_scope,
            &second_repo,
            &lease_id,
            &expires_at.to_rfc3339(),
        );
        wait_ready(&mut acquiring);
        let acquire_pid = acquiring.backend_pid.expect("acquire child backend pid");
        acquiring
            .child
            .stdin
            .take()
            .expect("acquire child stdin")
            .write_all(b"G")
            .expect("start acquire child");
        wait_for_lease_authority(
            &observer,
            acquire_pid,
            &lease_id,
            "acquire global-ID authority",
        );
        wait_for_blocker(&observer, holder_pid, acquire_pid, "owned live lease row");
        let mut releasing = spawn_race_child(
            "release-global-id",
            &first_scope,
            &first_repo,
            &lease_id,
            &expires_at.to_rfc3339(),
        );
        wait_ready(&mut releasing);
        let release_pid = releasing.backend_pid.expect("release child backend pid");
        releasing
            .child
            .stdin
            .take()
            .expect("release child stdin")
            .write_all(b"G")
            .expect("start release child");
        wait_for_advisory_wait(
            &observer,
            acquire_pid,
            release_pid,
            "acquire child production global-ID authority",
        );
        block_on(row_tx.commit()).expect("release owned row pause");
        let acquire_output = collect_child(acquiring);
        if same_scope_replay {
            assert!(
                acquire_output.lines().any(|line| line == "LEASE_OWNER"),
                "identical replay did not return the canonical lease"
            );
        } else {
            assert!(
                acquire_output.lines().any(|line| line == "LEASE_CONFLICT"),
                "different record did not return typed GenerationChanged"
            );
        }
        assert!(
            collect_child(releasing)
                .lines()
                .any(|line| line == "LEASE_RELEASED"),
            "release did not complete after acquire ended"
        );
        let state = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM code_read_lease_tombstones WHERE lease_id=$1), EXISTS (SELECT 1 FROM code_read_leases WHERE lease_id=$1)",
            &[&lease_id],
        ))
        .expect("inspect retired lease state");
        assert!(
            state.get::<_, bool>(0),
            "released lease tombstone is durable"
        );
        assert!(!state.get::<_, bool>(1), "released lease has no live row");
        assert!(matches!(
            block_on(store.release_read_lease(&original)),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
        assert!(matches!(
            block_on(store.renew_read_lease(
                &original,
                now,
                expires_at + chrono::Duration::minutes(1),
            )),
            Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
        ));
    }
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_released_lease_id_is_durably_rejected_across_reopen() {
    let store = repository();
    let suffix = unique();
    let first_scope = scope(&format!("released-id-first-{suffix}"), "factory-test");
    let second_scope = scope(&format!("released-id-second-{suffix}"), "factory-test");
    block_on(store.bind(binding(
        &first_scope,
        &format!("released-id-first-repo-{suffix}"),
    )))
    .expect("bind first scope");
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(5);
    let first_lease = block_on(store.acquire_read_lease(
        CodeReadLease {
            id: format!("released-id-{suffix}"),
            eligibility: block_on(store.eligibility(&first_scope)).expect("first eligibility"),
            fencing_token: 1,
            expires_at,
        },
        now,
    ))
    .expect("acquire first lease");
    block_on(store.release_read_lease(&first_lease)).expect("release first lease");

    let reopened = repository();
    block_on(reopened.bind(binding(
        &second_scope,
        &format!("released-id-second-repo-{suffix}"),
    )))
    .expect("bind second scope");
    let replacement = CodeReadLease {
        id: first_lease.id.clone(),
        eligibility: block_on(reopened.eligibility(&second_scope)).expect("second eligibility"),
        fencing_token: 1,
        expires_at: expires_at + chrono::Duration::minutes(1),
    };
    assert!(matches!(
        block_on(reopened.acquire_read_lease(replacement, now)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert!(matches!(
        block_on(reopened.release_read_lease(&first_lease)),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    assert!(matches!(
        block_on(reopened.renew_read_lease(
            &first_lease,
            now,
            expires_at + chrono::Duration::minutes(2),
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
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
fn postgres_first_bind_waited_stale_lease_is_rejected() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("first-bind-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("first-bind-repo-{suffix}"));
    let old_empty = block_on(store.eligibility(&test_scope)).expect("empty-scope eligibility");
    assert_eq!(old_empty.epoch, 0);
    assert!(old_empty.repository_ids.is_empty());
    assert!(old_empty.generations.is_empty());

    let mut stale_reader = spawn_race_child("read-stale", &test_scope, &bound.repo_id.0, "", "");
    wait_ready(&mut stale_reader);
    let reader_pid = stale_reader.backend_pid.expect("reader backend pid");

    let mut writer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("writer connection");
    let observer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("observer connection");
    let writer_pid: i32 = block_on(writer.client.query_one("SELECT pg_backend_pid()", &[]))
        .expect("writer backend pid")
        .get(0);
    let scope_key = serde_json::to_string(&test_scope).expect("scope key");
    let binding_json = serde_json::to_value(&bound).expect("binding fixture JSON");
    let tx = block_on(writer.client.transaction()).expect("writer transaction");
    block_on(tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("{}:{scope_key}", test_scope.tenant)],
    ))
    .expect("hold production scope authority");
    block_on(tx.execute(
        "INSERT INTO code_repo_bindings(tenant,scope_key,repo_id,branch_ref,version,active_generation,epoch,record_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        &[&test_scope.tenant, &scope_key, &bound.repo_id.0, &bound.branch_ref, &1_i64, &Option::<String>::None, &0_i64, &binding_json],
    ))
    .expect("stage first binding");
    block_on(tx.execute(
        "INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",
        &[&test_scope.tenant, &scope_key],
    ))
    .expect("stage scope epoch");

    stale_reader
        .child
        .stdin
        .as_mut()
        .expect("child stdin")
        .write_all(b"G")
        .expect("release stale reader");
    stale_reader.child.stdin.take().expect("close child stdin");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut waited_on_exact_authority = false;
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&writer_pid, &reader_pid],
        ))
        .expect("observe exact advisory authority")
        .get(0);
        if waiting {
            waited_on_exact_authority = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        waited_on_exact_authority,
        "stale reader did not wait on the writer's exact scope authority tuple"
    );
    block_on(tx.commit()).expect("commit staged first binding");
    let mut child_output = String::new();
    stale_reader
        .output
        .read_to_string(&mut child_output)
        .expect("read stale reader output");
    assert!(
        stale_reader
            .child
            .wait()
            .expect("reap stale reader")
            .success(),
        "stale reader failed"
    );
    assert!(
        child_output.lines().any(|line| line == "STALE"),
        "stale reader did not reject old empty-scope eligibility"
    );

    assert_eq!(
        block_on(store.get_binding(&test_scope, &bound.repo_id)).expect("committed binding"),
        Some(bound)
    );
    let committed = block_on(store.eligibility(&test_scope)).expect("committed eligibility");
    assert_eq!(committed.epoch, 1);
    assert!(committed.repository_ids.is_empty());
    assert!(committed.generations.is_empty());
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_renewal_waited_behind_publication_rejects_old_eligibility() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("renew-publication-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("renew-publication-repo-{suffix}"));
    block_on(store.bind(bound.clone())).expect("bind renewal scope");
    let old_generation = generation(&format!("renew-old-generation-{suffix}"));
    let old_attempt = attempt(&format!("renew-old-attempt-{suffix}"), 0, None, 1);
    block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        old_generation.clone(),
        old_attempt.clone(),
    ))
    .expect("begin old publication");
    receipt(&store, &old_attempt, &old_generation);
    block_on(store.publish(&test_scope, &bound.repo_id, &old_attempt.id, 0))
        .expect("publish old generation");
    let old_eligibility = block_on(store.eligibility(&test_scope)).expect("old eligibility");
    let now = chrono::Utc::now();
    let old_lease = block_on(store.acquire_read_lease(
        CodeReadLease {
            id: format!("renew-publication-lease-{suffix}"),
            eligibility: old_eligibility.clone(),
            fencing_token: 1,
            expires_at: now + chrono::Duration::minutes(10),
        },
        now,
    ))
    .expect("acquire valid old lease");
    let next_generation = generation(&format!("renew-next-generation-{suffix}"));
    let next_attempt = attempt(
        &format!("renew-next-attempt-{suffix}"),
        1,
        Some(old_generation.id.clone()),
        2,
    );
    block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        next_generation.clone(),
        next_attempt.clone(),
    ))
    .expect("prepare next publication");
    receipt(&store, &next_attempt, &next_generation);
    let current_binding = block_on(store.get_binding(&test_scope, &bound.repo_id))
        .expect("current binding")
        .expect("bound repository");
    assert_eq!(current_binding.epoch, 1);

    let mut renewal_child = spawn_renewal_child(&test_scope, &bound.repo_id.0, &old_lease);
    wait_ready(&mut renewal_child);
    let child_pid = renewal_child
        .backend_pid
        .expect("renewal child backend pid");
    let mut writer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("publication writer connection");
    let observer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("publication observer connection");
    let writer_pid: i32 = block_on(writer.client.query_one("SELECT pg_backend_pid()", &[]))
        .expect("publication writer backend pid")
        .get(0);
    let scope_key = serde_json::to_string(&test_scope).expect("scope key");
    let published_binding = CodeBinding {
        active_generation: Some(next_generation.id.clone()),
        epoch: current_binding.epoch + 1,
        ..current_binding.clone()
    };
    let published_json = serde_json::to_value(&published_binding).expect("published binding JSON");
    let tx = block_on(writer.client.transaction()).expect("publication writer transaction");
    block_on(tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("{}:{scope_key}", test_scope.tenant)],
    ))
    .expect("hold production scope authority");
    assert_eq!(
        block_on(tx.execute(
            "UPDATE code_repo_bindings SET active_generation=$1,epoch=epoch+1,record_json=$2 WHERE tenant=$3 AND scope_key=$4 AND repo_id=$5 AND epoch=$6 AND active_generation=$7",
            &[&next_generation.id, &published_json, &test_scope.tenant, &scope_key, &bound.repo_id.0, &i64::try_from(current_binding.epoch).expect("binding epoch"), &old_generation.id],
        ))
        .expect("stage published binding"),
        1
    );
    block_on(tx.execute(
        "INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",
        &[&test_scope.tenant, &scope_key],
    ))
    .expect("stage published scope epoch");
    assert_eq!(
        block_on(tx.execute(
            "UPDATE code_attempts SET state='published' WHERE attempt_id=$1 AND fence=$2",
            &[&next_attempt.id, &2_i64],
        ))
        .expect("stage published attempt"),
        1
    );

    renewal_child
        .child
        .stdin
        .as_mut()
        .expect("renewal child stdin")
        .write_all(b"G")
        .expect("release renewal child");
    renewal_child
        .child
        .stdin
        .take()
        .expect("close renewal child stdin");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut waited_on_exact_authority = false;
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&writer_pid, &child_pid],
        ))
        .expect("observe exact publication authority")
        .get(0);
        if waiting {
            waited_on_exact_authority = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        waited_on_exact_authority,
        "renewal child did not wait on the writer's exact scope authority tuple"
    );
    block_on(tx.commit()).expect("commit staged publication");
    let mut child_output = String::new();
    renewal_child
        .output
        .read_to_string(&mut child_output)
        .expect("read renewal child output");
    assert!(
        renewal_child
            .child
            .wait()
            .expect("reap renewal child")
            .success(),
        "renewal child failed"
    );
    assert!(
        child_output.lines().any(|line| line == "RENEW_STALE"),
        "renewal child did not reject old lease eligibility"
    );

    assert_eq!(
        block_on(store.get_binding(&test_scope, &bound.repo_id)).expect("published binding"),
        Some(published_binding)
    );
    let committed = block_on(store.eligibility(&test_scope)).expect("published eligibility");
    assert_eq!(committed.epoch, old_eligibility.epoch + 1);
    assert_eq!(committed.repository_ids, vec![bound.repo_id.0.clone()]);
    assert_eq!(committed.generations, vec![next_generation.id.clone()]);
    assert!(matches!(
        block_on(store.renew_read_lease(
            &old_lease,
            chrono::Utc::now(),
            old_lease.expires_at + chrono::Duration::minutes(1),
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
    let retained = block_on(store.retained_generations(&test_scope, chrono::Utc::now(), 0))
        .expect("retained generations after stale renewal");
    assert!(retained.contains(&old_generation.id));
    assert!(retained.contains(&next_generation.id));
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_retention_waited_behind_publication_completes_consistently() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("retention-publication-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("retention-publication-repo-{suffix}"));
    block_on(store.bind(bound.clone())).expect("bind retention scope");
    let old_generation = generation(&format!("retention-old-generation-{suffix}"));
    let old_attempt = attempt(&format!("retention-old-attempt-{suffix}"), 0, None, 1);
    block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        old_generation.clone(),
        old_attempt.clone(),
    ))
    .expect("begin old publication");
    receipt(&store, &old_attempt, &old_generation);
    block_on(store.publish(&test_scope, &bound.repo_id, &old_attempt.id, 0))
        .expect("publish old generation");
    let before_publication = block_on(store.eligibility(&test_scope)).expect("old eligibility");
    let next_generation = generation(&format!("retention-next-generation-{suffix}"));
    let next_attempt = attempt(
        &format!("retention-next-attempt-{suffix}"),
        1,
        Some(old_generation.id.clone()),
        2,
    );
    block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        next_generation.clone(),
        next_attempt.clone(),
    ))
    .expect("prepare next publication");
    receipt(&store, &next_attempt, &next_generation);
    let current_binding = block_on(store.get_binding(&test_scope, &bound.repo_id))
        .expect("current binding")
        .expect("bound repository");
    assert_eq!(current_binding.epoch, 1);

    let mut retention_child = spawn_retention_child(
        &test_scope,
        &bound.repo_id.0,
        &old_generation.id,
        &next_generation.id,
    );
    wait_ready(&mut retention_child);
    let child_pid = retention_child
        .backend_pid
        .expect("retention child backend pid");
    let mut writer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("publication writer connection");
    let observer = PgConnection::connect(
        &std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required"),
    )
    .expect("publication observer connection");
    let writer_pid: i32 = block_on(writer.client.query_one("SELECT pg_backend_pid()", &[]))
        .expect("publication writer backend pid")
        .get(0);
    let scope_key = serde_json::to_string(&test_scope).expect("scope key");
    let published_binding = CodeBinding {
        active_generation: Some(next_generation.id.clone()),
        epoch: current_binding.epoch + 1,
        ..current_binding.clone()
    };
    let published_json = serde_json::to_value(&published_binding).expect("published binding JSON");
    let tx = block_on(writer.client.transaction()).expect("publication writer transaction");
    block_on(tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!("{}:{scope_key}", test_scope.tenant)],
    ))
    .expect("hold production scope authority");
    assert_eq!(
        block_on(tx.execute(
            "UPDATE code_repo_bindings SET active_generation=$1,epoch=epoch+1,record_json=$2 WHERE tenant=$3 AND scope_key=$4 AND repo_id=$5 AND epoch=$6 AND active_generation=$7",
            &[&next_generation.id, &published_json, &test_scope.tenant, &scope_key, &bound.repo_id.0, &i64::try_from(current_binding.epoch).expect("binding epoch"), &old_generation.id],
        ))
        .expect("stage published binding"),
        1
    );
    block_on(tx.execute(
        "INSERT INTO code_epochs(tenant,scope_key,epoch) VALUES($1,$2,1) ON CONFLICT(tenant,scope_key) DO UPDATE SET epoch=code_epochs.epoch+1",
        &[&test_scope.tenant, &scope_key],
    ))
    .expect("stage published scope epoch");
    assert_eq!(
        block_on(tx.execute(
            "UPDATE code_attempts SET state='published' WHERE attempt_id=$1 AND fence=$2",
            &[&next_attempt.id, &2_i64],
        ))
        .expect("stage published attempt"),
        1
    );

    retention_child
        .child
        .stdin
        .as_mut()
        .expect("retention child stdin")
        .write_all(b"G")
        .expect("release retention child");
    retention_child
        .child
        .stdin
        .take()
        .expect("close retention child stdin");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut waited_on_exact_authority = false;
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&writer_pid, &child_pid],
        ))
        .expect("observe exact retention authority")
        .get(0);
        if waiting {
            waited_on_exact_authority = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        waited_on_exact_authority,
        "retention child did not wait on the writer's exact scope authority tuple"
    );
    block_on(tx.commit()).expect("commit staged publication");
    let mut child_output = String::new();
    retention_child
        .output
        .read_to_string(&mut child_output)
        .expect("read retention child output");
    assert!(
        retention_child
            .child
            .wait()
            .expect("reap retention child")
            .success(),
        "retention child failed"
    );
    assert!(
        child_output.lines().any(|line| line == "RETAINED"),
        "retention child did not return consistent retained generations"
    );

    assert_eq!(
        block_on(store.get_binding(&test_scope, &bound.repo_id)).expect("published binding"),
        Some(published_binding)
    );
    let committed = block_on(store.eligibility(&test_scope)).expect("published eligibility");
    assert_eq!(committed.epoch, before_publication.epoch + 1);
    assert_eq!(committed.repository_ids, vec![bound.repo_id.0.clone()]);
    assert_eq!(committed.generations, vec![next_generation.id.clone()]);
    let retained = block_on(store.retained_generations(&test_scope, chrono::Utc::now(), 0))
        .expect("parent retained generations");
    assert!(retained.contains(&old_generation.id));
    assert!(retained.contains(&next_generation.id));
}

#[ignore = "requires parent-authorized marked disposable PostgreSQL"]
#[test]
fn postgres_three_generation_lease_only_retention_obeys_grace() {
    let store = repository();
    let suffix = unique();
    let test_scope = scope(&format!("lease-only-retention-{suffix}"), "factory-test");
    let bound = binding(&test_scope, &format!("lease-only-retention-repo-{suffix}"));
    block_on(store.bind(bound.clone())).expect("bind retention scope");

    let generation_one = generation(&format!("lease-only-generation-one-{suffix}"));
    let attempt_one = block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        generation_one.clone(),
        attempt(&format!("lease-only-attempt-one-{suffix}"), 0, None, 0),
    ))
    .expect("allocate first attempt");
    assert_eq!(attempt_one.fencing_token, 1);
    receipt(&store, &attempt_one, &generation_one);
    block_on(store.mark_ready(
        &test_scope,
        &bound.repo_id,
        &attempt_one.id,
        attempt_one.fencing_token,
    ))
    .expect("ready first attempt");
    block_on(store.publish(&test_scope, &bound.repo_id, &attempt_one.id, 0))
        .expect("publish first generation");
    let eligibility_one = block_on(store.eligibility(&test_scope)).expect("first eligibility");
    assert_eq!(eligibility_one.generations, vec![generation_one.id.clone()]);
    let lease_now = chrono::Utc::now();
    let lease_expires_at = lease_now + chrono::Duration::minutes(10);
    let lease = block_on(store.acquire_read_lease(
        CodeReadLease {
            id: format!("lease-only-retention-lease-{suffix}"),
            eligibility: eligibility_one.clone(),
            fencing_token: 1,
            expires_at: lease_expires_at,
        },
        lease_now,
    ))
    .expect("acquire generation-one lease");

    let generation_two = generation(&format!("lease-only-generation-two-{suffix}"));
    let attempt_two = block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        generation_two.clone(),
        attempt(
            &format!("lease-only-attempt-two-{suffix}"),
            1,
            Some(generation_one.id.clone()),
            0,
        ),
    ))
    .expect("allocate second attempt");
    assert_eq!(attempt_two.fencing_token, 2);
    receipt(&store, &attempt_two, &generation_two);
    block_on(store.mark_ready(
        &test_scope,
        &bound.repo_id,
        &attempt_two.id,
        attempt_two.fencing_token,
    ))
    .expect("ready second attempt");
    block_on(store.publish(&test_scope, &bound.repo_id, &attempt_two.id, 1))
        .expect("publish second generation");

    let generation_three = generation(&format!("lease-only-generation-three-{suffix}"));
    let attempt_three = block_on(store.begin_attempt(
        &test_scope,
        &bound.repo_id,
        generation_three.clone(),
        attempt(
            &format!("lease-only-attempt-three-{suffix}"),
            2,
            Some(generation_two.id.clone()),
            0,
        ),
    ))
    .expect("allocate third attempt");
    assert_eq!(attempt_three.fencing_token, 3);
    receipt(&store, &attempt_three, &generation_three);
    block_on(store.mark_ready(
        &test_scope,
        &bound.repo_id,
        &attempt_three.id,
        attempt_three.fencing_token,
    ))
    .expect("ready third attempt");
    block_on(store.publish(&test_scope, &bound.repo_id, &attempt_three.id, 2))
        .expect("publish third generation");

    let active = block_on(store.eligibility(&test_scope)).expect("third eligibility");
    assert_eq!(active.generations, vec![generation_three.id.clone()]);
    assert_eq!(
        block_on(store.get_attempt(&test_scope, &bound.repo_id, &attempt_one.id))
            .expect("first attempt")
            .expect("first attempt row")
            .1
            .state,
        CodeAttemptState::Published
    );
    assert_eq!(
        block_on(store.get_attempt(&test_scope, &bound.repo_id, &attempt_two.id))
            .expect("second attempt")
            .expect("second attempt row")
            .1
            .state,
        CodeAttemptState::Published
    );

    let retained_while_valid = block_on(store.retained_generations(
        &test_scope,
        lease_expires_at - chrono::Duration::seconds(1),
        0,
    ))
    .expect("retained while lease valid");
    assert!(retained_while_valid.contains(&generation_one.id));
    assert!(retained_while_valid.contains(&generation_two.id));
    assert!(retained_while_valid.contains(&generation_three.id));

    let grace_millis = 60_000;
    let retained_within_grace = block_on(store.retained_generations(
        &test_scope,
        lease_expires_at + chrono::Duration::seconds(30),
        grace_millis,
    ))
    .expect("retained within lease grace");
    assert!(retained_within_grace.contains(&generation_one.id));
    assert!(retained_within_grace.contains(&generation_two.id));
    assert!(retained_within_grace.contains(&generation_three.id));

    let retained_after_grace = block_on(store.retained_generations(
        &test_scope,
        lease_expires_at + chrono::Duration::seconds(61),
        grace_millis,
    ))
    .expect("retained after lease grace");
    assert!(!retained_after_grace.contains(&generation_one.id));
    assert!(retained_after_grace.contains(&generation_two.id));
    assert!(retained_after_grace.contains(&generation_three.id));
    assert!(matches!(
        block_on(store.renew_read_lease(
            &lease,
            chrono::Utc::now(),
            lease.expires_at + chrono::Duration::minutes(1),
        )),
        Err(CoreError::Conflict { reason }) if reason == "GenerationChanged"
    ));
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
    let (store, backend_pid) = repository_with_pid();
    match mode {
        "read-stale" => {
            let eligibility = block_on(store.eligibility(&test_scope)).expect("child eligibility");
            println!("READY|{backend_pid}");
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
        "release-global-id" => {
            let eligibility = block_on(store.eligibility(&test_scope)).expect("child eligibility");
            let lease_id = std::env::var("TERRA_PG_RACE_ATTEMPT").expect("global lease id");
            let expires_at = std::env::var("TERRA_PG_RACE_EPOCH")
                .expect("global lease expiry")
                .parse()
                .expect("global lease expiry timestamp");
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            block_on(store.release_read_lease(&CodeReadLease {
                id: lease_id,
                eligibility,
                fencing_token: 1,
                expires_at,
            }))
            .expect("release lease through repository port");
            println!("LEASE_RELEASED");
        }
        "acquire-namespace" => {
            let lease_id = std::env::var("TERRA_PG_RACE_ATTEMPT").expect("namespace lease id");
            let eligibility = engram_knowledge::CodeEligibility {
                scope: test_scope,
                epoch: std::env::var("TERRA_PG_RACE_LEASE_EPOCH")
                    .expect("namespace lease epoch")
                    .parse()
                    .expect("namespace lease epoch number"),
                generations: vec![],
                repository_ids: vec![],
            };
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            match block_on(
                store.acquire_read_lease(
                    CodeReadLease {
                        id: lease_id,
                        eligibility,
                        fencing_token: 1,
                        expires_at: std::env::var("TERRA_PG_RACE_LEASE_EXPIRY")
                            .expect("namespace lease expiry")
                            .parse()
                            .expect("namespace lease expiry timestamp"),
                    },
                    chrono::Utc::now(),
                ),
            ) {
                Ok(_) => println!("LEASE_OWNER"),
                Err(_) => panic!("namespace lease acquisition failed"),
            }
        }
        "acquire-global-id" => {
            let eligibility = block_on(store.eligibility(&test_scope)).expect("child eligibility");
            let lease_id = std::env::var("TERRA_PG_RACE_ATTEMPT").expect("global lease id");
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            match block_on(
                store.acquire_read_lease(
                    CodeReadLease {
                        id: lease_id,
                        eligibility,
                        fencing_token: 1,
                        expires_at: std::env::var("TERRA_PG_RACE_EPOCH")
                            .expect("global lease expiry")
                            .parse()
                            .expect("global lease expiry timestamp"),
                    },
                    chrono::Utc::now(),
                ),
            ) {
                Ok(_) => println!("LEASE_OWNER"),
                Err(CoreError::Conflict { reason }) if reason == "GenerationChanged" => {
                    println!("LEASE_CONFLICT")
                }
                Err(_) => panic!("unexpected lease acquisition error"),
            }
        }
        "renew-stale" => {
            let lease = CodeReadLease {
                id: std::env::var("TERRA_PG_RACE_LEASE_ID").expect("renewal lease id"),
                eligibility: engram_knowledge::CodeEligibility {
                    scope: test_scope.clone(),
                    epoch: std::env::var("TERRA_PG_RACE_LEASE_EPOCH")
                        .expect("renewal lease epoch")
                        .parse()
                        .expect("renewal lease epoch number"),
                    generations: vec![std::env::var("TERRA_PG_RACE_LEASE_GENERATION")
                        .expect("renewal lease generation")],
                    repository_ids: vec![repo.0.clone()],
                },
                fencing_token: 1,
                expires_at: std::env::var("TERRA_PG_RACE_LEASE_EXPIRY")
                    .expect("renewal lease expiry")
                    .parse()
                    .expect("renewal lease expiry timestamp"),
            };
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            let result = block_on(store.renew_read_lease(
                &lease,
                chrono::Utc::now(),
                lease.expires_at + chrono::Duration::minutes(1),
            ));
            assert!(
                matches!(result, Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
            );
            println!("RENEW_STALE");
        }
        "retain" => {
            let old_generation =
                std::env::var("TERRA_PG_RACE_OLD_GENERATION").expect("old retained generation");
            let next_generation =
                std::env::var("TERRA_PG_RACE_NEXT_GENERATION").expect("next retained generation");
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let mut signal = [0];
            std::io::stdin()
                .read_exact(&mut signal)
                .expect("read barrier");
            let retained = block_on(store.retained_generations(&test_scope, chrono::Utc::now(), 0))
                .expect("retained generations after publication");
            assert!(retained.contains(&old_generation));
            assert!(retained.contains(&next_generation));
            println!("RETAINED");
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
        "begin-zero" => {
            let attempt_id = std::env::var("TERRA_PG_RACE_ATTEMPT").expect("race attempt");
            println!("READY|{backend_pid}");
            std::io::stdout().flush().expect("flush barrier");
            let allocated = block_on(store.begin_attempt(
                &test_scope,
                &repo,
                generation(&format!("race-generation-{attempt_id}")),
                attempt(&attempt_id, 0, None, 0),
            ))
            .expect("child zero allocation");
            println!("FENCE|{}", allocated.fencing_token);
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
    BoundedChild {
        child,
        output,
        backend_pid: None,
    }
}

fn spawn_namespace_child(
    scope: &Scope,
    repo: &str,
    lease_id: &str,
    epoch: u64,
    expiry: &str,
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
        .env("TERRA_PG_RACE_MODE", "acquire-namespace")
        .env("TERRA_PG_RACE_TENANT", &scope.tenant)
        .env(
            "TERRA_PG_RACE_WORKSPACE",
            scope.workspace.as_deref().expect("workspace"),
        )
        .env("TERRA_PG_RACE_REPO", repo)
        .env("TERRA_PG_RACE_ATTEMPT", lease_id)
        .env("TERRA_PG_RACE_LEASE_EPOCH", epoch.to_string())
        .env("TERRA_PG_RACE_LEASE_EXPIRY", expiry)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn namespace child process");
    let output = BufReader::new(child.stdout.take().expect("child stdout"));
    BoundedChild {
        child,
        output,
        backend_pid: None,
    }
}

fn spawn_renewal_child(scope: &Scope, repo: &str, lease: &CodeReadLease) -> BoundedChild {
    let mut child = Command::new("timeout")
        .arg("30s")
        .arg(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "postgres_cross_process_binding_publication_and_read_lease_fencing",
            "--ignored",
            "--nocapture",
        ])
        .env("TERRA_PG_RACE_MODE", "renew-stale")
        .env("TERRA_PG_RACE_TENANT", &scope.tenant)
        .env(
            "TERRA_PG_RACE_WORKSPACE",
            scope.workspace.as_deref().expect("workspace"),
        )
        .env("TERRA_PG_RACE_REPO", repo)
        .env("TERRA_PG_RACE_LEASE_ID", &lease.id)
        .env(
            "TERRA_PG_RACE_LEASE_EPOCH",
            lease.eligibility.epoch.to_string(),
        )
        .env(
            "TERRA_PG_RACE_LEASE_GENERATION",
            lease
                .eligibility
                .generations
                .first()
                .expect("active lease generation"),
        )
        .env("TERRA_PG_RACE_LEASE_EXPIRY", lease.expires_at.to_rfc3339())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn renewal child process");
    let output = BufReader::new(child.stdout.take().expect("child stdout"));
    BoundedChild {
        child,
        output,
        backend_pid: None,
    }
}

fn spawn_retention_child(
    scope: &Scope,
    repo: &str,
    old_generation: &str,
    next_generation: &str,
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
        .env("TERRA_PG_RACE_MODE", "retain")
        .env("TERRA_PG_RACE_TENANT", &scope.tenant)
        .env(
            "TERRA_PG_RACE_WORKSPACE",
            scope.workspace.as_deref().expect("workspace"),
        )
        .env("TERRA_PG_RACE_REPO", repo)
        .env("TERRA_PG_RACE_OLD_GENERATION", old_generation)
        .env("TERRA_PG_RACE_NEXT_GENERATION", next_generation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn retention child process");
    let output = BufReader::new(child.stdout.take().expect("child stdout"));
    BoundedChild {
        child,
        output,
        backend_pid: None,
    }
}

fn wait_for_scope_authority(observer: &PgConnection, pid: i32, scope: &Scope, name: &str) {
    let key = serde_json::to_string(scope).expect("scope authority key");
    let authority = format!("{}:{key}", scope.tenant);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let granted: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND pid=$1 AND granted AND ((classid::bigint << 32) + objid::bigint) = hashtextextended($2, 0) AND objsubid=1)",
            &[&pid, &authority],
        ))
        .expect("observe exact scope authority")
        .get(0);
        if granted {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("child did not grant {name}");
}

fn wait_for_lease_authority(observer: &PgConnection, pid: i32, lease_id: &str, name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let granted: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND pid=$1 AND granted AND classid=($2::int4)::oid AND objid=hashtext($3)::oid AND objsubid=2)",
            &[&pid, &LEASE_ID_ADVISORY_NAMESPACE, &lease_id],
        ))
        .expect("observe exact lease authority")
        .get(0);
        if granted {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("child did not grant {name}");
}

fn wait_for_blocker(observer: &PgConnection, blocker_pid: i32, blocked_pid: i32, name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let blocked: bool = block_on(observer.client.query_one(
            "SELECT $1 = ANY(pg_blocking_pids($2))",
            &[&blocker_pid, &blocked_pid],
        ))
        .expect("observe blocking graph")
        .get(0);
        if blocked {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("child did not wait on {name}");
}

fn wait_for_advisory_wait(observer: &PgConnection, holder_pid: i32, waiter_pid: i32, name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let waiting: bool = block_on(observer.client.query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_locks holder JOIN pg_locks waiter ON waiter.locktype=holder.locktype AND waiter.database IS NOT DISTINCT FROM holder.database AND waiter.classid IS NOT DISTINCT FROM holder.classid AND waiter.objid IS NOT DISTINCT FROM holder.objid AND waiter.objsubid IS NOT DISTINCT FROM holder.objsubid WHERE holder.locktype='advisory' AND holder.pid=$1 AND holder.granted AND waiter.pid=$2 AND NOT waiter.granted)",
            &[&holder_pid, &waiter_pid],
        ))
        .expect("observe exact advisory wait")
        .get(0);
        if waiting {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("child did not wait on {name}");
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
        if let Some(pid) = line.trim().strip_prefix("READY|") {
            child.backend_pid = Some(pid.parse().expect("child backend pid"));
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
    repository_with_pid().0
}

fn repository_with_pid() -> (PgCodeBindingRepository, i32) {
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
    let backend_pid = connection.block_on(async {
        connection
            .client
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .expect("backend pid")
            .get(0)
    });
    (
        PgCodeBindingRepository::new(connection).expect("initialize code-binding port"),
        backend_pid,
    )
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

fn publish_generation(
    store: &PgCodeBindingRepository,
    scope: &Scope,
    repo: &str,
    generation: CodeGeneration,
    expected_epoch: u64,
    expected_active_generation: Option<String>,
    attempt_id: &str,
) {
    let allocated = block_on(store.begin_attempt(
        scope,
        &RepoId(repo.into()),
        generation.clone(),
        attempt(attempt_id, expected_epoch, expected_active_generation, 0),
    ))
    .expect("allocate publication attempt");
    receipt(store, &allocated, &generation);
    block_on(store.mark_ready(
        scope,
        &RepoId(repo.into()),
        &allocated.id,
        allocated.fencing_token,
    ))
    .expect("ready publication attempt");
    block_on(store.publish(scope, &RepoId(repo.into()), &allocated.id, expected_epoch))
        .expect("publish generation");
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
