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
use engram_integration::EngramProviderBuilder;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator,
};
use engram_knowledge::{
    CodeBindingRepository, KnowledgeRepository, RetainedSourceReader, StageReceipt,
};
use engram_runtime::CoreError;
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
    fixture_inner(dir, Arc::new(AllowAuthorizer), true)
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
            std::time::Duration::from_secs(600),
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
        let leased = block_on(leased_fixture.provider.code_fetch_exact_at(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
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
        let lease = leased_fixture.historical_lease.clone().expect("lease");
        block_on(coordinator.release_read(&actor(), &RepoId("repo-a".to_string()), &lease))
            .expect("release lease");
        let released = block_on(leased_fixture.provider.code_fetch_exact_at(
            &actor(),
            &scope(),
            "repo-a",
            "gen-old",
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
}
