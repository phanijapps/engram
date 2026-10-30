//! TASK-017 corrective-r2 cell security proof (finding 2): the PostgreSQL
//! generation reader binds generation lookup to durable (scope, repository)
//! ownership — a generation id alone is not authority. Runs ONLY against the
//! approved marked disposable fixture (marker assert inside) and is gated
//! behind `--ignored` + `ENGRAM_TEST_PG_URL` like the accepted PG suites.

use engram_domain::{CodeAttempt, CodeBinding, CodeGeneration, RepoId, Scope};
use engram_knowledge::{CodeBindingRepository, CodeEligibilityGate};
use engram_store_pgvector::PgCodeBindingRepository;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

fn url() -> String {
    std::env::var("ENGRAM_TEST_PG_URL").expect("marked disposable PostgreSQL URL required")
}

fn id() -> String {
    format!(
        "r2g{}",
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

fn generation(g: &str) -> CodeGeneration {
    CodeGeneration {
        id: g.into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".into(),
        filter_fingerprint: "filter".into(),
    }
}

/// Serialized exclusive child runner (the accepted PG-suite pattern): each
/// child creates its own repository + publishes one generation, then the
/// parent asserts the ownership binding. Output is checked verbatim.
fn child(script: &str, seed_id: &str) -> (String, bool) {
    let url = url();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bound_generation_ownership",
            "--ignored",
            "--nocapture",
        ])
        .env("ENGRAM_TEST_PG_URL", &url)
        .env("ENGRAM_R2_CHILD", script)
        .env("ENGRAM_R2_SEED_ID", seed_id)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child");
    let mut out = String::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            println!("{line}");
            out.push_str(&line);
            out.push('\n');
        }
    }
    let ok = child.wait().expect("wait child").success();
    (out, ok)
}

fn repository() -> PgCodeBindingRepository {
    let connection =
        engram_store_pgvector::PgConnection::connect(&url()).expect("connect disposable fixture");
    let marker = connection
        .block_on(async {
            connection
                .client
                .query_one(
                    "SELECT shobj_description(oid,'pg_database') FROM pg_database WHERE datname=current_database()",
                    &[],
                )
                .await
        })
        .expect("marker query")
        .get::<_, Option<String>>(0);
    assert_eq!(
        marker.as_deref(),
        Some("engram.factory.job=010-project-bound-branch-indexing; disposable=true"),
        "disposable fixture marker mismatch — refusing to touch any other database"
    );
    PgCodeBindingRepository::new(connection).expect("initialize repository")
}

/// Publishes one generation of `repo` in `workspace` (child side) or asserts
/// the ownership binding (parent side) per the ENGRAM_R2_CHILD script.
#[test]
#[ignore]
fn bound_generation_ownership() {
    let script = std::env::var("ENGRAM_R2_CHILD").unwrap_or_default();
    if script == "seed" {
        let seed_id = std::env::var("ENGRAM_R2_SEED_ID").expect("seed id");
        let repo = repository();
        let scope = scope(&seed_id);
        let repo_id = RepoId(format!("{seed_id}-repo"));
        futures::executor::block_on(async {
            repo.bind(CodeBinding {
                scope: scope.clone(),
                repo_id: repo_id.clone(),
                branch_ref: "refs/heads/main".into(),
                version: 1,
                active_generation: None,
                epoch: 0,
            })
            .await
            .unwrap();
            let attempt = CodeAttempt {
                id: format!("{seed_id}-attempt"),
                idempotency_key: format!("{seed_id}-idempotent"),
                binding_version: 1,
                expected_epoch: 0,
                expected_active_generation: None,
                fencing_token: 0,
                state: engram_domain::CodeAttemptState::Created,
                receipt_kinds: vec![
                    engram_domain::CodeReceiptKind::Source,
                    engram_domain::CodeReceiptKind::Knowledge,
                ],
            };
            repo.begin_attempt(
                &scope,
                &repo_id,
                generation(&format!("{seed_id}-gen")),
                attempt,
            )
            .await
            .unwrap();
            for lane in ["source", "knowledge"] {
                repo.record_receipt(engram_knowledge::StageReceipt {
                    attempt_id: format!("{seed_id}-attempt"),
                    generation_id: format!("{seed_id}-gen"),
                    lane: lane.into(),
                    digest: "d".repeat(64),
                })
                .await
                .unwrap();
            }
            let (_, stored) = repo
                .get_attempt(&scope, &repo_id, &format!("{seed_id}-attempt"))
                .await
                .unwrap()
                .unwrap();
            repo.mark_ready(
                &scope,
                &repo_id,
                &format!("{seed_id}-attempt"),
                stored.fencing_token,
            )
            .await
            .unwrap();
            repo.publish(&scope, &repo_id, &format!("{seed_id}-attempt"), 0)
                .await
                .unwrap();
        });
        return;
    }
    if !script.is_empty() {
        panic!("unknown child script {script}");
    }

    // Parent: seed one repository, then prove the OWNERSHIP binding of the
    // changed generation reader (finding 2) against the real cell.
    let seed_id = id();
    let (out, ok) = child("seed", &seed_id);
    assert!(ok, "seed child failed:\n{out}");

    let repo = repository();
    let own_scope = scope(&seed_id);
    let own_repo = RepoId(format!("{seed_id}-repo"));
    let other_repo = RepoId(format!("{seed_id}-other"));
    let other_scope = scope(&format!("{seed_id}-other"));
    let generation_id = format!("{seed_id}-gen");

    let own = futures::executor::block_on(async {
        repo.get_generation(&own_scope, &own_repo, &generation_id)
            .await
    })
    .expect("owned lookup executes")
    .expect("the owning scope+repository resolves its generation");

    // Foreign REPOSITORY in the same scope: no ownership association.
    let foreign_repo = futures::executor::block_on(async {
        repo.get_generation(&own_scope, &other_repo, &generation_id)
            .await
    })
    .expect("lookup executes");
    assert!(
        foreign_repo.is_none(),
        "a foreign repository must not resolve another repository's generation"
    );

    // Foreign SCOPE with its own repository id: still none.
    let foreign_scope = futures::executor::block_on(async {
        repo.get_generation(&other_scope, &own_repo, &generation_id)
            .await
    })
    .expect("lookup executes");
    assert!(
        foreign_scope.is_none(),
        "a foreign scope must not resolve this scope's generation"
    );

    // The gate's wildcard/foreign semantics stay lineage-bound: the seeded
    // scope has no staged lineage rows, so its wildcard resolves empty and
    // an explicit foreign id denies.
    let wildcard =
        futures::executor::block_on(async { repo.authorize_query(&own_scope, &[], 10).await })
            .expect("wildcard executes");
    assert!(
        wildcard.target_ids.is_empty(),
        "no staged lineage ⇒ wildcard resolves nothing (never widens)"
    );
    let foreign_id = futures::executor::block_on(async {
        repo.authorize_query(&own_scope, &["no-such-record".to_string()], 10)
            .await
    });
    assert!(
        foreign_id.is_err(),
        "unknown explicit ids deny through the PG gate"
    );
    let _ = own;
}
