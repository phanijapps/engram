use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
};
use engram_store_sqlite::knowledge::code_binding::SqlCodeBindingStore;
use tempfile::tempdir;

mod task_005 {
    use super::*;
    fn scope(w: &str) -> Scope {
        Scope {
            tenant: "t".into(),
            subject: None,
            workspace: Some(w.into()),
            session: None,
            environment: None,
        }
    }
    fn binding(w: &str, branch: &str) -> CodeBinding {
        CodeBinding {
            scope: scope(w),
            repo_id: RepoId("repo".into()),
            branch_ref: branch.into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }
    }
    fn attempt(id: &str) -> CodeAttempt {
        CodeAttempt {
            id: id.into(),
            idempotency_key: id.into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Ready,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        }
    }
    fn generation(id: &str) -> CodeGeneration {
        CodeGeneration {
            id: id.into(),
            binding_version: 1,
            commit_oid: "a".repeat(40),
            snapshot_digest: "b".repeat(64),
            parser_fingerprint: "p".into(),
            filter_fingerprint: "f".into(),
        }
    }
    #[test]
    fn restart_two_scopes() {
        let d = tempdir().unwrap();
        let p = d.path().join("s.db");
        {
            let mut s = SqlCodeBindingStore::open(&p).unwrap();
            s.bind(&binding("w1", "refs/heads/main")).unwrap();
            s.bind(&binding("w2", "refs/heads/dev")).unwrap();
        }
        let s = SqlCodeBindingStore::open(&p).unwrap();
        assert_eq!(
            s.binding(&scope("w1"), "repo").unwrap().unwrap().branch_ref,
            "refs/heads/main"
        );
        assert_eq!(
            s.binding(&scope("w2"), "repo").unwrap().unwrap().branch_ref,
            "refs/heads/dev"
        );
    }
    #[test]
    fn same_bind_idempotent() {
        let d = tempdir().unwrap();
        let mut s = SqlCodeBindingStore::open(d.path().join("s.db")).unwrap();
        let b = binding("w", "refs/heads/main");
        assert_eq!(s.bind(&b).unwrap(), s.bind(&b).unwrap());
    }
    #[test]
    fn conflict_preserves_pointer() {
        let d = tempdir().unwrap();
        let mut s = SqlCodeBindingStore::open(d.path().join("s.db")).unwrap();
        s.bind(&binding("w", "refs/heads/main")).unwrap();
        assert!(s.bind(&binding("w", "refs/heads/other")).is_err());
        assert_eq!(
            s.binding(&scope("w"), "repo").unwrap().unwrap().branch_ref,
            "refs/heads/main"
        );
    }
    #[test]
    fn concurrent_first_bind_processes() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::process::{Command, Stdio};
        let d = tempdir().unwrap();
        let path = d.path().join("process.db");
        // Initialize schema before the competing processes so this test isolates first-bind races.
        drop(SqlCodeBindingStore::open(&path).unwrap());
        let mut children = Vec::new();
        for branch in ["refs/heads/main", "refs/heads/dev"] {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "task_005::concurrent_child_worker",
                    "--nocapture",
                ])
                .env("BOUND_CODE_CHILD_DB", &path)
                .env("BOUND_CODE_CHILD_BRANCH", branch)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut reader = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            while !ready.contains("READY") {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    panic!("child exited before barrier: {ready}");
                }
                ready.push_str(&line);
            }
            children.push((child, reader));
        }
        for (child, _) in &mut children {
            child.stdin.take().unwrap().write_all(b"G").unwrap();
        }
        let mut results = Vec::new();
        for (mut child, mut reader) in children {
            let mut output = String::new();
            reader.read_to_string(&mut output).unwrap();
            let status = child.wait().unwrap();
            assert!(status.success(), "child failed: {output}");
            let result = output
                .lines()
                .find(|line| line.starts_with("RESULT:"))
                .unwrap()
                .to_owned();
            results.push(result);
        }
        assert_eq!(
            results.iter().filter(|s| s.as_str() == "RESULT:ok").count(),
            1,
            "{results:?}"
        );
        assert_eq!(
            results
                .iter()
                .filter(|s| s.as_str() == "RESULT:conflict")
                .count(),
            1,
            "{results:?}"
        );
        let store = SqlCodeBindingStore::open(&path).unwrap();
        assert!(
            ["refs/heads/main", "refs/heads/dev"].contains(
                &store
                    .binding(&scope("w"), "repo")
                    .unwrap()
                    .unwrap()
                    .branch_ref
                    .as_str()
            )
        );
    }
    #[test]
    fn concurrent_child_worker() {
        use std::io::{Read, Write};
        let Ok(path) = std::env::var("BOUND_CODE_CHILD_DB") else {
            return;
        };
        let branch = std::env::var("BOUND_CODE_CHILD_BRANCH").unwrap();
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        writeln!(out, "READY").unwrap();
        out.flush().unwrap();
        let mut byte = [0];
        std::io::stdin().read_exact(&mut byte).unwrap();
        let mut store = SqlCodeBindingStore::open(path).unwrap();
        match store.bind(&binding("w", &branch)) {
            Ok(_) => println!("RESULT:ok"),
            Err(engram_domain::CodeBindingError::BindingConflict) => println!("RESULT:conflict"),
            Err(e) => panic!("unexpected persistence failure: {e:?}"),
        }
    }
    #[test]
    fn idempotent_generation_and_attempt_ownership() {
        let d = tempdir().unwrap();
        let mut s = SqlCodeBindingStore::open(d.path().join("s.db")).unwrap();
        let b = binding("w", "refs/heads/main");
        s.bind(&b).unwrap();
        let g = generation("g1");
        let a = attempt("a1");
        s.begin_attempt(&b.scope, "repo", &g, &a).unwrap();
        s.begin_attempt(&b.scope, "repo", &g, &a).unwrap();
        let mut conflict = g.clone();
        conflict.snapshot_digest = "c".repeat(64);
        assert!(
            s.stage_generation(&conflict).is_err(),
            "same generation ID cannot hide different payload"
        );
        let wrong_scope = scope("foreign");
        assert!(s.begin_attempt(&wrong_scope, "repo", &g, &a).is_err());
        let mut altered = a.clone();
        altered.fencing_token = 2;
        assert!(
            s.begin_attempt(&b.scope, "repo", &g, &altered).is_err(),
            "idempotency key cannot be rebound to another fence"
        );
    }
    #[test]
    fn fenced_publication_cas() {
        let d = tempdir().unwrap();
        let mut s = SqlCodeBindingStore::open(d.path().join("s.db")).unwrap();
        let b = binding("w", "refs/heads/main");
        s.bind(&b).unwrap();
        s.begin_attempt(&b.scope, "repo", &generation("g1"), &attempt("a1"))
            .unwrap();
        assert!(s.publish(&b.scope, "repo", "a1").is_err());
        s.record_receipt("a1", "knowledge", "digest").unwrap();
        let published = s.publish(&b.scope, "repo", "a1").unwrap();
        assert_eq!(published.active_generation.as_deref(), Some("g1"));
        assert_eq!(published.epoch, 1);
    }
}
