use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeGeneration, CodeLineage, CodeReceiptKind,
    CodeResetJournal, RepoId, Scope,
};
use engram_knowledge::{CodeBindingRepository, RepositoryRegistration};
use engram_store_sqlite::knowledge::SqlCodeBindingRepository;
use futures::executor::block_on;

fn scope() -> Scope {
    Scope {
        tenant: "reset-tenant".into(),
        subject: None,
        workspace: Some("reset-workspace".into()),
        session: None,
        environment: None,
    }
}
fn repo() -> RepoId {
    RepoId("reset-repo".into())
}
fn generation() -> CodeGeneration {
    CodeGeneration {
        id: "reset-generation".into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser-v1".into(),
        filter_fingerprint: "filter-v1".into(),
    }
}
fn setup() -> SqlCodeBindingRepository {
    let store = SqlCodeBindingRepository::open(":memory:").expect("store");
    let binding = CodeBinding {
        scope: scope(),
        repo_id: repo(),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    };
    block_on(store.register(RepositoryRegistration {
        repo_id: repo(),
        locator_id: "fixture".into(),
        acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
    }))
    .expect("register");
    block_on(store.bind(binding)).expect("bind");
    let generation = generation();
    block_on(store.begin_attempt(
        &scope(),
        &repo(),
        generation.clone(),
        CodeAttempt {
            id: "reset-attempt".into(),
            idempotency_key: "reset-key".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Created,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        },
    ))
    .expect("attempt");
    block_on(store.put_lineage(
        &scope(),
        "target-a",
        CodeLineage {
            repo_id: repo(),
            generation_id: generation.id,
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into()],
            span: None,
        },
    ))
    .expect("lineage");
    store
}
fn journal(preview: engram_domain::CodeResetPreview) -> CodeResetJournal {
    CodeResetJournal {
        reset_id: "reset-1".into(),
        preview_hash: preview.preview_hash.clone(),
        target_fingerprints: preview
            .targets
            .iter()
            .map(|target| (target.record_id.clone(), target.fingerprint.clone()))
            .collect(),
        authorizing_decisions: vec!["approved-by-fixture".into()],
        checkpoints: vec!["authorized".into()],
        preservation_digests: preview.preservation_digests.clone(),
        preview,
    }
}
mod task_023 {
    use super::*;
    #[test]
    fn preview_replay_mismatch_denied() {
        let store = setup();
        let mut value = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        value.preview_hash = "0".repeat(64);
        assert!(block_on(store.journal_reset(journal(value))).is_err());
    }
    #[test]
    fn substituted_target_denied() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut value = journal(preview);
        value.target_fingerprints[0].1 = "0".repeat(64);
        assert!(block_on(store.journal_reset(value)).is_err());
    }
    #[test]
    fn subset_approval_denied() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let mut value = journal(preview);
        value.target_fingerprints.clear();
        assert!(block_on(store.journal_reset(value)).is_err());
    }
    #[test]
    fn ambiguous_lineage_blocks_preview() {
        let store = setup();
        let mut preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        preview.ambiguity.push(engram_domain::AmbiguousOwnership {
            record_id: "legacy-row".into(),
            reason: "record-level code ownership cannot be proven".into(),
        });
        assert!(
            preview.validate().is_err(),
            "an ambiguity report cannot carry a deletion list"
        );
    }
    #[test]
    fn ordinary_preservation_digests_present() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        assert!(!preview.preservation_digests.is_empty());
        assert!(
            preview
                .preservation_digests
                .iter()
                .all(|digest| digest.len() == 64)
        );
    }
    #[test]
    fn journal_bound_to_preview_hash() {
        let store = setup();
        let preview = block_on(store.preview_reset(&scope(), &repo())).expect("preview");
        let value = journal(preview.clone());
        assert_eq!(value.preview_hash, preview.preview_hash);
        assert!(block_on(store.journal_reset(value)).is_ok());
    }
}
