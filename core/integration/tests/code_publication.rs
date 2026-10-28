use async_trait::async_trait;
use engram_domain::*;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator, CodeGenerationStages,
    CodeRebuildRequest,
};
use engram_knowledge::{CodeBindingRepository, CoreError, CoreResult, StageReceipt};
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn scope() -> Scope {
    Scope {
        tenant: "publication-fixture".into(),
        subject: None,
        workspace: Some("project".into()),
        session: None,
        environment: None,
    }
}
fn repo() -> RepoId {
    RepoId("repository".into())
}
fn actor() -> Actor {
    Actor {
        id: Id::from("fixture"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}
struct Authorizer;
#[async_trait]
impl CodeBindingAuthorizer for Authorizer {
    async fn authorize(
        &self,
        actor: &Actor,
        s: &Scope,
        r: &RepoId,
        _: CodeAuthorityAction,
    ) -> CoreResult<()> {
        if s != &scope() || r != &repo() || actor.id != Id::from("fixture") {
            return Err(CoreError::PolicyDenied {
                reason: "fixture authorization denied".into(),
            });
        }
        Ok(())
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
fn request(id: &str) -> CodeRebuildRequest {
    CodeRebuildRequest {
        scope: scope(),
        repo_id: repo(),
        generation: generation(id),
        attempt_id: format!("attempt-{id}"),
        idempotency_key: format!("request-{id}"),
    }
}
fn coordinator(store: Arc<dyn CodeBindingRepository>) -> CodeBindingCoordinator {
    CodeBindingCoordinator::new(
        store,
        Arc::new(Authorizer),
        vec![
            CodeReceiptKind::Vector,
            CodeReceiptKind::Lexical,
            CodeReceiptKind::Derived,
        ],
    )
}
// These port doubles model durable external-stage completion and failure only.
// Real knowledge/source writes are proven in ingest suites; vector/derived behavior belongs to the later real matrix.
struct Stages {
    root: PathBuf,
    fail: Option<CodeReceiptKind>,
    barrier: bool,
    calls: AtomicUsize,
}
impl Stages {
    fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            fail: None,
            barrier: false,
            calls: AtomicUsize::new(0),
        }
    }
}
fn barrier() {
    use std::io::{Read, Write};
    println!("READY");
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
}
#[async_trait]
impl CodeGenerationStages for Stages {
    async fn stage(
        &self,
        lane: &CodeReceiptKind,
        _: &CodeBinding,
        generation: &CodeGeneration,
        attempt: &CodeAttempt,
    ) -> CoreResult<StageReceipt> {
        use std::io::Write;
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.as_ref() == Some(lane) {
            return Err(CoreError::Adapter {
                adapter: "fixture-stage".into(),
                message: "injected stage failure".into(),
            });
        }
        let name = serde_json::to_value(lane)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let path = self.root.join(format!("{}-{name}", attempt.id));
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(generation.id.as_bytes()).unwrap();
        file.sync_all().unwrap();
        if self.barrier && lane == &CodeReceiptKind::Source {
            barrier();
        }
        Ok(StageReceipt {
            attempt_id: attempt.id.clone(),
            generation_id: generation.id.clone(),
            lane: name,
            digest: "c".repeat(64),
        })
    }
}
fn setup(path: &Path) -> (Arc<SqlCodeBindingRepository>, CodeBindingCoordinator) {
    let store = Arc::new(SqlCodeBindingRepository::open(path).unwrap());
    let coordinator = coordinator(store.clone());
    block_on(coordinator.bind(&actor(), &scope(), &repo(), "refs/heads/main")).unwrap();
    (store, coordinator)
}

mod task_011 {
    use super::*;
    #[test]
    fn same_bind_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let (store, c) = setup(&d.path().join("authority.db"));
        let stages = Stages::new(d.path());
        let published = block_on(c.rebuild(&actor(), request("old"), &stages)).unwrap();
        assert_eq!(
            block_on(c.bind(&actor(), &scope(), &repo(), "refs/heads/main")).unwrap(),
            published
        );
        assert!(
            matches!(block_on(c.bind(&actor(), &scope(), &repo(), "refs/heads/other")), Err(CoreError::Conflict { reason }) if reason == "BindingConflict")
        );
        assert_eq!(
            block_on(store.get_binding(&scope(), &repo()))
                .unwrap()
                .unwrap(),
            published
        );
    }
    #[test]
    fn failure_each_stage() {
        for lane in [
            CodeReceiptKind::Source,
            CodeReceiptKind::Knowledge,
            CodeReceiptKind::Vector,
            CodeReceiptKind::Lexical,
            CodeReceiptKind::Derived,
        ] {
            let d = tempfile::tempdir().unwrap();
            let db = d.path().join("authority.db");
            let (store, c) = setup(&db);
            let old =
                block_on(c.rebuild(&actor(), request("old"), &Stages::new(d.path()))).unwrap();
            let mut failing = Stages::new(d.path());
            failing.fail = Some(lane);
            assert!(
                matches!(block_on(c.rebuild(&actor(), request("next"), &failing)), Err(CoreError::Adapter { adapter, .. }) if adapter == "fixture-stage")
            );
            assert_eq!(
                block_on(store.get_binding(&scope(), &repo()))
                    .unwrap()
                    .unwrap(),
                old
            );
            drop(c);
            drop(store);
            let (store, c) = setup(&db);
            let (_, attempt) = block_on(store.get_attempt(&scope(), &repo(), "attempt-next"))
                .unwrap()
                .unwrap();
            assert_eq!(attempt.state, CodeAttemptState::Staging);
            let next =
                block_on(c.rebuild(&actor(), request("next"), &Stages::new(d.path()))).unwrap();
            assert_eq!(next.active_generation.as_deref(), Some("next"));
            assert_eq!(next.epoch, old.epoch + 1);
        }
    }
    #[test]
    fn fenced_loser() {
        let d = tempfile::tempdir().unwrap();
        let (store, _) = setup(&d.path().join("authority.db"));
        let attempt = |id: &str| CodeAttempt {
            id: id.into(),
            idempotency_key: id.into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 0,
            state: CodeAttemptState::Staging,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        };
        let old =
            block_on(store.begin_attempt(&scope(), &repo(), generation("old"), attempt("old")))
                .unwrap();
        let new =
            block_on(store.begin_attempt(&scope(), &repo(), generation("new"), attempt("new")))
                .unwrap();
        assert!(new.fencing_token > old.fencing_token);
        for (a, g) in [(&old, "old"), (&new, "new")] {
            block_on(store.record_receipt(StageReceipt {
                attempt_id: a.id.clone(),
                generation_id: g.into(),
                lane: "knowledge".into(),
                digest: "c".repeat(64),
            }))
            .unwrap();
        }
        assert!(
            matches!(block_on(store.mark_ready(&scope(), &repo(), &old.id, old.fencing_token)), Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
        );
        block_on(store.mark_ready(&scope(), &repo(), &new.id, new.fencing_token)).unwrap();
        assert!(
            matches!(block_on(store.publish(&scope(), &repo(), &old.id, 0)), Err(CoreError::Conflict { reason }) if reason == "GenerationChanged")
        );
        let result = block_on(store.publish(&scope(), &repo(), &new.id, 0)).unwrap();
        assert_eq!(result.active_generation.as_deref(), Some("new"));
    }
    fn crash_case(name: &str, before_receipt: bool) {
        if let Ok(db) = std::env::var("CODE_PUBLICATION_CHILD_DB") {
            let root = PathBuf::from(std::env::var("CODE_PUBLICATION_CHILD_ROOT").unwrap());
            let (_, c) = setup(Path::new(&db));
            let mut stages = Stages::new(&root);
            stages.barrier = before_receipt;
            block_on(c.rebuild(&actor(), request("next"), &stages)).unwrap();
            if !before_receipt {
                barrier();
            }
            return;
        }
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("authority.db");
        let (store, c) = setup(&db);
        let old = block_on(c.rebuild(&actor(), request("old"), &Stages::new(d.path()))).unwrap();
        drop(c);
        drop(store);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("CODE_PUBLICATION_CHILD_DB", &db)
            .env("CODE_PUBLICATION_CHILD_ROOT", d.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "child exited before durable barrier"
            );
            if line.contains("READY") {
                break;
            }
        }
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        let (store, c) = setup(&db);
        let after_kill = block_on(store.get_binding(&scope(), &repo()))
            .unwrap()
            .unwrap();
        assert_eq!(
            after_kill.epoch,
            if before_receipt {
                old.epoch
            } else {
                old.epoch + 1
            }
        );
        assert!(
            d.path().join("attempt-next-source").exists(),
            "external write barrier not reached"
        );
        let stages = Stages::new(d.path());
        let next = block_on(c.rebuild(&actor(), request("next"), &stages)).unwrap();
        assert_eq!(next.active_generation.as_deref(), Some("next"));
        assert_eq!(next.epoch, old.epoch + 1);
        if !before_receipt {
            assert_eq!(
                stages.calls.load(Ordering::SeqCst),
                0,
                "post-CAS retry repeated external work"
            );
        }
    }
    #[test]
    fn crash_before_receipt() {
        crash_case("task_011::crash_before_receipt", true);
    }
    #[test]
    fn crash_after_cas() {
        crash_case("task_011::crash_after_cas", false);
    }
    #[test]
    fn idempotent_resume() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("authority.db");
        let (_, c) = setup(&db);
        let first = block_on(c.rebuild(&actor(), request("next"), &Stages::new(d.path()))).unwrap();
        drop(c);
        let (_, c) = setup(&db);
        let stages = Stages::new(d.path());
        assert_eq!(
            block_on(c.rebuild(&actor(), request("next"), &stages)).unwrap(),
            first
        );
        assert_eq!(stages.calls.load(Ordering::SeqCst), 0);
        let mut substituted = request("next");
        substituted.generation.commit_oid = "d".repeat(40);
        assert!(matches!(
            block_on(c.rebuild(&actor(), substituted, &stages)),
            Err(CoreError::InvalidRequest { .. })
        ));
    }
    #[test]
    fn authorization_precedes_mutation() {
        let d = tempfile::tempdir().unwrap();
        let (store, c) = setup(&d.path().join("authority.db"));
        let before = block_on(store.get_binding(&scope(), &repo()))
            .unwrap()
            .unwrap();
        let mut denied = actor();
        denied.id = Id::from("denied");
        let stages = Stages::new(d.path());
        assert!(matches!(
            block_on(c.rebuild(&denied, request("next"), &stages)),
            Err(CoreError::PolicyDenied { .. })
        ));
        assert!(
            block_on(store.get_attempt(&scope(), &repo(), "attempt-next"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            block_on(store.get_binding(&scope(), &repo()))
                .unwrap()
                .unwrap(),
            before
        );
        assert_eq!(stages.calls.load(Ordering::SeqCst), 0);
    }
}
