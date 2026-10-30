use engram_domain::{CodeBinding, RepoId, Scope};
use engram_store_pgvector::code_binding::PgCodeBindingStore;
use engram_store_pgvector::PgConnection;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

mod task_006 {
    use super::*;
    fn url() -> String {
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
    }
    fn id() -> String {
        format!(
            "t{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
    fn scope(w: &str) -> Scope {
        Scope {
            tenant: "factory-test".into(),
            subject: None,
            workspace: Some(w.into()),
            session: None,
            environment: None,
        }
    }
    fn binding(w: &str, r: &str, b: &str) -> CodeBinding {
        CodeBinding {
            scope: scope(w),
            repo_id: RepoId(r.into()),
            branch_ref: b.into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }
    }
    fn store() -> PgCodeBindingStore {
        let c = PgConnection::connect(&url()).expect("connect disposable test PostgreSQL");
        let marker=c.block_on(async{c.client.query_one("SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()",&[]).await.unwrap().get::<_,Option<String>>(0)});
        assert_eq!(
            marker.as_deref(),
            Some("engram.factory.job=010-project-bound-branch-indexing; disposable=true")
        );
        let s = PgCodeBindingStore::new(c);
        s.initialize().unwrap();
        s
    }
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn restart_two_scopes() {
        let r = id();
        let w1 = id();
        let w2 = id();
        let mut s = store();
        s.bind(&binding(&w1, &r, "refs/heads/main")).unwrap();
        s.bind(&binding(&w2, &r, "refs/heads/dev")).unwrap();
        drop(s);
        let s = store();
        assert_eq!(
            s.get_binding(&scope(&w1), &RepoId(r.clone()))
                .unwrap()
                .unwrap()
                .branch_ref,
            "refs/heads/main"
        );
        assert_eq!(
            s.get_binding(&scope(&w2), &RepoId(r))
                .unwrap()
                .unwrap()
                .branch_ref,
            "refs/heads/dev"
        );
    }
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn same_bind_idempotent() {
        let r = id();
        let w = id();
        let mut s = store();
        let b = binding(&w, &r, "refs/heads/main");
        assert_eq!(s.bind(&b).unwrap(), s.bind(&b).unwrap());
    }
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn conflict_preserves_pointer() {
        let r = id();
        let w = id();
        let mut s = store();
        s.bind(&binding(&w, &r, "refs/heads/main")).unwrap();
        assert_eq!(
            s.bind(&binding(&w, &r, "refs/heads/other")).unwrap_err(),
            engram_domain::CodeBindingError::BindingConflict
        );
        assert_eq!(
            s.get_binding(&scope(&w), &RepoId(r))
                .unwrap()
                .unwrap()
                .branch_ref,
            "refs/heads/main"
        );
    }
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn concurrent_first_bind_processes() {
        let r = id();
        let w = id();
        let url = url();
        let s = store();
        let b = binding(&w, &r, "refs/heads/main");
        let b2 = binding(&w, &r, "refs/heads/dev");
        let db_path = std::env::temp_dir().join(format!("engram-pg-child-{w}-{r}"));
        std::fs::write(&db_path, format!("{w}\n{r}")).unwrap();
        drop(s);
        let mut children = Vec::new();
        for branch in ["refs/heads/main", "refs/heads/dev"] {
            let mut c = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "task_006::pg_child_worker", "--nocapture"])
                .env("ENGRAM_PG_CHILD", "1")
                .env("ENGRAM_TEST_PG_URL", &url)
                .env("ENGRAM_PG_CHILD_DB", &db_path)
                .env("ENGRAM_PG_CHILD_BRANCH", branch)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut reader = BufReader::new(c.stdout.take().unwrap());
            let mut ready = String::new();
            while !ready.contains("READY") {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    panic!("child exited before barrier")
                }
                ready.push_str(&line)
            }
            children.push((c, reader));
        }
        for (c, _) in &mut children {
            c.stdin.take().unwrap().write_all(b"G").unwrap();
        }
        let mut outcomes = Vec::new();
        for (mut c, mut rd) in children {
            let mut out = String::new();
            rd.read_to_string(&mut out).unwrap();
            assert!(c.wait().unwrap().success(), "child failed");
            outcomes.push(
                out.lines()
                    .find(|x| x.starts_with("RESULT:"))
                    .unwrap()
                    .to_owned(),
            );
        }
        assert_eq!(
            outcomes
                .iter()
                .filter(|x| x.as_str() == "RESULT:ok")
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|x| x.as_str() == "RESULT:conflict")
                .count(),
            1
        );
        let s = store();
        let actual = s.get_binding(&scope(&w), &RepoId(r)).unwrap().unwrap();
        assert!(actual.branch_ref == b.branch_ref || actual.branch_ref == b2.branch_ref);
        std::fs::remove_file(db_path).unwrap();
    }
    #[test]
    fn pg_child_worker() {
        if std::env::var_os("ENGRAM_PG_CHILD").is_none() {
            return;
        }
        let mut f = std::fs::File::open(std::env::var("ENGRAM_PG_CHILD_DB").unwrap()).unwrap();
        let mut identity = String::new();
        f.read_to_string(&mut identity).unwrap();
        let mut lines = identity.lines();
        let w = lines.next().unwrap();
        let r = lines.next().unwrap();
        let branch = std::env::var("ENGRAM_PG_CHILD_BRANCH").unwrap();
        let mut out = std::io::stdout().lock();
        writeln!(out, "READY").unwrap();
        out.flush().unwrap();
        let mut byte = [0];
        std::io::stdin().read_exact(&mut byte).unwrap();
        let result = store().bind(&binding(w, r, &branch));
        match result {
            Ok(_) => println!("RESULT:ok"),
            Err(engram_domain::CodeBindingError::BindingConflict) => println!("RESULT:conflict"),
            Err(e) => panic!("database failure {e:?}"),
        }
    }
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn fenced_publication_cas() {
        let r = id();
        let w = id();
        let mut s = store();
        let b = binding(&w, &r, "refs/heads/main");
        s.bind(&b).unwrap();
        let generation_id = id();
        let attempt_id = id();
        let g = engram_domain::CodeGeneration {
            id: generation_id.clone(),
            binding_version: 1,
            commit_oid: "a".repeat(40),
            snapshot_digest: "b".repeat(64),
            parser_fingerprint: "p".into(),
            filter_fingerprint: "f".into(),
        };
        let a = engram_domain::CodeAttempt {
            id: attempt_id.clone(),
            idempotency_key: attempt_id.clone(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: engram_domain::CodeAttemptState::Ready,
            receipt_kinds: vec![engram_domain::CodeReceiptKind::Knowledge],
        };
        s.begin_attempt(&b.scope, &b.repo_id, &g, &a).unwrap();
        assert!(s.publish(&b.scope, &b.repo_id, &attempt_id, 0).is_err());
        s.record_receipt(&engram_knowledge::StageReceipt {
            attempt_id: attempt_id.clone(),
            generation_id: generation_id.clone(),
            lane: "knowledge".into(),
            digest: "d".into(),
        })
        .unwrap();
        let published = s.publish(&b.scope, &b.repo_id, &attempt_id, 0).unwrap();
        assert_eq!(
            published.active_generation.as_deref(),
            Some(generation_id.as_str())
        );
        assert_eq!(published.epoch, 1);
        assert_eq!(
            s.publish(&b.scope, &b.repo_id, &attempt_id, 0).unwrap(),
            published,
            "post-CAS retry returns publication idempotently"
        );
    }
}

