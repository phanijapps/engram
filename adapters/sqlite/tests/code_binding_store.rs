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

/// Review r5 (findings 3/10): the ADOPTED repeated-retirement contract for
/// the typed owning-store fence, executed against the REAL SQLite port —
/// whole-set validation before any mutation, exact-generation association
/// enforcement, idempotent matching no-op without epoch advance, stale-epoch
/// rejection even for no-ops, and mixed-set atomicity.
mod maintenance_retirement_contract {
    use engram_domain::{
        CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeReceiptKind, RepoId, Scope,
    };
    use engram_knowledge::{CodeBindingRepository, RetirementTarget, StageReceipt};
    use engram_runtime::CoreError;
    use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
    use futures::executor::block_on;
    use tempfile::tempdir;

    fn scope(w: &str) -> Scope {
        Scope {
            tenant: "retirement-contract".into(),
            subject: None,
            workspace: Some(w.into()),
            session: None,
            environment: None,
        }
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
    /// → receipts → ready → publish → lineage), returning the binding epoch.
    fn publish(
        store: &SqlCodeBindingRepository,
        workspace: &str,
        repo: &str,
        generation_id: &str,
        expected_epoch: u64,
        records: &[&str],
    ) -> u64 {
        let scope = scope(workspace);
        let repo_id = RepoId(repo.to_string());
        let attempt_id = format!("attempt-{workspace}-{repo}-{generation_id}");
        block_on(store.bind(CodeBinding {
            scope: scope.clone(),
            repo_id: repo_id.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: None,
            epoch: expected_epoch,
        }))
        .expect("bind");
        block_on(store.begin_attempt(
            &scope,
            &repo_id,
            generation(generation_id),
            attempt(&attempt_id, expected_epoch),
        ))
        .expect("begin attempt");
        block_on(store.record_receipt(receipt(
            &attempt_id,
            generation_id,
            CodeReceiptKind::Source,
        )))
        .expect("source receipt");
        block_on(store.record_receipt(receipt(
            &attempt_id,
            generation_id,
            CodeReceiptKind::Knowledge,
        )))
        .expect("knowledge receipt");
        let (_, stored) = block_on(store.get_attempt(&scope, &repo_id, &attempt_id))
            .expect("get attempt")
            .expect("attempt exists");
        block_on(store.mark_ready(&scope, &repo_id, &attempt_id, stored.fencing_token))
            .expect("mark ready");
        let binding = block_on(store.publish(&scope, &repo_id, &attempt_id, expected_epoch))
            .expect("publish");
        for record in records {
            block_on(store.put_lineage(
                &scope,
                record,
                engram_domain::CodeLineage {
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

    fn binding_epoch(store: &SqlCodeBindingRepository, workspace: &str, repo: &str) -> u64 {
        block_on(store.get_binding(&scope(workspace), &RepoId(repo.into())))
            .expect("get binding")
            .expect("bound")
            .epoch
    }

    fn scope_epoch(store: &SqlCodeBindingRepository, workspace: &str) -> u64 {
        block_on(store.eligibility(&scope(workspace)))
            .expect("eligibility")
            .epoch
    }

    fn lineage(store: &SqlCodeBindingRepository, workspace: &str, record: &str) -> bool {
        block_on(store.record_lineage(&scope(workspace), record))
            .expect("record lineage")
            .expect("lineage exists")
            .retired
    }

    fn stale(err: &CoreError) -> bool {
        matches!(err, CoreError::Conflict { reason } if reason.contains("GenerationChanged"))
    }

    /// Wrong/old-generation targets from an ALLOWED repository reject the
    /// whole operation with NOTHING mutated (finding 3: repository
    /// membership alone is not authority).
    #[test]
    fn wrong_generation_target_rejects_without_mutation() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        let epoch = publish(&store, "wrong-gen", "repo-r", "gen-1", 0, &["rec-a"]);
        assert_eq!(epoch, 1);
        let denied = block_on(store.retire_lineage_targets(
            &scope("wrong-gen"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-2", "rec-a")],
        ));
        let denied = denied.expect_err("wrong generation rejects");
        assert!(stale(&denied), "{denied:?}");
        assert!(!lineage(&store, "wrong-gen", "rec-a"), "nothing retired");
        assert_eq!(
            binding_epoch(&store, "wrong-gen", "repo-r"),
            1,
            "no epoch advance"
        );
    }

    /// A declared-repository mismatch against the durable lineage rejects
    /// identically (the typed association is validated as a whole).
    #[test]
    fn wrong_repository_association_rejects() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        publish(&store, "wrong-repo", "repo-r", "gen-1", 0, &["rec-a"]);
        let denied = block_on(store.retire_lineage_targets(
            &scope("wrong-repo"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-other", "gen-1", "rec-a")],
        ));
        assert!(stale(
            &denied.expect_err("declared repo must match durable lineage")
        ));
        assert!(!lineage(&store, "wrong-repo", "rec-a"));
        assert_eq!(binding_epoch(&store, "wrong-repo", "repo-r"), 1);
    }

    /// A target with no durable lineage rejects, and a foreign-repository
    /// target (durable lineage outside the fenced set) rejects.
    #[test]
    fn missing_and_foreign_targets_reject() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        publish(&store, "missing", "repo-r", "gen-1", 0, &["rec-a"]);
        let missing = block_on(store.retire_lineage_targets(
            &scope("missing"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-never-written")],
        ));
        assert!(stale(&missing.expect_err("missing lineage rejects")));

        publish(&store, "foreign", "repo-r", "gen-1", 0, &["rec-x"]);
        publish(&store, "foreign", "repo-other", "gen-o", 0, &["rec-o"]);
        let foreign = block_on(store.retire_lineage_targets(
            &scope("foreign"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-other", "gen-o", "rec-o")],
        ));
        assert!(stale(
            &foreign.expect_err("foreign repository target rejects")
        ));
        assert!(!lineage(&store, "foreign", "rec-o"));
    }

    /// Matching repeated retirement is an idempotent NO-OP SUCCESS without
    /// advancing binding or scope epochs; a stale/fenced epoch still rejects
    /// even when nothing needs retirement (parent-adopted contract).
    #[test]
    fn repeated_matching_retirement_is_idempotent_noop() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        publish(&store, "noop", "repo-r", "gen-1", 0, &["rec-a"]);
        let first = block_on(store.retire_lineage_targets(
            &scope("noop"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ));
        first.expect("first retirement succeeds");
        assert!(lineage(&store, "noop", "rec-a"));
        assert_eq!(binding_epoch(&store, "noop", "repo-r"), 2);
        let scope_epoch_after_first = scope_epoch(&store, "noop");

        // Repeated with the CURRENT (post-retirement) epoch: no-op success.
        block_on(store.retire_lineage_targets(
            &scope("noop"),
            &[(RepoId("repo-r".into()), 2)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("matching repeated retirement is an idempotent no-op");
        assert_eq!(
            binding_epoch(&store, "noop", "repo-r"),
            2,
            "no binding-epoch advance on the no-op"
        );
        assert_eq!(
            scope_epoch(&store, "noop"),
            scope_epoch_after_first,
            "no scope-epoch advance on the no-op"
        );
        assert!(lineage(&store, "noop", "rec-a"), "still retired");

        // Stale/fenced epoch rejects even though nothing needs retirement.
        let stale_repeat = block_on(store.retire_lineage_targets(
            &scope("noop"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect_err("stale epoch rejects even for a no-op");
        assert!(stale(&stale_repeat), "{stale_repeat:?}");
    }

    /// The WHOLE set is validated before ANY mutation: one invalid member
    /// rejects the set with the valid member untouched and no epoch advance;
    /// a mixed set of already-retired + fresh targets succeeds, retires the
    /// fresh member and advances epochs exactly once.
    #[test]
    fn mixed_set_validates_all_before_any_mutation() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        publish(&store, "mixed", "repo-r", "gen-1", 0, &["rec-a", "rec-b"]);

        // Invalid member (wrong generation): nothing mutates.
        let denied = block_on(store.retire_lineage_targets(
            &scope("mixed"),
            &[(RepoId("repo-r".into()), 1)],
            &[
                target("repo-r", "gen-1", "rec-a"),
                target("repo-r", "gen-9", "rec-b"),
            ],
        ));
        assert!(stale(
            &denied.expect_err("one invalid member rejects the set")
        ));
        assert!(
            !lineage(&store, "mixed", "rec-a") && !lineage(&store, "mixed", "rec-b"),
            "whole-set validation means NO partial retirement"
        );
        assert_eq!(binding_epoch(&store, "mixed", "repo-r"), 1);

        // Retire rec-a first, then a mixed [already-retired, fresh] set with
        // the current epoch: success, rec-b retired, epochs advance ONCE.
        block_on(store.retire_lineage_targets(
            &scope("mixed"),
            &[(RepoId("repo-r".into()), 1)],
            &[target("repo-r", "gen-1", "rec-a")],
        ))
        .expect("first retirement");
        assert_eq!(binding_epoch(&store, "mixed", "repo-r"), 2);
        block_on(store.retire_lineage_targets(
            &scope("mixed"),
            &[(RepoId("repo-r".into()), 2)],
            &[
                target("repo-r", "gen-1", "rec-a"),
                target("repo-r", "gen-1", "rec-b"),
            ],
        ))
        .expect("mixed already-retired + fresh set succeeds");
        assert!(lineage(&store, "mixed", "rec-a") && lineage(&store, "mixed", "rec-b"));
        assert_eq!(
            binding_epoch(&store, "mixed", "repo-r"),
            3,
            "epochs advance exactly once for the one target that needed retirement"
        );
    }

    /// Empty repos or targets are invalid requests, not no-ops.
    #[test]
    fn empty_fence_inputs_reject() {
        let dir = tempdir().expect("tempdir");
        let store =
            SqlCodeBindingRepository::open(dir.path().join("contract.sqlite")).expect("open store");
        publish(&store, "empty", "repo-r", "gen-1", 0, &["rec-a"]);
        for call in [
            block_on(store.retire_lineage_targets(
                &scope("empty"),
                &[],
                &[target("repo-r", "gen-1", "rec-a")],
            )),
            block_on(store.retire_lineage_targets(
                &scope("empty"),
                &[(RepoId("repo-r".into()), 1)],
                &[],
            )),
        ] {
            assert!(
                matches!(call, Err(CoreError::InvalidRequest { .. })),
                "fence inputs must be explicit: {call:?}"
            );
        }
    }
}

/// Final r5 C3 (finding 2): the durable canonical source path and line span
/// staged with a record's lineage survive the put_lineage → target_lineage
/// round trip (record_lineage too) — the revision-bound descriptors the
/// exact-read window validates mutable claims against.
mod canonical_span_roundtrip {
    use super::*;
    use engram_knowledge::CodeBindingRepository as _;
    use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
    use futures::executor::block_on;

    #[test]
    fn target_lineage_carries_durable_path_and_span() {
        let dir = tempdir().expect("tempdir");
        let store = SqlCodeBindingRepository::open(dir.path().join("spans.sqlite")).expect("open");
        let scope = Scope {
            tenant: "t".into(),
            subject: None,
            workspace: Some("w-spans".into()),
            session: None,
            environment: None,
        };
        let repo_id = RepoId("repo-span".into());
        block_on(store.bind(CodeBinding {
            scope: scope.clone(),
            repo_id: repo_id.clone(),
            branch_ref: "refs/heads/main".into(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        block_on(store.put_lineage(
            &scope,
            "chunk-span-1",
            engram_domain::CodeLineage {
                repo_id: repo_id.clone(),
                generation_id: "gen-1".to_string(),
                source_path: "src/lib.rs".to_owned(),
                contributors: vec!["doc-1".to_owned()],
                span: Some(engram_domain::CanonicalSpan {
                    start_line: 4,
                    end_line: 9,
                }),
            },
        ))
        .expect("put lineage with span");

        let lineage = block_on(store.target_lineage(&scope)).expect("target lineage");
        let entry = lineage
            .iter()
            .find(|entry| entry.record_id == "chunk-span-1")
            .expect("lineage entry present");
        assert_eq!(entry.source_path, "src/lib.rs");
        assert_eq!(
            entry.span,
            Some(engram_domain::CanonicalSpan {
                start_line: 4,
                end_line: 9
            })
        );
        assert!(!entry.retired);

        let keyed = block_on(store.record_lineage(&scope, "chunk-span-1"))
            .expect("keyed lineage")
            .expect("entry");
        assert_eq!(keyed.source_path, "src/lib.rs");
        assert_eq!(
            keyed.span.map(|span| (span.start_line, span.end_line)),
            Some((4, 9))
        );
    }
}
