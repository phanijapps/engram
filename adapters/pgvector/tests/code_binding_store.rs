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
    fn same_bind_idempotent() {
        let r = id();
        let w = id();
        let mut s = store();
        let b = binding(&w, &r, "refs/heads/main");
        assert_eq!(s.bind(&b).unwrap(), s.bind(&b).unwrap());
    }
    #[test]
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