/// PREPARED (COMPILE-ONLY) contract cases for the ADOPTED repeated-
/// retirement branches on the PostgreSQL cell (review r5 verification
/// finding 1 / original findings 3+10): the boolean retirement decode this
/// bug blocked from ever running is exercised here through the repository
/// port's `retire_lineage_targets` — whole-set typed-association
/// validation, the idempotent matching no-op WITHOUT epoch advance,
/// stale-epoch rejection even for no-ops, mixed already-retired + fresh
/// sets, and rejection without mutation for wrong-generation,
/// wrong-repository and missing targets.
///
/// NOT EXECUTED in this tranche: no PostgreSQL execution authority exists
/// (D5/D6 withholding). Like every test in this file they require the
/// `ENGRAM_TEST_PG_URL` disposable marked database and run only under the
/// separately granted PostgreSQL lane; `cargo test -p engram-store-pgvector
/// --no-run` is the compile verification recorded for them.
mod maintenance_retirement_contract {
    use engram_domain::{
        CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeLineage, CodeReceiptKind,
        RepoId, Scope,
    };
    use engram_knowledge::{CodeBindingRepository, RetirementTarget, StageReceipt};
    use engram_runtime::CoreError;
    use engram_store_pgvector::{PgCodeBindingRepository, PgConnection};
    use futures::executor::block_on;

