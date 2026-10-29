//! TASK-017 focused proof (corrective r2): exact retained committed bytes and
//! same-OID spans with authorized generation reads, durable-descriptor
//! integrity validation, adapter-owned custody, and honest unavailability
//! errors (REQ-006 / AC-005 / AC-008; findings 2/3/7).
//!
//! Every case drives REAL owned behavior: a REAL `retain_snapshot` write (the
//! immutable acquisition stage), a REAL file-backed `SqlCodeBindingRepository`
//! (bind → receipts → ready → publish at the retained commit, DURABLE source
//! descriptors + lineage), the REAL adapter-owned
//! `RetainedSnapshotReader` (capability-relative custody + digest
//! validation), and the REAL authorized provider chain
//! `code_fetch_exact`/`code_fetch_exact_at`.

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::*;
use engram_ingest::git_source::{
    CommittedFile, ResolvedCommit, RetainedSnapshotReader, retain_snapshot,
};
use engram_ingest::{
    CommittedGenerationFiles, ScanOptions, prepare_generation, stage_generation_records,
};
use engram_integration::EngramProviderBuilder;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator, CodeGenerationStages,
    CodeRebuildRequest, ExactReadCandidate, ParticipantExactSource,
};
use engram_knowledge::{
    CodeBindingRepository, KnowledgeRepository, RetainedSourceReader, StageReceipt,
};
use engram_runtime::CoreError;
use engram_store_sqlite::SqlKnowledgeStore;
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;

// ---------- fixture ---------------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "task-017".to_string(),
        subject: None,
        workspace: Some("bound-source".to_string()),
        session: None,
        environment: None,
    }
}

