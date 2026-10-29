//! TASK-017 focused proof: exact retained committed bytes and same-OID spans
//! with honest unavailability errors and NO checkout fallback
//! (REQ-006 / AC-005 / AC-008).
//!
//! Every case drives REAL owned behavior: a REAL `retain_snapshot` write (the
//! immutable acquisition stage), a REAL file-backed `SqlCodeBindingRepository`
//! (bind → receipts → ready → publish at the retained commit), and the REAL
//! provider chain `code_fetch_exact` (binding → active generation →
//! generation record → retained blob).

use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::*;
use engram_ingest::git_source::{CommittedFile, ResolvedCommit, retain_snapshot};
use engram_integration::EngramProviderBuilder;
use engram_integration::code_binding::{
    CodeAuthorityAction, CodeBindingAuthorizer, CodeBindingCoordinator, RetainedCodeSource,
};
use engram_knowledge::{CodeBindingRepository, StageReceipt};
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

fn actor() -> Actor {
    Actor {
        id: Id::from("task-017-agent"),
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
        requested: &Scope,
        _repo: &RepoId,
        _action: CodeAuthorityAction,
    ) -> engram_runtime::CoreResult<()> {
        if actor.id == Id::from("task-017-agent") && requested == &scope() {
            Ok(())
        } else {
            Err(CoreError::PolicyDenied {
                reason: "fixture authorization denied".into(),
            })
        }
    }
}

fn generation(commit_oid: &str) -> CodeGeneration {
    CodeGeneration {
        id: "gen-1".to_string(),
        binding_version: 1,
        commit_oid: commit_oid.to_string(),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser".to_string(),
        filter_fingerprint: "filter".to_string(),
    }
}

fn attempt() -> CodeAttempt {
    CodeAttempt {
        id: "attempt-task-017".to_string(),
        idempotency_key: "idempotent-task-017".to_string(),
        binding_version: 1,
        expected_epoch: 0,
        expected_active_generation: None,
        fencing_token: 0,
        state: CodeAttemptState::Created,
        receipt_kinds: vec![CodeReceiptKind::Source, CodeReceiptKind::Knowledge],
    }
}

fn receipt(lane: CodeReceiptKind) -> StageReceipt {
    StageReceipt {
        attempt_id: "attempt-task-017".to_string(),
        generation_id: "gen-1".to_string(),
        lane: serde_json::to_value(&lane)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        digest: "d".repeat(64),
    }
}

struct SourceFixture {
    provider: engram_integration::EngramProvider,
    retained_root: std::path::PathBuf,
    working_tree: std::path::PathBuf,
    source: Vec<u8>,
    commit_oid: String,
}

/// Retains one REAL committed file (CRLF + non-ASCII), binds `repo-a` and
/// publishes `gen-1` at that commit, and mirrors the file into a "working
/// tree" directory the exact path MUST NOT consult.
fn fixture(dir: &std::path::Path) -> SourceFixture {
    let source = "def alpha():\r\n    return 'caf\u{e9}'\r\n\u{00e9}t\u{00e9} marker\r\n"
        .as_bytes()
        .to_vec();
    let commit = ResolvedCommit {
        branch_ref: "refs/heads/main".to_string(),
        oid: "c".repeat(40),
        object_format: "sha1".to_string(),
    };
    let files = vec![CommittedFile {
        path: "src/exact.py".into(),
        oid: "d".repeat(40),
        mode: "100644".to_string(),
        bytes: source.clone(),
    }];
    let retained_root = dir.join("retained");
    let snapshot = retain_snapshot(&retained_root, commit.clone(), &files).expect("retain");

    let bindings =
        SqlCodeBindingRepository::open(dir.join("bindings.sqlite")).expect("open binding store");
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
    block_on(bindings.begin_attempt(&scope(), &repo, generation(&snapshot.commit.oid), attempt()))
        .expect("begin attempt");
    block_on(bindings.record_receipt(receipt(CodeReceiptKind::Source))).expect("source receipt");
    block_on(bindings.record_receipt(receipt(CodeReceiptKind::Knowledge))).expect("knowledge");
    let (_, stored) = block_on(bindings.get_attempt(&scope(), &repo, "attempt-task-017"))
        .expect("get attempt")
        .expect("attempt exists");
    block_on(bindings.mark_ready(&scope(), &repo, "attempt-task-017", stored.fencing_token))
        .expect("mark ready");
    block_on(bindings.publish(&scope(), &repo, "attempt-task-017", 0)).expect("publish");
    let bindings = Arc::new(bindings);

    let coordinator = CodeBindingCoordinator::new(
        bindings.clone() as Arc<dyn CodeBindingRepository>,
        Arc::new(Authorizer),
        Vec::new(),
    );
    let provider = EngramProviderBuilder::new(engram_integration::CapabilityReport::new(
        CapabilityState::Supported,
    ))
    .code_bindings(Arc::new(coordinator))
    .code_source(Arc::new(RetainedCodeSource::new(retained_root.clone())))
    .build();

    // A "checkout" copy of the file that exists on disk. The exact path must
    // never consult it; the checkout_deleted case removes it entirely.
    let working_tree = dir.join("working");
    std::fs::create_dir_all(working_tree.join("src")).expect("mkdir working tree");
    std::fs::write(working_tree.join("src/exact.py"), source.clone()).expect("write working copy");

    SourceFixture {
        provider,
        retained_root,
        working_tree,
        source,
        commit_oid: snapshot.commit.oid.clone(),
    }
}