    fn url() -> String {
        std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
    }
    fn id() -> String {
        format!(
            "t{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
    fn scope(w: &str) -> Scope {
        Scope {
            tenant: "factory-test".into(),
            subject: None,
            workspace: Some(w.into()),
            session: None,
            environment: None,
        }
    }
    fn repository() -> PgCodeBindingRepository {
        let connection = PgConnection::connect(&url()).expect("connect disposable test PostgreSQL");
        let marker = connection.block_on(async {
            connection
                .client
                .query_one(
                    "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()",
                    &[],
                )
                .await
                .unwrap()
                .get::<_, Option<String>>(0)
        });
        assert_eq!(
            marker.as_deref(),
            Some("engram.factory.job=010-project-bound-branch-indexing; disposable=true")
        );
        PgCodeBindingRepository::new(connection).expect("initialize repository port")
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
    fn attempt(id: &str, expected_epoch: u64) -> CodeAttempt {
        CodeAttempt {
            id: id.into(),
            idempotency_key: format!("idem-{id}"),
            binding_version: 1,
            expected_epoch,
            expected_active_generation: None,
            fencing_token: 0,
            state: CodeAttemptState::Created,
            receipt_kinds: vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge],
        }
    }
    fn receipt(attempt_id: &str, generation_id: &str, lane: CodeReceiptKind) -> StageReceipt {
        StageReceipt {
            attempt_id: attempt_id.into(),
            generation_id: generation_id.to_string(),
            lane: serde_json::to_value(&lane)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
            digest: format!("digest-{attempt_id}-{lane:?}"),
        }
    }
    fn target(repo: &str, generation_id: &str, record: &str) -> RetirementTarget {
        RetirementTarget {
            repo_id: repo.into(),
            generation_id: generation_id.into(),
            record_id: record.into(),
        }
    }
    /// Full durable publication state machine over the PORT (bind → attempt
    /// → receipts → ready → publish → lineage); returns the binding epoch.
    fn publish(
        repository: &PgCodeBindingRepository,
        workspace: &str,
        repo: &str,
        generation_id: &str,
        records: &[&str],
    ) -> u64 {
        let scope = scope(workspace);
        let repo_id = RepoId(repo.to_string());
        let attempt_id = format!("attempt-{workspace}-{repo}-{generation_id}");
        block_on(repository.bind(CodeBinding {
            scope: scope.clone(),
            repo_id: repo_id.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        block_on(repository.begin_attempt(
            &scope,
            &repo_id,
            generation(generation_id),
            attempt(&attempt_id, 0),
        ))
        .expect("begin attempt");
        for lane in [CodeReceiptKind::Source, CodeReceiptKind::Knowledge] {
            block_on(repository.record_receipt(receipt(&attempt_id, generation_id, lane)))
                .expect("receipt");
        }
        let (_, stored) = block_on(repository.get_attempt(&scope, &repo_id, &attempt_id))
            .expect("get attempt")
            .expect("attempt exists");
        block_on(repository.mark_ready(&scope, &repo_id, &attempt_id, stored.fencing_token))
            .expect("mark ready");
        let binding =
            block_on(repository.publish(&scope, &repo_id, &attempt_id, 0)).expect("publish");
        for record in records {
            block_on(repository.put_lineage(
                &scope,
                record,
                CodeLineage {
                    repo_id: repo_id.clone(),
                    generation_id: generation_id.to_string(),
                    source_path: format!("src/{record}.rs"),
                    contributors: vec![record.to_string()],
                    span: None,
                },
            ))
            .expect("put lineage");
        }
        binding.epoch
    }
    fn binding_epoch(repository: &PgCodeBindingRepository, workspace: &str, repo: &str) -> u64 {
        block_on(repository.get_binding(&scope(workspace), &RepoId(repo.into())))
            .expect("get binding")
            .expect("bound")
            .epoch
    }
    fn scope_epoch(repository: &PgCodeBindingRepository, workspace: &str) -> u64 {
        block_on(repository.eligibility(&scope(workspace)))
            .expect("eligibility")
            .epoch
    }
    fn retired(repository: &PgCodeBindingRepository, workspace: &str, record: &str) -> bool {
        block_on(repository.record_lineage(&scope(workspace), record))
            .expect("record lineage")
            .expect("lineage exists")
            .retired
    }
    fn stale(err: &CoreError) -> bool {
        matches!(err, CoreError::Conflict { reason } if reason.contains("GenerationChanged"))
    }

    /// The ADOPTED repeated-retirement contract: a matching repeated
    /// retirement is an idempotent no-op SUCCESS that advances NEITHER the
    /// binding epoch NOR the scope epoch and performs no write.
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn repeated_matching_retirement_is_noop_success() {
        let workspace = id();
        let repository = repository();
        let epoch = publish(&repository, &workspace, "repo-r", "gen-1", &["rec-a"]);
        assert_eq!(epoch, 1);
        block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("first retirement commits");
        assert_eq!((), ());
        let after_first = binding_epoch(&repository, &workspace, "repo-r");
        let scope_after_first = scope_epoch(&repository, &workspace);
        assert!(after_first > 1, "the real retirement advanced the epoch");
        // Repeated with the CURRENT (post-retirement) epoch: a no-op success.
        block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), after_first)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("matching repeated retirement is a no-op success");
        assert_eq!(
            binding_epoch(&repository, &workspace, "repo-r"),
            after_first,
            "no binding-epoch advance for the no-op"
        );
        assert_eq!(
            scope_epoch(&repository, &workspace),
            scope_after_first,
            "no scope-epoch advance for the no-op"
        );
        assert!(retired(&repository, &workspace, "rec-a"));
    }

    /// A wrong/old-generation target from an ALLOWED repository rejects the
    /// whole operation with NOTHING mutated (the boolean decode under test
    /// runs for every existing target).
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn wrong_generation_target_rejects_without_mutation() {
        let workspace = id();
        let repository = repository();
        let epoch = publish(&repository, &workspace, "repo-r", "gen-1", &["rec-a"]);
        assert_eq!(epoch, 1);
        let denied = block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-2", "rec-a")],
        ))
        .expect_err("wrong generation rejects");
        assert!(stale(&denied), "{denied:?}");
        assert!(!retired(&repository, &workspace, "rec-a"));
        assert_eq!(
            binding_epoch(&repository, &workspace, "repo-r"),
            1,
            "no epoch advance"
        );
    }

    /// A declared-repository mismatch against the durable lineage rejects
    /// identically (the typed association is validated as a whole).
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn wrong_repository_association_rejects() {
        let workspace = id();
        let repository = repository();
        publish(&repository, &workspace, "repo-r", "gen-1", &["rec-a"]);
        let denied = block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-other", "gen-1", "rec-a")],
        ));
        assert!(stale(
            &denied.expect_err("declared repo must match durable lineage")
        ));
        assert!(!retired(&repository, &workspace, "rec-a"));
    }

    /// A stale expected epoch rejects even when every target is already
    /// retired (the no-op path still validates the fence).
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn stale_epoch_rejects_even_for_noop() {
        let workspace = id();
        let repository = repository();
        publish(&repository, &workspace, "repo-r", "gen-1", &["rec-a"]);
        block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("first retirement");
        let advanced = binding_epoch(&repository, &workspace, "repo-r");
        let denied = block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), advanced - 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect_err("stale epoch rejects even for a no-op");
        assert!(stale(&denied), "{denied:?}");
    }

    /// A mixed set of an ALREADY-RETIRED target and a fresh one succeeds
    /// and advances the epochs exactly ONCE (whole-set validation first,
    /// then one fenced mutation).
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn mixed_retired_and_fresh_retire_successfully() {
        let workspace = id();
        let repository = repository();
        publish(
            &repository,
            &workspace,
            "repo-r",
            "gen-1",
            &["rec-a", "rec-b"],
        );
        block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("first retirement");
        let between = binding_epoch(&repository, &workspace, "repo-r");
        let scope_between = scope_epoch(&repository, &workspace);
        block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), between)],
            &[
                target("repo-r", "gen-1", "rec-a"),
                target("repo-r", "gen-1", "rec-b"),
            ],
        ))
        .expect("mixed already-retired + fresh succeeds");
        assert_eq!(
            binding_epoch(&repository, &workspace, "repo-r"),
            between + 1,
            "exactly one advance"
        );
        assert_eq!(
            scope_epoch(&repository, &workspace),
            scope_between + 1,
            "scope epoch advances exactly once"
        );
        assert!(retired(&repository, &workspace, "rec-a"));
        assert!(retired(&repository, &workspace, "rec-b"));
    }

    /// A MISSING target record rejects the whole set without mutation.
    #[test]
    #[ignore = "requires ENGRAM_TEST_PG_URL; run via --ignored with the sourced env"]
    fn missing_target_rejects() {
        let workspace = id();
        let repository = repository();
        publish(&repository, &workspace, "repo-r", "gen-1", &["rec-a"]);
        let denied = block_on(repository.retire_lineage_targets(
            &scope(&workspace),
            &[(RepoId("repo-r".into()), 1)],
            &[
                target("repo-r", "gen-1", "rec-a"),
                target("repo-r", "gen-1", "rec-never-written"),
            ],
        ))
        .expect_err("a missing target rejects the whole set");
        assert!(stale(&denied), "{denied:?}");
        assert!(
            !retired(&repository, &workspace, "rec-a"),
            "nothing retired"
        );
    }
}