fn foreign_scope() -> Scope {
    Scope {
        tenant: "task-017".to_string(),
        subject: None,
        workspace: Some("other-workspace".to_string()),
        session: None,
        environment: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("task-017-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

struct DenyAllAuthorizer;
#[async_trait]
impl CodeBindingAuthorizer for DenyAllAuthorizer {
    async fn authorize(
        &self,
        _actor: &Actor,
        _scope: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> engram_runtime::CoreResult<()> {
        Err(CoreError::PolicyDenied {
            reason: "deny-all fixture".into(),
        })
    }
}

struct AllowAuthorizer;
#[async_trait]
impl CodeBindingAuthorizer for AllowAuthorizer {
    async fn authorize(
        &self,
        _actor: &Actor,
        _scope: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> engram_runtime::CoreResult<()> {
        Ok(())
    }
}

fn generation(id: &str, commit_oid: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.to_string(),
        binding_version: 1,
        commit_oid: commit_oid.to_string(),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".to_string(),
        filter_fingerprint: "filter".to_string(),
    }
}

fn attempt(id: &str, expected_epoch: u64, expected_active: Option<&str>) -> CodeAttempt {
    CodeAttempt {
        id: id.to_string(),
        idempotency_key: format!("idempotent-{id}"),
        binding_version: 1,
        expected_epoch,
        expected_active_generation: expected_active.map(str::to_string),
        fencing_token: 0,
        state: CodeAttemptState::Created,
        receipt_kinds: vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge],
    }
}

fn receipt(attempt_id: &str, generation_id: &str, lane: CodeReceiptKind) -> StageReceipt {
    StageReceipt {
        attempt_id: attempt_id.to_string(),
        generation_id: generation_id.to_string(),
        lane: serde_json::to_value(&lane)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        digest: "d".repeat(64),
    }
}

fn publish(
    bindings: &SqlCodeBindingRepository,
    repo: &str,
    generation: &CodeGeneration,
    attempt_id: &str,
    expected_epoch: u64,
    expected_active: Option<&str>,
) {
    let repo_id = RepoId(repo.to_string());
    block_on(bindings.begin_attempt(
        &scope(),
        &repo_id,
        generation.clone(),
        attempt(attempt_id, expected_epoch, expected_active),
    ))
    .expect("begin attempt");
    for lane in [CodeReceiptKind::Source, CodeReceiptKind::Knowledge] {
        block_on(bindings.record_receipt(receipt(attempt_id, &generation.id, lane)))
            .expect("receipt");
    }
    let (_, stored) = block_on(bindings.get_attempt(&scope(), &repo_id, attempt_id))
        .expect("get attempt")
        .expect("attempt exists");
    block_on(bindings.mark_ready(&scope(), &repo_id, attempt_id, stored.fencing_token))
        .expect("mark ready");
    block_on(bindings.publish(&scope(), &repo_id, attempt_id, expected_epoch)).expect("publish");
}

struct SourceFixture {
    provider: engram_integration::EngramProvider,
    retained_root: std::path::PathBuf,
    working_tree: std::path::PathBuf,
    source: Vec<u8>,
    commit_oid: String,
    bindings: Arc<SqlCodeBindingRepository>,
    /// A lease pinned while gen-old was the ACTIVE generation (historical
    /// lease; renewed lifetime). `None` when fixture_with_lease(false).
    historical_lease: Option<engram_knowledge::CodeReadLease>,
}

/// Retains one REAL committed file (CRLF + non-ASCII), binds `repo-a` with
/// TWO published generations (gen-old then gen-new, both at their own
/// retained commits), records DURABLE source descriptors, and mirrors the
/// file into a "working tree" the exact path MUST NOT consult.
fn fixture(dir: &std::path::Path) -> SourceFixture {
    fixture_with(dir, Arc::new(AllowAuthorizer))
}

/// `with_historical_lease`: pins a read lease while gen-old is ACTIVE (before
/// gen-new publishes), so the lease's historical eligibility covers gen-old
/// across the publication — the explicitly authorized pinned historical read.
fn fixture_with_historical_lease(dir: &std::path::Path) -> SourceFixture {
    fixture_inner_with_lease_window(
        dir,
        Arc::new(AllowAuthorizer),
        true,
        std::time::Duration::from_secs(600),
    )
}

/// [`fixture_with_historical_lease`] with an explicit lease lifetime — the
/// expiry-during-read proof pins a SHORT lease.
fn fixture_with_historical_lease_window(
    dir: &std::path::Path,
    lease_window: std::time::Duration,
) -> SourceFixture {
    fixture_inner_with_lease_window(dir, Arc::new(AllowAuthorizer), true, lease_window)
}

fn fixture_with(
    dir: &std::path::Path,
    authorizer: Arc<dyn CodeBindingAuthorizer>,
) -> SourceFixture {
    fixture_inner(dir, authorizer, false)
}

fn fixture_inner(
    dir: &std::path::Path,
    authorizer: Arc<dyn CodeBindingAuthorizer>,
    pin_historical_lease: bool,
) -> SourceFixture {
    fixture_inner_with_lease_window(
        dir,
        authorizer,
        pin_historical_lease,
        std::time::Duration::from_secs(600),
    )
}

fn fixture_inner_with_lease_window(
    dir: &std::path::Path,
    authorizer: Arc<dyn CodeBindingAuthorizer>,
    pin_historical_lease: bool,
    lease_window: std::time::Duration,
) -> SourceFixture {
    let source = "def alpha():\r\n    return 'caf\u{e9}'\r\n\u{00e9}t\u{00e9} marker\r\n"
        .as_bytes()
        .to_vec();
    let old_source = "def old():\r\n    return 0\r\n".as_bytes().to_vec();

    let retained_root = dir.join("retained");
    let new_commit = ResolvedCommit {
        branch_ref: "refs/heads/main".to_string(),
        oid: "c".repeat(40),
        object_format: "sha1".to_string(),
    };
    let old_commit = ResolvedCommit {
        branch_ref: "refs/heads/main".to_string(),
        oid: "e".repeat(40),
        object_format: "sha1".to_string(),
    };
    let new_snapshot = retain_snapshot(
        &retained_root,
        new_commit.clone(),
        &[CommittedFile {
            path: "src/exact.py".into(),
            oid: "d".repeat(40),
            mode: "100644".to_string(),
            bytes: source.clone(),
        }],
    )
    .expect("retain new");
    let old_snapshot = retain_snapshot(
        &retained_root,
        old_commit,
        &[CommittedFile {
            path: "src/exact.py".into(),
            oid: "f".repeat(40),
            mode: "100644".to_string(),
            bytes: old_source.clone(),
        }],
    )
    .expect("retain old");

    let bindings = Arc::new(
        SqlCodeBindingRepository::open(dir.join("bindings.sqlite")).expect("open binding store"),
    );
    let repo = RepoId("repo-a".to_string());
    block_on(bindings.bind(CodeBinding {
        scope: scope(),
        repo_id: repo.clone(),
        branch_ref: "refs/heads/main".to_string(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }))
    .expect("bind");
    // gen-old published first (then superseded), gen-new active.
    publish(
        &bindings,
        "repo-a",
        &generation("gen-old", &old_snapshot.commit.oid),
        "attempt-old",
        0,
        None,
    );
    // Optionally pin the HISTORICAL lease while gen-old is still active.
    let historical_lease = if pin_historical_lease {
        let early_coordinator = CodeBindingCoordinator::new(
            (Arc::clone(&bindings) as Arc<dyn CodeBindingRepository>),
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let lease = block_on(early_coordinator.pin_read(
            &actor(),
            &scope(),
            &RepoId("repo-a".to_string()),
            lease_window,
        ))
        .expect("pin historical lease");
        Some(lease)
    } else {
        None
    };
    publish(
        &bindings,
        "repo-a",
        &generation("gen-new", &new_snapshot.commit.oid),
        "attempt-new",
        1,
        Some("gen-old"),
    );
    // Durable source descriptors for both generations.
    for (generation_id, oid, bytes) in [
        (
            "gen-old",
            old_snapshot.commit.oid.clone(),
            old_source.clone(),
        ),
        ("gen-new", new_snapshot.commit.oid.clone(), source.clone()),
    ] {
        block_on(
            bindings.put_source_descriptor(
                &scope(),
                IndexedSource {
                    repo_id: RepoId("repo-a".to_string()),
                    generation_id: generation_id.to_string(),
                    commit_oid: oid,
                    path: "src/exact.py".to_string(),
                    content_digest: engram_ingest::content_hash(bytes)
                        .trim_start_matches("sha256:")
                        .to_string(),
                },
            ),
        )
        .expect("put source descriptor");
    }
    let coordinator = CodeBindingCoordinator::new(
        bindings.clone() as Arc<dyn CodeBindingRepository>,
        authorizer,
        Vec::new(),
    );
    let reader = Arc::new(RetainedSnapshotReader::new(retained_root.clone()));
    let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
        CapabilityState::Supported,
    ))
    .code_bindings(Arc::new(coordinator))
    .code_source(reader as Arc<dyn RetainedSourceReader>)
    .build();

    let working_tree = dir.join("working");
    std::fs::create_dir_all(working_tree.join("src")).expect("mkdir working tree");
    std::fs::write(working_tree.join("src/exact.py"), source.clone()).expect("write working copy");

    SourceFixture {
        provider,
        retained_root,
        working_tree,
        source,
        commit_oid: new_snapshot.commit.oid.clone(),
        bindings,
        historical_lease,
    }
}

// ---------- task_018 fixture -------------------------------------------------

/// One bound repository for TASK-018: a working-tree directory (shared
/// `basename`) plus its published generation facts. The coordinator NEVER
/// receives any path — durable records are the only authority.
struct BoundRepo {
    repo_id: String,
    working_root: std::path::PathBuf,
    commit_oid: String,
    parser_fingerprint: String,
    filter_fingerprint: String,
}

/// Binds `repos` in ONE scope, publishes one generation per repository
/// (distinct OIDs and fingerprints), and returns the coordinator wired on
/// the REAL SQLite binding cell. Each repo's working root carries the SAME
/// basename so root-basename collisions are structurally present.
struct ObservationFixture {
    coordinator: Arc<CodeBindingCoordinator>,
    bindings: Arc<SqlCodeBindingRepository>,
    repos: Vec<BoundRepo>,
    db_path: std::path::PathBuf,
}

fn observation_fixture(dir: &std::path::Path, repos: &[(&str, char, char)]) -> ObservationFixture {
    let bindings = Arc::new(
        SqlCodeBindingRepository::open(dir.join("bindings-t018.sqlite"))
            .expect("open binding store"),
    );
    let mut bound = Vec::new();
    for (index, (repo_id, oid_char, fingerprint_char)) in repos.iter().enumerate() {
        let repo = RepoId(repo_id.to_string());
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: repo.clone(),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");
        let generation = CodeGeneration {
            id: format!("gen-t018-{index}-{repo_id}"),
            binding_version: 1,
            commit_oid: oid_char.to_string().repeat(40),
            snapshot_digest: format!("{fingerprint_char}").repeat(64),
            parser_fingerprint: format!("parser-{fingerprint_char}"),
            filter_fingerprint: format!("filter-{fingerprint_char}"),
        };
        publish(
            &bindings,
            repo_id,
            &generation,
            &format!("attempt-t018-{index}"),
            0,
            None,
        );
        // Both working roots share the SAME basename (the collision hazard).
        let working_root = dir.join(format!("root-{index}")).join("proj");
        std::fs::create_dir_all(&working_root).expect("mkdir working root");
        std::fs::write(working_root.join("src"), "uncommitted bytes\n")
            .expect("write working copy");
        bound.push(BoundRepo {
            repo_id: repo_id.to_string(),
            working_root,
            commit_oid: generation.commit_oid.clone(),
            parser_fingerprint: generation.parser_fingerprint.clone(),
            filter_fingerprint: generation.filter_fingerprint.clone(),
        });
    }
    let coordinator = Arc::new(CodeBindingCoordinator::new(
        (Arc::clone(&bindings) as Arc<dyn engram_knowledge::CodeBindingRepository>),
        Arc::new(AllowAuthorizer),
        Vec::new(),
    ));
    ObservationFixture {
        coordinator,
        bindings,
        repos: bound,
        db_path: dir.join("bindings-t018.sqlite"),
    }
}

impl ObservationFixture {
    fn repo(&self, id: &str) -> &BoundRepo {
        self.repos
            .iter()
            .find(|repo| repo.repo_id == id)
            .unwrap_or_else(|| panic!("unknown fixture repo {id}"))
    }

    fn status(&self, repo_id: &str) -> engram_integration::code_binding::CodeBindingStatus {
        block_on(
            self.coordinator
                .status(&actor(), &scope(), &RepoId(repo_id.to_string()), None),
        )
        .expect("status")
    }
}

// ---------- task_017 focused cases -----------------------------------------

mod task_017 {
    use super::*;

    /// exact_bytes_and_spans: byte-exact committed content (CRLF + UTF-8
    /// preserved) with full/served digests and same-OID byte spans; bounds
    /// are enforced.
    #[cfg(unix)]
    #[test]
    fn exact_bytes_and_spans() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let view = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read");
        assert_eq!(view.bytes, fixture.source, "byte-exact committed content");
        assert_eq!(view.commit_oid, fixture.commit_oid);
        let expected_digest = engram_ingest::content_hash(&fixture.source);
        assert_eq!(
            view.full_content_digest,
            expected_digest.trim_start_matches("sha256:")
        );
        assert_eq!(
            view.served_digest, view.full_content_digest,
            "whole-file: served == full"
        );

        let first_crlf = fixture
            .source
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("first CRLF");
        let span_start = first_crlf + 2;
        let span_end = fixture.source.len() - "\r\n".len();
        let spanned = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            Some(span_start..span_end),
        ))
        .expect("span read");
        assert_eq!(
            spanned.bytes,
            fixture.source[span_start..span_end],
            "spans are byte offsets into the SAME retained blob"
        );
        assert_eq!(spanned.span, Some((span_start, span_end)));
        assert_ne!(spanned.served_digest, spanned.full_content_digest);

        let beyond = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            Some(0..fixture.source.len() + 1),
        ));
        assert!(beyond.is_err(), "out-of-bounds spans must reject");
    }

    /// checkout_deleted: with the working tree GONE, the exact read still
    /// returns the retained bytes — the answer never depended on a checkout.
    #[test]
    fn checkout_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        std::fs::remove_dir_all(&fixture.working_tree).expect("delete the whole working tree");

        let view = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read after checkout deletion");
        assert_eq!(view.bytes, fixture.source);
        assert_eq!(view.commit_oid, fixture.commit_oid);
    }

    /// Task017 r3: a lease RELEASED DURING the read denies the answer — the
    /// reader port double releases the REAL durable lease mid-read (before
    /// the bytes return), and the post-read revalidation must deny. The
    /// release goes through the real lease store; only the read I/O is at
    /// the port boundary.
    #[test]
    fn lease_release_during_read_denies() {
        use engram_knowledge::RetainedSourceReader;

        struct ReleasingReader {
            inner: Arc<RetainedSnapshotReader>,
            lease: engram_knowledge::CodeReadLease,
            released: std::sync::atomic::AtomicBool,
        }

        #[async_trait::async_trait]
        impl RetainedSourceReader for ReleasingReader {
            fn read_span(
                &self,
                commit_oid: &str,
                path: &str,
                span: Option<std::ops::Range<usize>>,
                expected_digest: &str,
            ) -> engram_knowledge::CoreResult<engram_knowledge::ExactSourceView> {
                let view = self
                    .inner
                    .read_span(commit_oid, path, span, expected_digest)?;
                // Release the durable lease DURING the read (after the bytes
                // were read, before they are returned).
                if !self
                    .released
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    let coordinator = CodeBindingCoordinator::new(
                        (self.lease_store()) as Arc<dyn CodeBindingRepository>,
                        Arc::new(AllowAuthorizer),
                        Vec::new(),
                    );
                    let lease = self.lease.clone();
                    futures::executor::block_on(coordinator.release_read(
                        &actor(),
                        &RepoId("repo-a".to_string()),
                        &lease,
                    ))
                    .expect("release during read");
                }
                Ok(view)
            }
        }

        // Extend the double with the store handle it needs.
        struct Releasing {
            inner: Arc<RetainedSnapshotReader>,
            lease: engram_knowledge::CodeReadLease,
            store: Arc<SqlCodeBindingRepository>,
            released: std::sync::atomic::AtomicBool,
        }
        #[async_trait::async_trait]
        impl RetainedSourceReader for Releasing {
            fn read_span(
                &self,
                commit_oid: &str,
                path: &str,
                span: Option<std::ops::Range<usize>>,
                expected_digest: &str,
            ) -> engram_knowledge::CoreResult<engram_knowledge::ExactSourceView> {
                let view = self
                    .inner
                    .read_span(commit_oid, path, span, expected_digest)?;
                if !self
                    .released
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    // Release on a DEDICATED thread: read_span is sync and
                    // already runs inside the recall executor, so a nested
                    // block_on would re-enter the LocalPool.
                    let store = self.store.clone();
                    let lease = self.lease.clone();
                    std::thread::spawn(move || {
                        let coordinator = CodeBindingCoordinator::new(
                            store as Arc<dyn CodeBindingRepository>,
                            Arc::new(AllowAuthorizer),
                            Vec::new(),
                        );
                        futures::executor::block_on(coordinator.release_read(
                            &actor(),
                            &RepoId("repo-a".to_string()),
                            &lease,
                        ))
                    })
                    .join()
                    .expect("release thread")
                    .expect("release during read");
                }
                Ok(view)
            }
        }
        impl ReleasingReader {
            fn lease_store(&self) -> Arc<SqlCodeBindingRepository> {
                unreachable!("constructor placeholder — the real double is `Releasing`")
            }
        }
        let _ = ReleasingReader {
            inner: Arc::new(RetainedSnapshotReader::new(std::path::PathBuf::from(
                "/dev/null",
            ))),
            lease: engram_knowledge::CodeReadLease {
                id: String::new(),
                eligibility: engram_knowledge::CodeEligibility {
                    scope: scope(),
                    epoch: 0,
                    generations: Vec::new(),
                    repository_ids: Vec::new(),
                },
                fencing_token: 0,
                expires_at: chrono::Utc::now(),
            },
            released: std::sync::atomic::AtomicBool::new(false),
        };

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture_with_historical_lease(dir.path());
        let lease = fixture.historical_lease.clone().expect("historical lease");
        let releasing = Arc::new(Releasing {
            inner: Arc::new(RetainedSnapshotReader::new(fixture.retained_root.clone())),
            lease: lease.clone(),
            store: fixture.bindings.clone(),
            released: std::sync::atomic::AtomicBool::new(false),
        });
        let coordinator = CodeBindingCoordinator::new(
            fixture.bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .code_bindings(Arc::new(coordinator))
        .code_source(releasing as Arc<dyn RetainedSourceReader>)
        .build();

        let during = block_on(provider.code_fetch_exact_at_with_lease(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            Some(&lease),
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&during, Err(CoreError::InvalidRequest { reason }) if reason.contains("no authorized read lease")),
            "a lease released during the read must deny the answer: {during:?}"
        );
    }

    /// Task017 r3: a lease EXPIRING DURING the read denies the answer. The
    /// double presents a short-lived lease (20ms) and delays the read I/O
    /// return past expiry (50ms) — bounded margins, no unbounded waits.
    #[test]
    fn lease_expiry_during_read_denies() {
        use engram_knowledge::RetainedSourceReader;

        struct DelayingReader {
            inner: Arc<RetainedSnapshotReader>,
        }
        #[async_trait::async_trait]
        impl RetainedSourceReader for DelayingReader {
            fn read_span(
                &self,
                commit_oid: &str,
                path: &str,
                span: Option<std::ops::Range<usize>>,
                expected_digest: &str,
            ) -> engram_knowledge::CoreResult<engram_knowledge::ExactSourceView> {
                let view = self
                    .inner
                    .read_span(commit_oid, path, span, expected_digest)?;
                std::thread::sleep(std::time::Duration::from_millis(500));
                Ok(view)
            }
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture =
            fixture_with_historical_lease_window(dir.path(), std::time::Duration::from_millis(250));
        // A short-lived durable lease (250ms) pinned while gen-old was
        // active; the reader delays 500ms past the (fast) start validation,
        // so the lease EXPIRES DURING the read I/O. Renewal cannot shorten a
        // lease and acquisition pins only the current view, so the short
        // window is pinned at the fixture's authorized moment.
        let lease = fixture.historical_lease.clone().expect("historical lease");
        assert!(
            lease.expires_at > chrono::Utc::now(),
            "the lease must still be live when the read starts"
        );
        let short = lease;
        let delaying = Arc::new(DelayingReader {
            inner: Arc::new(RetainedSnapshotReader::new(fixture.retained_root.clone())),
        });
        let coordinator = CodeBindingCoordinator::new(
            fixture.bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .code_bindings(Arc::new(coordinator))
        .code_source(delaying as Arc<dyn RetainedSourceReader>)
        .build();

        let during = block_on(provider.code_fetch_exact_at_with_lease(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            Some(&short),
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&during, Err(CoreError::InvalidRequest { reason }) if reason.contains("no authorized read lease")),
            "a lease expired during the read must deny the answer: {during:?}"
        );
    }

    /// Task017 r3 caller isolation: while caller A's historical lease is
    /// LIVE, caller B's generation-ID-only request (no presented lease) still
    /// denies — A's live lease never becomes B's authority.
    #[test]
    fn caller_isolation_live_lease_never_authorizes_generation_id_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture_with_historical_lease(dir.path());
        // A's lease is live and covers gen-old.
        let lease = fixture.historical_lease.clone().expect("historical lease");
        assert!(lease.expires_at > chrono::Utc::now());
        // B presents only the generation id.
        let other = engram_domain::Actor {
            id: engram_domain::Id::from("other-caller"),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        };
        let denied = block_on(fixture.provider.code_fetch_exact_at(
            &other,
            &scope(),
            "repo-a",
            "gen-old",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&denied, Err(CoreError::InvalidRequest { reason }) if reason.contains("no authorized read lease")),
            "another caller's live lease is not generation-id authority: {denied:?}"
        );
        // A (the lease holder) still succeeds with the presented lease.
        let allowed = block_on(fixture.provider.code_fetch_exact_at_with_lease(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            Some(&lease),
            "src/exact.py",
            None,
        ))
        .expect("the lease holder's presented lease still authorizes");
        assert_eq!(allowed.bytes, b"def old():\r\n    return 0\r\n".to_vec());
    }

    /// source_unavailable + corrective r2 findings 2/3/7: foreign-scope and
    /// staged generations deny; a RETIRED generation denies without a live
    /// lease; unbound repositories deny; a DENYING authorizer denies even
    /// with everything published; missing descriptors, tampered bytes, leaf
    /// and ancestor substitution deny with SANITIZED errors; an on-disk
    /// decoy is never served.
    #[test]
    fn source_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        // (a) The superseded gen-old is RETIRED: denied without a lease even
        // though its snapshot and descriptor still exist (finding 2). The
        // deletion-protection set (`retained_generations`) is NOT read
        // authority — only a live lease authorizes the historical read.
        let retired = block_on(fixture.provider.code_fetch_exact_at(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&retired, Err(CoreError::InvalidRequest { reason }) if reason.contains("no authorized read lease")),
            "a retired generation denies without an authorized lease: {retired:?}"
        );

        // (b) A LIVE HISTORICAL LEASE authorizes the historical read: a
        // lease pinned while gen-old was active (and still unexpired)
        // covers gen-old across the publication. Fresh tempdir: retention
        // rejects duplicate commits in one storage root.
        let lease_dir = tempfile::tempdir().expect("lease tempdir");
        let leased_fixture = fixture_with_historical_lease(lease_dir.path());
        let presented_lease = leased_fixture.historical_lease.clone().expect("lease");
        let leased = block_on(leased_fixture.provider.code_fetch_exact_at_with_lease(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            Some(&presented_lease),
            "src/exact.py",
            None,
        ))
        .expect("an authorized live historical lease covers the generation");
        assert_eq!(leased.bytes, b"def old():\r\n    return 0\r\n".to_vec());
        // Releasing the lease removes the authorization again.
        let coordinator = leased_fixture
            .provider
            .code_bindings()
            .expect("coordinator");
        let lease = presented_lease;
        block_on(coordinator.release_read(&actor(), &RepoId("repo-a".to_string()), &lease))
            .expect("release lease");
        let released = block_on(leased_fixture.provider.code_fetch_exact_at_with_lease(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
            Some(&lease),
            "src/exact.py",
            None,
        ));
        assert!(
            released.is_err(),
            "a released lease no longer authorizes history"
        );

        // (c) Foreign scope: the same generation id does not authorize in a
        // different scope (finding 2).
        let foreign = block_on(fixture.provider.code_fetch_exact_at(
            &actor(),
            &foreign_scope(),
            "repo-a",
            "gen-new",
            "src/exact.py",
            None,
        ));
        assert!(foreign.is_err(), "foreign scopes never authorize");

        // (d) Foreign repository: an unbound repo denies before any consult.
        let unbound = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-unbound",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&unbound, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "unbound repositories are denied: {unbound:?}"
        );

        // (e) A staged-only generation (never published, never leased) denies.
        block_on(fixture.bindings.begin_attempt(
            &scope(),
            &RepoId("repo-a".to_string()),
            generation("gen-staged", &"a".repeat(40)),
            attempt("attempt-staged", 2, Some("gen-new")),
        ))
        .expect("begin staged attempt");
        let staged = block_on(fixture.provider.code_fetch_exact_at(
            &actor(),
            &scope(),
            "repo-a",
            "gen-staged",
            "src/exact.py",
            None,
        ));
        assert!(staged.is_err(), "staged generations deny");

        // (f) A path with no durable descriptor denies.
        let missing = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/not-retained.py",
            None,
        ));
        assert!(
            matches!(&missing, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "missing retained path surfaces the typed error: {missing:?}"
        );

        // (g) On-disk DECOY with different bytes is never served: plant an
        // impostor in the working tree; the retained blob still answers.
        std::fs::write(fixture.working_tree.join("src/exact.py"), "IMPOSTER BYTES")
            .expect("plant impostor");
        let view = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("retained snapshot still answers");
        assert_eq!(view.bytes, fixture.source, "the retained blob wins");

        // (h) TAMPERED retained bytes deny: alter the blob in place so the
        // durable descriptor digest no longer matches (finding 7). Retained
        // leaves are published read-only; the tamper first takes write
        // permission (an attacker with storage write access), which custody
        // does not prevent — DIGEST VERIFICATION catches it.
        let blob = fixture
            .retained_root
            .join(&fixture.commit_oid)
            .join("src")
            .join("exact.py");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o600))
                .expect("take write permission");
        }
        std::fs::write(&blob, b"TAMPERED BYTES").expect("tamper blob");
        let tampered = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&tampered, Err(CoreError::InvalidRequest { reason }) if reason.contains("does not match the durable indexed-source digest")),
            "tampered bytes must never serve as exact: {tampered:?}"
        );
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o600))
                .expect("write permission for restore");
        }
        std::fs::write(&blob, &fixture.source).expect("restore blob");

        // (i) LEAF SUBSTITUTION with a symlink denies sanitized (finding 7).
        let impostor = fixture.retained_root.join("../impostor.py");
        std::fs::write(&impostor, "IMPOSTER").expect("write impostor");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o600))
                .expect("write permission for substitution");
        }
        std::fs::remove_file(&blob).expect("remove leaf");
        std::os::unix::fs::symlink(&impostor, &blob).expect("symlink leaf");
        let substituted = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&substituted, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "symlink substitution must deny sanitized: {substituted:?}"
        );

        // (j) ANCESTOR SUBSTITUTION with a symlink denies sanitized.
        std::fs::remove_file(&blob).expect("remove link");
        std::fs::write(&blob, &fixture.source).expect("restore leaf");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o400))
                .expect("restore readonly");
        }
        let ancestor_dir = fixture.retained_root.join(&fixture.commit_oid).join("src");
        let outside = fixture.retained_root.join("../outside-src");
        std::fs::create_dir_all(&outside).expect("mkdir outside");
        std::fs::write(outside.join("exact.py"), &fixture.source).expect("write outside copy");
        std::fs::remove_dir_all(&ancestor_dir).expect("remove ancestor dir");
        std::os::unix::fs::symlink(&outside, &ancestor_dir).expect("symlink ancestor");
        let ancestor_sub = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&ancestor_sub, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "ancestor substitution must deny sanitized: {ancestor_sub:?}"
        );

        // (k) A DENYING authorizer denies even with everything published
        // (finding 3) — fresh fixture, deny-all coordinator.
        let deny_dir = tempfile::tempdir().expect("deny tempdir");
        let deny_fixture = fixture_with(deny_dir.path(), Arc::new(DenyAllAuthorizer));
        let denied = block_on(deny_fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&denied, Err(CoreError::PolicyDenied { reason }) if reason.contains("deny-all")),
            "a denying caller is denied before any byte is opened: {denied:?}"
        );

        // (l) Missing storage denies sanitized (no filesystem error chains).
        std::fs::remove_dir_all(&fixture.retained_root).expect("remove retained root");
        let gone = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&gone, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "no disk fallback after snapshot loss: {gone:?}"
        );
    }

    /// chunk_mode_not_exact: chunk text is a retrieval reconstruction, NEVER
    /// the exact source; the provider's exact view carries the revision
    /// identity (commit OID + digest) that reconstruction answers cannot
    /// claim (the MCP-side labels are proven in engram-mcp's fetch_content
    /// selection).
    #[test]
    fn chunk_mode_not_exact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let exact = block_on(fixture.provider.code_fetch_exact(
            &actor(),
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read");

        let signature_only: Vec<u8> = fixture
            .source
            .split(|byte| *byte == b'\n')
            .next()
            .map(<[u8]>::to_vec)
            .expect("first line");
        assert!(
            exact.bytes.len() > signature_only.len() && exact.bytes.starts_with(&signature_only),
            "the exact view is the full committed file, not a chunk reconstruction"
        );
        assert_eq!(exact.commit_oid, fixture.commit_oid);
        assert_eq!(exact.full_content_digest.len(), 64);
        assert!(
            exact.bytes.contains(&b'\r'),
            "exact bytes preserve CRLF; chunk stitching would not guarantee it"
        );
    }

    // ---------- pre-gate obligation 2: durable scan → stage → publish → fetch --

    /// Fixture texts (ground truth written to disk, then scanned back):
    /// v1 of `src/service.py` carries `alpha` (1–2) and `legacy` (4–5) over
    /// CRLF lines with non-ASCII; v2 moves `alpha` to 6–7, adds `beta` at
    /// 3–4 plus a leading import (the real chunker stages a File chunk at
    /// 1–1), and REMOVES `legacy`. `src/util.py` (LF) is content-identical
    /// across both generations — its chunk ids still differ per generation
    /// because staging namespaces ids by generation.
    const E2E_SERVICE_V1: &str =
        "def alpha():\r\n    return 'caf\u{e9}'\r\n\r\ndef legacy():\r\n    return 0\r\n";
    const E2E_SERVICE_V2: &str = "import os\r\n\r\ndef beta():\r\n    return 2\r\n\r\ndef alpha():\r\n    return 'nuovo'\r\n";
    const E2E_UTIL: &str = "def helper():\n    return 1\n";

    /// REAL scan of the fixture working tree: every file read from disk,
    /// deterministically ordered by path, with content-derived blob ids —
    /// the same inventory shape `read_resolved_commit` produces for a real
    /// git tree. The scanned bytes (never in-test literals) feed every
    /// later stage.
    fn scan_checkout(dir: &std::path::Path) -> Vec<CommittedFile> {
        let mut scanned = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let mut entries: Vec<_> = std::fs::read_dir(&current)
                .expect("read fixture directory")
                .collect();
            entries.sort_by_key(|entry| entry.as_ref().expect("fixture entry").file_name());
            for entry in entries
                .into_iter()
                .map(|entry| entry.expect("fixture entry"))
            {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let bytes = std::fs::read(&path).expect("read scanned fixture file");
                let digest = engram_ingest::content_hash(&bytes)
                    .trim_start_matches("sha256:")
                    .to_string();
                scanned.push((
                    path.strip_prefix(dir)
                        .expect("fixture-relative path")
                        .to_path_buf(),
                    digest[..40].to_string(),
                    bytes,
                ));
            }
        }
        scanned.sort_by(|a, b| a.0.cmp(&b.0));
        scanned
            .into_iter()
            .map(|(path, oid, bytes)| CommittedFile {
                path,
                oid,
                mode: "100644".to_string(),
                bytes,
            })
            .collect()
    }

    fn e2e_scan_options() -> ScanOptions {
        ScanOptions {
            scope: scope(),
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                allowed_uses: vec![AllowedUse::Retrieval],
                sensitivity: None,
                expires_at: None,
                delete_mode: None,
            },
            actor: actor(),
            source_name: "task-017-e2e-scan".to_string(),
            max_bytes: 0,
            manifest: std::collections::HashMap::new(),
            scan_filter: Default::default(),
        }
    }

    /// The REAL lane implementations of one staged publication: the Source
    /// lane receipts the digest of the ACTUALLY RETAINED snapshot; the
    /// Knowledge lane performs `stage_generation_records` — the real durable
    /// knowledge writes plus the real lineage/source staging and the real
    /// manifest-derived receipt digest.
    struct E2eStages {
        manifest_root: std::path::PathBuf,
        snapshot_digest: String,
        prepared: engram_ingest::PreparedGeneration,
        knowledge: Arc<SqlKnowledgeStore>,
    }

    #[async_trait]
    impl CodeGenerationStages for E2eStages {
        async fn stage(
            &self,
            lane: &CodeReceiptKind,
            binding: &CodeBinding,
            generation: &CodeGeneration,
            attempt: &CodeAttempt,
        ) -> engram_runtime::CoreResult<StageReceipt> {
            let digest = match lane {
                CodeReceiptKind::Source => self.snapshot_digest.clone(),
                CodeReceiptKind::Knowledge => {
                    stage_generation_records(
                        &self.manifest_root,
                        binding,
                        &self.prepared,
                        &attempt.id,
                        self.knowledge.as_ref(),
                    )
                    .await?
                    .digest
                }
                other => {
                    return Err(CoreError::InvalidRequest {
                        reason: format!("e2e declares no {other:?} lane"),
                    });
                }
            };
            let lane_name = serde_json::to_value(lane)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            Ok(StageReceipt {
                attempt_id: attempt.id.clone(),
                generation_id: generation.id.clone(),
                lane: lane_name,
                digest,
            })
        }
    }

    /// One REAL composition pass: scan the fixture checkout from disk →
    /// retain the immutable snapshot → prepare the generation over the
    /// scanned bytes (the real ingest/staging path) → publish through the
    /// coordinator's real begin → receipts → ready → publish sequence.
    /// Every digest on the way is real (snapshot digest, manifest-derived
    /// knowledge receipt); nothing is hand-stamped.
    fn e2e_publish(
        checkout: &std::path::Path,
        retained_root: &std::path::Path,
        manifest_root: &std::path::Path,
        bindings: &Arc<SqlCodeBindingRepository>,
        knowledge: &Arc<SqlKnowledgeStore>,
        generation_id: &str,
        commit_oid: &str,
        attempt_id: &str,
    ) -> (engram_ingest::PreparedGeneration, CodeBinding) {
        let files = scan_checkout(checkout);
        assert!(
            !files.is_empty(),
            "the fixture checkout must scan non-empty"
        );
        let commit = ResolvedCommit {
            branch_ref: "refs/heads/main".to_string(),
            oid: commit_oid.to_string(),
            object_format: "sha1".to_string(),
        };
        let snapshot =
            retain_snapshot(retained_root, commit.clone(), &files).expect("retain snapshot");
        let repo = RepoId("repo-a".to_string());
        let binding = block_on(bindings.get_binding(&scope(), &repo))
            .expect("read binding")
            .expect("repo-a is durably bound");
        let generation = CodeGeneration {
            id: generation_id.to_string(),
            binding_version: binding.version,
            commit_oid: commit.oid.clone(),
            snapshot_digest: snapshot.digest.clone(),
            parser_fingerprint: "e2e-parser".to_string(),
            filter_fingerprint: "e2e-filter".to_string(),
        };
        let prepared = prepare_generation(
            &binding,
            &generation,
            &CommittedGenerationFiles {
                commit: &commit,
                files: &files,
            },
            &e2e_scan_options(),
        )
        .expect("prepare generation over scanned bytes");
        let stages = E2eStages {
            manifest_root: manifest_root.to_path_buf(),
            snapshot_digest: snapshot.digest.clone(),
            prepared,
            knowledge: knowledge.clone(),
        };
        let coordinator = CodeBindingCoordinator::new(
            bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let published = block_on(coordinator.rebuild(
            &actor(),
            CodeRebuildRequest {
                scope: scope(),
                repo_id: RepoId("repo-a".to_string()),
                generation,
                attempt_id: attempt_id.to_string(),
                idempotency_key: format!("idempotent-{attempt_id}"),
            },
            &stages,
        ))
        .expect("real staged publication");
        let E2eStages { prepared, .. } = stages;
        (prepared, published)
    }

    /// The MCP `fetch_content` handler's caller-side shaping, replicated
    /// exactly (path match, source-order sort, names + anchor, first stored
    /// line): the handler delegates its whole bound dispatch — exact read,
    /// chunks reconstruction and symbol span — to the SAME provider
    /// operation this test drives (`code_fetch_exact_participants`).
    fn mcp_shaped_candidates(
        chunks: &[engram_domain::KnowledgeChunk],
        path: &str,
    ) -> Vec<ExactReadCandidate> {
        let mut matched: Vec<&engram_domain::KnowledgeChunk> = chunks
            .iter()
            .filter(|chunk| chunk.location.as_ref().and_then(|l| l.path.as_deref()) == Some(path))
            .collect();
        matched.sort_by_key(|chunk| {
            (
                chunk
                    .location
                    .as_ref()
                    .and_then(|l| l.start_line)
                    .unwrap_or(u32::MAX),
                chunk
                    .location
                    .as_ref()
                    .and_then(|l| l.end_line)
                    .unwrap_or(u32::MAX),
            )
        });
        matched
            .iter()
            .map(|chunk| {
                let location = chunk.location.as_ref();
                let mut names: Vec<String> = chunk
                    .entities
                    .iter()
                    .filter_map(|e| e.name.clone())
                    .collect();
                if let Some(anchor) = location.and_then(|location| location.anchor.as_deref()) {
                    names.push(anchor.to_owned());
                }
                ExactReadCandidate {
                    record_id: chunk.id.to_string(),
                    start_line: location.and_then(|location| location.start_line),
                    end_line: location.and_then(|location| location.end_line),
                    names,
                    text_head: chunk.text.lines().next().unwrap_or("").to_owned(),
                }
            })
            .collect()
    }

    /// Independent 1-based inclusive line-span → byte-range mapping over the
    /// fixture bytes (computed here, never through the coordinator).
    fn independent_line_span(bytes: &[u8], start_line: u32, end_line: u32) -> &[u8] {
        let mut offsets = Vec::with_capacity(bytes.len() / 8 + 2);
        offsets.push(0usize);
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                offsets.push(index + 1);
            }
        }
        let start = offsets[(start_line - 1) as usize];
        let end = if (end_line as usize) < offsets.len() {
            offsets[end_line as usize]
        } else {
            bytes.len()
        };
        &bytes[start..end]
    }

    /// The MCP handler's `mode: "chunks"` reconstruction semantics over the
    /// AUTHENTICATED participants only: durable-span-ordered, non-empty
    /// UTF-8 slices of the digest-validated revision bytes, joined with the
    /// code separator. Stored chunk text is never consumed.
    fn reconstruct_chunks_mode(
        view_bytes: &[u8],
        participants: &[engram_integration::code_binding::BoundParticipant],
    ) -> String {
        let mut ordered = participants.to_vec();
        ordered.sort_by_key(|participant| (participant.start_line, participant.end_line));
        ordered
            .iter()
            .filter_map(|participant| {
                let text = std::str::from_utf8(
                    &view_bytes[participant.byte_span.0..participant.byte_span.1],
                )
                .expect("authenticated participant span is UTF-8");
                (!text.is_empty()).then_some(text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// durable_scan_stage_publish_fetch (pre-gate obligation 2): the WHOLE
    /// bound chain over the durable SQLite cell — REAL fixture files scanned
    /// from disk through the real ingest/staging path
    /// (`prepare_generation` + `stage_generation_records`: real chunks, real
    /// durable lineage with canonical spans, real indexed-source
    /// descriptors), a REAL staged publication (`rebuild`: begin attempt →
    /// real lane receipts → ready → publish at a retained commit), then the
    /// provider operation the MCP `fetch_content` handler dispatches to
    /// (`code_fetch_exact_participants`) for the exact read, the
    /// chunks-mode reconstruction at ACTUALLY STAGED durable spans, and the
    /// symbol span. Proves: working-tree drift never answers; a durable
    /// current-row span substitution denies; a tampered retained blob denies
    /// on the staged digest; a second publication flips the participants to
    /// the new generation while the old generation's rows coexist; and an
    /// old-only symbol / retired generation denies.
    #[test]
    fn durable_scan_stage_publish_fetch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let checkout = dir.path().join("checkout");
        std::fs::create_dir_all(checkout.join("src")).expect("mkdir checkout src");
        std::fs::write(checkout.join("src/service.py"), E2E_SERVICE_V1).expect("write service v1");
        std::fs::write(checkout.join("src/util.py"), E2E_UTIL).expect("write util");

        // ONE durable SQLite cell serves knowledge AND code bindings.
        let cell = dir.path().join("cell.sqlite");
        let knowledge = Arc::new(SqlKnowledgeStore::open_file(&cell).expect("open knowledge"));
        let bindings = Arc::new(SqlCodeBindingRepository::open(&cell).expect("open binding store"));
        block_on(bindings.bind(CodeBinding {
            scope: scope(),
            repo_id: RepoId("repo-a".to_string()),
            branch_ref: "refs/heads/main".to_string(),
            version: 1,
            active_generation: None,
            epoch: 0,
        }))
        .expect("bind");

        let retained_root = dir.path().join("retained-e2e");
        let manifest_root = dir.path().join("manifests");
        let commit1 = "1".repeat(40);
        let commit2 = "2".repeat(40);
        let (prepared1, published1) = e2e_publish(
            &checkout,
            &retained_root,
            &manifest_root,
            &bindings,
            &knowledge,
            "gen-e2e-1",
            &commit1,
            "attempt-e2e-1",
        );
        assert_eq!(published1.active_generation.as_deref(), Some("gen-e2e-1"));
        assert_eq!(published1.epoch, 1);

        // The fetch side rides the SAME durable cell: the real knowledge
        // listing, the real coordinator over the binding store, and the real
        // adapter-owned retained reader.
        let coordinator = CodeBindingCoordinator::new(
            bindings.clone() as Arc<dyn CodeBindingRepository>,
            Arc::new(AllowAuthorizer),
            Vec::new(),
        );
        let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .knowledge_query(knowledge.clone() as Arc<dyn engram_integration::KnowledgeQuery>)
        .code_bindings(Arc::new(coordinator))
        .code_source(Arc::new(RetainedSnapshotReader::new(retained_root.clone())))
        .build();

        // -- the served read must ride the ACTUALLY STAGED descriptors -----
        let service_descriptor = block_on(bindings.get_source_descriptor(
            &scope(),
            &RepoId("repo-a".to_string()),
            "gen-e2e-1",
            "src/service.py",
        ))
        .expect("descriptor lookup")
        .expect("service descriptor staged");
        let expected_digest = engram_ingest::content_hash(E2E_SERVICE_V1.as_bytes())
            .trim_start_matches("sha256:")
            .to_string();
        assert_eq!(service_descriptor.path, "src/service.py");
        assert_eq!(service_descriptor.commit_oid, commit1);
        assert_eq!(
            service_descriptor.content_digest, expected_digest,
            "the staged descriptor digests the SCANNED file bytes"
        );
        let service_chunks1: Vec<&engram_domain::KnowledgeChunk> = prepared1
            .documents
            .iter()
            .find(|doc| doc.source.path == "src/service.py")
            .expect("service document")
            .records
            .chunks
            .iter()
            .collect();
        assert_eq!(service_chunks1.len(), 2, "real chunker: alpha + legacy");
        for chunk in &service_chunks1 {
            let lineage = prepared1
                .lineage
                .get(&chunk.id.to_string())
                .expect("staged lineage for chunk");
            assert_eq!(lineage.source_path, "src/service.py");
            assert!(
                lineage.span.is_some(),
                "staged chunks carry durable canonical spans"
            );
        }

        // -- exact read (the handler's default mode) ----------------------
        let chunks = block_on(
            provider
                .knowledge_query()
                .expect("knowledge query")
                .list_chunks(&scope()),
        )
        .expect("durable chunk listing");
        assert!(chunks.len() >= 3, "both scanned files staged chunk rows");
        let candidates = mcp_shaped_candidates(&chunks, "src/service.py");
        assert_eq!(candidates.len(), 2);
        let served = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &candidates,
            None,
        ))
        .expect("exact participant read");
        let ParticipantExactSource::Served {
            repo_id,
            generation_id,
            view,
            span,
            participants,
        } = served
        else {
            panic!("the freshly published generation must serve: {served:?}");
        };
        assert_eq!(repo_id, "repo-a");
        assert_eq!(generation_id, "gen-e2e-1");
        assert_eq!(view.bytes, E2E_SERVICE_V1.as_bytes());
        assert_eq!(view.commit_oid, commit1);
        assert_eq!(view.full_content_digest, expected_digest);
        assert_eq!(view.served_digest, view.full_content_digest);
        assert_eq!(span, None, "no symbol requested: whole-file view");
        // Participants are EXACTLY the staged gen-1 chunks at their staged
        // durable canonical spans (1–2 alpha, 4–5 legacy).
        assert_eq!(participants.len(), 2);
        let mut durable_spans: Vec<(u32, u32)> = participants
            .iter()
            .map(|participant| (participant.start_line, participant.end_line))
            .collect();
        durable_spans.sort();
        assert_eq!(durable_spans, vec![(1, 2), (4, 5)]);
        for participant in &participants {
            let lineage = prepared1
                .lineage
                .get(&participant.record_id)
                .expect("participant is a staged record");
            assert_eq!(
                lineage.span,
                Some(engram_domain::CanonicalSpan {
                    start_line: participant.start_line,
                    end_line: participant.end_line,
                }),
                "served spans ARE the staged durable spans"
            );
            let served_slice = &view.bytes[participant.byte_span.0..participant.byte_span.1];
            assert_eq!(
                served_slice,
                independent_line_span(
                    E2E_SERVICE_V1.as_bytes(),
                    participant.start_line,
                    participant.end_line
                ),
                "authenticated participant bytes are the fixture's own lines"
            );
        }

        // -- chunks reconstruction from authenticated retained bytes ------
        let reconstruction = reconstruct_chunks_mode(&view.bytes, &participants);
        let expected_reconstruction = format!(
            "{}\n{}",
            String::from_utf8_lossy(independent_line_span(E2E_SERVICE_V1.as_bytes(), 1, 2)),
            String::from_utf8_lossy(independent_line_span(E2E_SERVICE_V1.as_bytes(), 4, 5)),
        );
        assert_eq!(reconstruction, expected_reconstruction);
        assert!(reconstruction.contains("def alpha():\r\n"));
        assert!(reconstruction.contains("def legacy():\r\n"));
        assert!(reconstruction.contains("caf\u{e9}"));

        // -- symbol span ---------------------------------------------------
        let symbol_served = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &candidates,
            Some("alpha"),
        ))
        .expect("symbol participant read");
        let ParticipantExactSource::Served { span, .. } = symbol_served else {
            panic!("alpha must resolve: {symbol_served:?}");
        };
        let (start, end) = span.expect("alpha span");
        assert_eq!(
            &view.bytes[start..end],
            independent_line_span(E2E_SERVICE_V1.as_bytes(), 1, 2),
            "the symbol span is the durable canonical span's exact bytes"
        );
        assert!(view.bytes[start..end].starts_with(b"def alpha"));
        let missing = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &candidates,
            Some("nosuch"),
        ))
        .expect("denial is an outcome, not an error");
        assert!(
            matches!(&missing, ParticipantExactSource::Denied { symbol_selection: true, reason } if reason.contains("no chunk in the indexed generation")),
            "missing symbol denies as a symbol selection: {missing:?}"
        );

        // -- second staged path: the util.py descriptor serves too ---------
        let util_candidates = mcp_shaped_candidates(&chunks, "src/util.py");
        assert_eq!(util_candidates.len(), 1);
        let util_served = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/util.py",
            &util_candidates,
            None,
        ))
        .expect("util participant read");
        assert!(
            matches!(&util_served, ParticipantExactSource::Served { view, participants, .. } if view.bytes == E2E_UTIL.as_bytes() && participants.len() == 1)
        );

        // -- working-tree DRIFT never answers ------------------------------
        std::fs::write(checkout.join("src/service.py"), "IMPOSTOR WORKING TREE")
            .expect("drift the working tree");
        let drifted = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &candidates,
            None,
        ))
        .expect("participant read under working-tree drift");
        assert!(
            matches!(&drifted, ParticipantExactSource::Served { view, .. } if view.bytes == E2E_SERVICE_V1.as_bytes()),
            "the retained revision answers, never the drifted checkout"
        );

        // -- durable CURRENT-row span SUBSTITUTION denies ------------------
        let alpha_row = chunks
            .iter()
            .find(|chunk| {
                chunk
                    .entities
                    .iter()
                    .any(|entity| entity.name.as_deref() == Some("alpha"))
                    && chunk
                        .location
                        .as_ref()
                        .and_then(|location| location.path.as_deref())
                        == Some("src/service.py")
            })
            .expect("durable alpha row")
            .clone();
        let mut substituted = alpha_row.clone();
        substituted.location.as_mut().expect("location").start_line = Some(
            substituted
                .location
                .as_ref()
                .and_then(|l| l.start_line)
                .expect("start")
                + 1,
        );
        block_on(knowledge.put_chunk(substituted)).expect("durable row substitution");
        let substituted_outcome = block_on(
            provider.code_fetch_exact_participants(
                &actor(),
                &scope(),
                "src/service.py",
                &mcp_shaped_candidates(
                    &block_on(
                        provider
                            .knowledge_query()
                            .expect("knowledge query")
                            .list_chunks(&scope()),
                    )
                    .expect("re-list"),
                    "src/service.py",
                ),
                None,
            ),
        )
        .expect("denial is an outcome");
        assert!(
            matches!(&substituted_outcome, ParticipantExactSource::Denied { symbol_selection: false, reason } if reason.contains("does not match the retained revision")),
            "a drifted durable span claim denies: {substituted_outcome:?}"
        );
        block_on(knowledge.put_chunk(alpha_row.clone())).expect("restore the durable row");

        // -- TAMPERED retained blob denies on the staged digest ------------
        let blob = retained_root.join(&commit1).join("src").join("service.py");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o600))
                .expect("take write permission");
        }
        std::fs::write(&blob, b"TAMPERED BYTES").expect("tamper blob");
        let tampered = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &candidates,
            None,
        ));
        assert!(
            matches!(&tampered, Err(CoreError::InvalidRequest { reason }) if reason.contains("does not match the durable indexed-source digest")),
            "tampered retained bytes deny on digest: {tampered:?}"
        );
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o600))
                .expect("write permission for restore");
        }
        std::fs::write(&blob, E2E_SERVICE_V1).expect("restore blob");
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o400))
                .expect("restore readonly");
        }

        // -- SECOND publication flips the participants ---------------------
        std::fs::write(checkout.join("src/service.py"), E2E_SERVICE_V2).expect("write service v2");
        let (prepared2, published2) = e2e_publish(
            &checkout,
            &retained_root,
            &manifest_root,
            &bindings,
            &knowledge,
            "gen-e2e-2",
            &commit2,
            "attempt-e2e-2",
        );
        assert_eq!(published2.active_generation.as_deref(), Some("gen-e2e-2"));
        assert_eq!(published2.epoch, 2);

        // Durable coexistence: BOTH generations' rows list at the path.
        let chunks_after = block_on(
            provider
                .knowledge_query()
                .expect("knowledge query")
                .list_chunks(&scope()),
        )
        .expect("re-list after second publication");
        let id_of = |prepared: &engram_ingest::PreparedGeneration, path: &str| {
            prepared
                .documents
                .iter()
                .find(|doc| doc.source.path == path)
                .expect("document")
                .records
                .chunks
                .iter()
                .map(|chunk| chunk.id.to_string())
                .collect::<std::collections::HashSet<String>>()
        };
        let service_ids1 = id_of(&prepared1, "src/service.py");
        let service_ids2 = id_of(&prepared2, "src/service.py");
        assert_eq!(service_ids1.len(), 2);
        assert_eq!(service_ids2.len(), 3, "v2: File + beta + alpha");
        assert!(
            service_ids1.is_disjoint(&service_ids2),
            "generation namespaces"
        );
        let listed_service: std::collections::HashSet<String> = chunks_after
            .iter()
            .filter(|chunk| {
                chunk
                    .location
                    .as_ref()
                    .and_then(|location| location.path.as_deref())
                    == Some("src/service.py")
            })
            .map(|chunk| chunk.id.to_string())
            .collect();
        assert!(
            listed_service.is_superset(&service_ids1) && listed_service.is_superset(&service_ids2),
            "retention coexistence: both generations' rows remain listed"
        );

        // The served origin flips to gen-e2e-2 with gen-e2e-2's spans.
        let flipped = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &mcp_shaped_candidates(&chunks_after, "src/service.py"),
            None,
        ))
        .expect("flipped participant read");
        let ParticipantExactSource::Served {
            generation_id: flipped_generation,
            view: flipped_view,
            participants: flipped_participants,
            ..
        } = flipped
        else {
            panic!("gen-e2e-2 must serve: {flipped:?}");
        };
        assert_eq!(flipped_generation, "gen-e2e-2");
        assert_eq!(flipped_view.bytes, E2E_SERVICE_V2.as_bytes());
        assert_eq!(flipped_view.commit_oid, commit2);
        let mut flipped_spans: Vec<(u32, u32)> = flipped_participants
            .iter()
            .map(|participant| (participant.start_line, participant.end_line))
            .collect();
        flipped_spans.sort();
        assert_eq!(flipped_spans, vec![(1, 1), (3, 4), (6, 7)]);
        for participant in &flipped_participants {
            assert!(
                service_ids2.contains(&participant.record_id),
                "participants are the NEW generation's staged records"
            );
            assert!(
                !service_ids1.contains(&participant.record_id),
                "no retired-generation record rides the flip"
            );
            assert_eq!(
                &flipped_view.bytes[participant.byte_span.0..participant.byte_span.1],
                independent_line_span(
                    E2E_SERVICE_V2.as_bytes(),
                    participant.start_line,
                    participant.end_line
                )
            );
        }
        let v2_reconstruction = reconstruct_chunks_mode(&flipped_view.bytes, &flipped_participants);
        let expected_v2_reconstruction = [
            independent_line_span(E2E_SERVICE_V2.as_bytes(), 1, 1),
            independent_line_span(E2E_SERVICE_V2.as_bytes(), 3, 4),
            independent_line_span(E2E_SERVICE_V2.as_bytes(), 6, 7),
        ]
        .iter()
        .map(|slice| String::from_utf8_lossy(slice).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
        assert_eq!(v2_reconstruction, expected_v2_reconstruction);
        assert!(v2_reconstruction.contains("import os\r\n"));
        assert!(v2_reconstruction.contains("def beta():\r\n"));
        assert!(v2_reconstruction.contains("return 'nuovo'\r\n"));

        // The moved symbol resolves at its NEW durable span.
        let moved = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &mcp_shaped_candidates(&chunks_after, "src/service.py"),
            Some("alpha"),
        ))
        .expect("moved symbol read");
        let ParticipantExactSource::Served { span, .. } = moved else {
            panic!("alpha must resolve in gen-e2e-2: {moved:?}");
        };
        let (start, end) = span.expect("moved span");
        assert_eq!(
            &flipped_view.bytes[start..end],
            independent_line_span(E2E_SERVICE_V2.as_bytes(), 6, 7),
            "alpha serves at its MOVED durable span"
        );

        // OLD-ONLY symbol denies: legacy's durable rows still list at the
        // path, but none is a current participant.
        let legacy = block_on(provider.code_fetch_exact_participants(
            &actor(),
            &scope(),
            "src/service.py",
            &mcp_shaped_candidates(&chunks_after, "src/service.py"),
            Some("legacy"),
        ))
        .expect("denial is an outcome");
        assert!(
            matches!(&legacy, ParticipantExactSource::Denied { symbol_selection: true, reason } if reason.contains("no chunk in the indexed generation")),
            "the old-only symbol denies: {legacy:?}"
        );

        // The RETIRED generation denies without an authorized lease.
        let retired = block_on(provider.code_fetch_exact_at(
            &actor(),
            &scope(),
            "repo-a",
            "gen-e2e-1",
            "src/service.py",
            None,
        ));
        assert!(
            matches!(&retired, Err(CoreError::InvalidRequest { reason }) if reason.contains("no authorized read lease")),
            "the retired generation denies: {retired:?}"
        );
    }
}