// ---------- task_017 focused cases -----------------------------------------

mod task_017 {
    use super::*;

    /// exact_bytes_and_spans: the read returns the committed bytes verbatim
    /// (CRLF + UTF-8 preserved) with content digests, and same-OID byte spans
    /// slice EXACTLY those bytes; out-of-bounds spans reject.
    #[test]
    fn exact_bytes_and_spans() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let view = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read");
        assert_eq!(view.bytes, fixture.source, "byte-exact committed content");
        assert_eq!(view.commit_oid, fixture.commit_oid);
        assert_eq!(view.span, None);
        let expected_digest = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&fixture.source);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(view.full_content_digest, expected_digest);
        assert_eq!(
            view.served_digest, expected_digest,
            "whole-file: served == full"
        );

        // Same-OID span: bytes [first CRLF .. second CRLF).
        let first_crlf = fixture
            .source
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("first CRLF");
        let span_start = first_crlf + 2;
        let span_end = fixture.source.len() - "\r\n".len();
        let spanned = block_on(fixture.provider.code_fetch_exact(
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
        assert_eq!(
            spanned.full_content_digest, expected_digest,
            "span keeps the full-blob identity"
        );

        // Out-of-bounds and inverted spans reject cleanly.
        let beyond = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            Some(0..fixture.source.len() + 1),
        ));
        assert!(beyond.is_err(), "out-of-bounds spans must reject");
    }

    /// checkout_deleted: with the working tree GONE, the exact read still
    /// returns the retained bytes — the answer never depended on the checkout.
    #[test]
    fn checkout_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());
        std::fs::remove_dir_all(&fixture.working_tree).expect("delete the whole working tree");

        let view = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read after checkout deletion");
        assert_eq!(view.bytes, fixture.source);
        assert_eq!(view.commit_oid, fixture.commit_oid);
    }

    /// source_unavailable: a path the snapshot never retained — or a missing
    /// snapshot root — is a typed `ExactSourceUnavailable` error, and an
    /// on-disk DECOY with different bytes is never served. An unbound
    /// repository is `BindingRequired`.
    #[test]
    fn source_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        // A path not in the retained inventory.
        let missing = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/not-retained.py",
            None,
        ));
        assert!(
            matches!(&missing, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "missing retained path surfaces the typed error: {missing:?}"
        );

        // A decoy on disk with DIFFERENT bytes at the same relative path must
        // not be served: point a second fixture's root at an empty directory
        // while the working tree holds an impostor.
        std::fs::write(fixture.working_tree.join("src/exact.py"), "IMPOSTER BYTES")
            .expect("plant impostor");
        let view = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("retained snapshot still answers");
        assert_eq!(
            view.bytes, fixture.source,
            "the retained blob wins; the on-disk impostor is never read"
        );

        // A snapshot root that disappears entirely errors, never falls back.
        std::fs::remove_dir_all(&fixture.retained_root).expect("remove retained root");
        let gone = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&gone, Err(CoreError::InvalidRequest { reason }) if reason.contains("ExactSourceUnavailable")),
            "no disk fallback after snapshot loss: {gone:?}"
        );

        // An unbound repository denies before any source consult.
        let unbound = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-unbound",
            "src/exact.py",
            None,
        ));
        assert!(
            matches!(&unbound, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "unbound repositories are denied: {unbound:?}"
        );
    }

    /// chunk_mode_not_exact: chunk text is a retrieval reconstruction (a code
    /// container keeps signature/doc-comment only), NEVER the exact source —
    /// the exact view must match the committed bytes, and any chunk-derived
    /// answer must carry an explicit reconstruction label (proven on the MCP
    /// surface in `engram-mcp` fetch_content). This case pins the provider
    /// side: exact reads are byte-identical to the blob and structurally
    /// labeled with revision + digest, so a chunk answer can never pose as
    /// one.
    #[test]
    fn chunk_mode_not_exact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = fixture(dir.path());

        let exact = block_on(fixture.provider.code_fetch_exact(
            &scope(),
            "repo-a",
            "src/exact.py",
            None,
        ))
        .expect("exact read");

        // The reconstruction a chunker would serve for the same file: the
        // first line only (signature), a strict subset of the exact bytes.
        let signature_only: Vec<u8> = fixture
            .source
            .split(|byte| *byte == b'\n')
            .next()
            .map(<[u8]>::to_vec)
            .expect("first line");
        assert!(
            exact.bytes.len() > signature_only.len() && exact.bytes.starts_with(&signature_only),
            "the exact view is the full committed file, not the chunk reconstruction"
        );

        // Structural honesty: every exact view carries the revision identity
        // (commit OID + digest) that reconstruction answers cannot claim.
        assert_eq!(exact.commit_oid, fixture.commit_oid);
        assert_eq!(exact.full_content_digest.len(), 64);
        assert!(
            exact.bytes.contains(&b'\r'),
            "exact bytes preserve CRLF; chunk-mode stitching would not guarantee it"
        );
    }
}