// ---------- task_018 focused cases -----------------------------------------

mod task_018 {
    use super::*;
    use engram_integration::code_binding::CodeFreshnessState;
    use engram_knowledge::{CodeBindingRepository, CodeRefObservation, RefObservationPolicy};

    /// binding_ref_observation: an on-request observation of the CONFIGURED
    /// ref is durably recorded SEPARATELY from the indexed generation OID;
    /// status reports both facts independently, the observation never moves
    /// the published pointer, and an observation naming any other ref is a
    /// typed denial.
    #[test]
    fn binding_ref_observation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = observation_fixture(dir.path(), &[("repo-a", 'a', 'c')]);
        let repo = RepoId("repo-a".to_string());
        let indexed_oid = "a".repeat(40);
        let observed_oid = "b".repeat(40);

        let observation = block_on(fixture.coordinator.record_ref_observation(
            &actor(),
            &scope(),
            &repo,
            &observed_oid,
        ))
        .expect("record observation");
        assert_eq!(observation.branch_ref, "refs/heads/main");
        assert_eq!(observation.observed_oid, observed_oid);
        assert_eq!(observation.policy, RefObservationPolicy::OnRequest);

        let status = fixture.status("repo-a");
        assert_eq!(
            status.indexed_commit_oid.as_deref(),
            Some(indexed_oid.as_str()),
            "indexed OID comes from the durable generation record"
        );
        assert_eq!(
            status.freshness.observed_oid.as_deref(),
            Some(observed_oid.as_str()),
            "the observed ref OID is a SEPARATE fact from the indexed OID"
        );
        assert_eq!(status.freshness.observed_at, Some(observation.observed_at));
        assert_eq!(
            status.freshness.state,
            CodeFreshnessState::Observed {
                matches_indexed: Some(false)
            }
        );

        // Durable: a FRESH process handle reads the same observation back,
        // and the published pointer never moved for the observation.
        let reopened =
            SqlCodeBindingRepository::open(&fixture.db_path).expect("reopen binding store");
        let stored = block_on(reopened.get_ref_observation(&scope(), &repo))
            .expect("read observation")
            .expect("observation persisted");
        assert_eq!(stored, observation);
        let binding = block_on(
            (fixture.bindings.clone() as Arc<dyn CodeBindingRepository>)
                .get_binding(&scope(), &repo),
        )
        .expect("read binding")
        .expect("binding exists");
        assert_eq!(
            binding.active_generation.as_deref(),
            Some("gen-t018-0-repo-a"),
            "an observation never republishes or moves the active generation"
        );

        // An observation naming any ref OTHER than the bound branch is a
        // typed denial, never a silent record.
        let mismatch = block_on(
            (fixture.bindings.clone() as Arc<dyn CodeBindingRepository>).put_ref_observation(
                &scope(),
                &repo,
                CodeRefObservation {
                    branch_ref: "refs/heads/other".to_string(),
                    observed_oid: observed_oid.clone(),
                    observed_at: observation.observed_at,
                    policy: RefObservationPolicy::OnRequest,
                },
            ),
        );
        assert!(
            matches!(mismatch, Err(CoreError::InvalidRequest { ref reason }) if reason == "BindingConflict"),
            "foreign-ref observation must deny with BindingConflict, got {mismatch:?}"
        );
        // A malformed OID never persists.
        assert!(
            matches!(
                block_on(fixture.coordinator.record_ref_observation(
                    &actor(),
                    &scope(),
                    &repo,
                    "not-an-oid"
                )),
                Err(CoreError::InvalidRequest { .. })
            ),
            "malformed observed OID must deny"
        );
    }

    /// head_irrelevant: binding-keyed status/freshness/checkpoint facts are
    /// unchanged by ANY mutation of an on-disk checkout — including a
    /// HEAD-shaped file naming a different OID — because no checkout path is
    /// ever an input to the durable reads.
    #[test]
    fn head_irrelevant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = observation_fixture(dir.path(), &[("repo-a", 'a', 'c')]);
        let repo = RepoId("repo-a".to_string());
        let observed_oid = "b".repeat(40);
        block_on(fixture.coordinator.record_ref_observation(
            &actor(),
            &scope(),
            &repo,
            &observed_oid,
        ))
        .expect("record observation");
        let before = fixture.status("repo-a");

        // Mutate the "checkout": rewrite the working copy, plant a
        // HEAD-shaped file naming a THIRD oid, and add a packed-refs style
        // file — none of it is an input to the binding-keyed reads.
        let working = &fixture.repo("repo-a").working_root;
        std::fs::write(working.join("src"), "totally different bytes\n").expect("rewrite working");
        std::fs::create_dir_all(working.join(".git")).expect("mkdir .git");
        let head_oid = "c".repeat(40);
        std::fs::write(working.join(".git/HEAD"), &head_oid).expect("plant HEAD file");
        std::fs::write(
            working.join(".git/packed-refs"),
            format!("{} refs/heads/main\n", head_oid),
        )
        .expect("plant packed-refs");

        let after = fixture.status("repo-a");
        assert_eq!(after.indexed_commit_oid, before.indexed_commit_oid);
        assert_eq!(after.freshness.observed_oid, before.freshness.observed_oid);
        assert_eq!(after.freshness.observed_at, before.freshness.observed_at);
        assert_eq!(after.freshness.state, before.freshness.state);
        assert_eq!(after.checkpoint, before.checkpoint);
        assert_ne!(
            after.indexed_commit_oid.as_deref(),
            Some(head_oid.as_str()),
            "a checkout HEAD oid must never leak into the indexed OID"
        );
        assert_ne!(
            after.freshness.observed_oid.as_deref(),
            Some(head_oid.as_str()),
            "a checkout HEAD oid must never leak into the observed ref OID"
        );

        // Deleting the checkout entirely changes nothing either.
        std::fs::remove_dir_all(working).expect("delete checkout");
        let deleted = fixture.status("repo-a");
        assert_eq!(deleted.indexed_commit_oid, before.indexed_commit_oid);
        assert_eq!(
            deleted.freshness.observed_oid,
            before.freshness.observed_oid
        );
        assert_eq!(deleted.checkpoint, before.checkpoint);
    }

    /// stale_observation_unknown: without a valid observation the freshness
    /// state is UNKNOWN with no observed OID/time — while the indexed OID
    /// still comes from the durable generation record. Observations are
    /// per-repository: recording one repository's ref never manufactures a
    /// state for another.
    #[test]
    fn stale_observation_unknown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture =
            observation_fixture(dir.path(), &[("repo-a", 'a', 'c'), ("repo-b", 'b', 'd')]);

        let status = fixture.status("repo-a");
        assert_eq!(
            status.freshness.state,
            CodeFreshnessState::Unknown,
            "no observation exists: freshness must be unknown, never invented"
        );
        assert_eq!(status.freshness.observed_oid, None);
        assert_eq!(status.freshness.observed_at, None);
        assert_eq!(
            status.indexed_commit_oid.as_deref(),
            Some("a".repeat(40).as_str()),
            "unknown freshness does not blank or invent the indexed OID"
        );
        assert_eq!(
            status.freshness.branch_ref, "refs/heads/main",
            "the CONFIGURED ref is always reported"
        );

        // Record for repo-a only: repo-b stays unknown (per-repo keying).
        block_on(fixture.coordinator.record_ref_observation(
            &actor(),
            &scope(),
            &RepoId("repo-a".to_string()),
            &"9".repeat(40),
        ))
        .expect("record repo-a observation");
        let repo_b = fixture.status("repo-b");
        assert_eq!(repo_b.freshness.state, CodeFreshnessState::Unknown);
        assert_eq!(repo_b.freshness.observed_oid, None);
        assert_eq!(
            repo_b.indexed_commit_oid.as_deref(),
            Some("b".repeat(40).as_str())
        );
        let repo_a = fixture.status("repo-a");
        assert_eq!(
            repo_a.freshness.observed_oid.as_deref(),
            Some("9".repeat(40).as_str())
        );
    }

    /// basename_checkpoint_collision: two repositories whose scan roots
    /// share the SAME basename select their OWN binding-keyed checkpoints —
    /// RepoId + binding version + generation + OID + fingerprints — and
    /// never each other's metadata or observations.
    #[test]
    fn basename_checkpoint_collision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture =
            observation_fixture(dir.path(), &[("proj-x", 'a', 'c'), ("proj-y", 'b', 'd')]);
        let roots: Vec<_> = fixture
            .repos
            .iter()
            .map(|repo| repo.working_root.clone())
            .collect();
        let basenames: Vec<_> = roots
            .iter()
            .map(|root| root.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(basenames[0], "proj", "fixture plants basename collisions");
        assert_eq!(basenames[0], basenames[1]);

        // Record an observation for proj-x ONLY.
        block_on(fixture.coordinator.record_ref_observation(
            &actor(),
            &scope(),
            &RepoId("proj-x".to_string()),
            &"7".repeat(40),
        ))
        .expect("record proj-x observation");

        let x = fixture.status("proj-x");
        let y = fixture.status("proj-y");
        assert_eq!(
            x.checkpoint.repo_id,
            RepoId("proj-x".to_string()),
            "checkpoints are RepoId-keyed, never basename-keyed"
        );
        assert_eq!(y.checkpoint.repo_id, RepoId("proj-y".to_string()));
        assert_eq!(
            x.checkpoint.generation.as_ref().unwrap().commit_oid,
            "a".repeat(40)
        );
        assert_eq!(
            y.checkpoint.generation.as_ref().unwrap().commit_oid,
            "b".repeat(40)
        );
        assert_ne!(
            x.checkpoint.generation.as_ref().unwrap().commit_oid,
            y.checkpoint.generation.as_ref().unwrap().commit_oid,
            "the shared basename must never cross-select metadata"
        );
        // The full fingerprint set is part of each checkpoint key.
        let xg = x.checkpoint.generation.as_ref().unwrap();
        assert_eq!(xg.parser_fingerprint, "parser-c");
        assert_eq!(xg.filter_fingerprint, "filter-c");
        let yg = y.checkpoint.generation.as_ref().unwrap();
        assert_eq!(yg.parser_fingerprint, "parser-d");
        assert_eq!(yg.filter_fingerprint, "filter-d");
        assert_eq!(x.checkpoint.binding_version, 1);
        assert_eq!(y.checkpoint.binding_version, 1);
        // Observations do not cross repositories either.
        assert_eq!(
            x.freshness.observed_oid.as_deref(),
            Some("7".repeat(40).as_str())
        );
        assert_eq!(y.freshness.observed_oid, None);
        assert_eq!(y.freshness.state, CodeFreshnessState::Unknown);
    }
}
