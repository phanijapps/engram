use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage,
    CodeReceiptKind, CodeResetJournal, CodeResetPreview, IndexedSource, RepoId, Scope,
};
use serde::{Serialize, de::DeserializeOwned};

fn binding() -> CodeBinding {
    CodeBinding {
        scope: Scope {
            tenant: "t1".into(),
            subject: None,
            workspace: Some("w1".into()),
            session: None,
            environment: None,
        },
        repo_id: RepoId("repo-1".into()),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }
}
fn generation() -> CodeGeneration {
    CodeGeneration {
        id: "g1".into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser-v1".into(),
        filter_fingerprint: "filter-v1".into(),
    }
}
fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) {
    assert_eq!(
        serde_json::from_str::<T>(&serde_json::to_string(value).unwrap()).unwrap(),
        *value
    );
}

mod task_001 {
    use super::*;
    #[test]
    fn model_roundtrip() {
        let b = binding();
        b.validate().unwrap();
        roundtrip(&b);
        let g = generation();
        g.validate().unwrap();
        roundtrip(&g);
        let l = CodeLineage {
            repo_id: b.repo_id.clone(),
            generation_id: g.id.clone(),
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into()],
        };
        l.validate().unwrap();
        roundtrip(&l);
        let s = IndexedSource {
            repo_id: b.repo_id.clone(),
            generation_id: g.id.clone(),
            commit_oid: g.commit_oid.clone(),
            path: l.source_path.clone(),
            content_digest: "c".repeat(64),
        };
        s.validate().unwrap();
        roundtrip(&s);
        let a = CodeAttempt {
            id: "a1".into(),
            idempotency_key: "key1".into(),
            binding_version: 1,
            expected_epoch: 0,
            fencing_token: 1,
            state: CodeAttemptState::Ready,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        };
        a.validate().unwrap();
        roundtrip(&a);
        let p = CodeResetPreview {
            binding: b,
            preview_hash: "d".repeat(64),
            target_ids: vec!["target".into()],
        };
        p.validate().unwrap();
        roundtrip(&p);
        let j = CodeResetJournal {
            preview: p,
            checkpoints: vec!["begin".into()],
            preservation_digests: vec!["e".repeat(64)],
        };
        roundtrip(&j);
    }
    #[test]
    fn invalid_binding_generation_lineage_source_reset() {
        let mut b = binding();
        b.repo_id = RepoId(" ".into());
        assert_eq!(b.validate(), Err(CodeBindingError::InvalidRepoId));
        for bad in [
            "main",
            "refs/tags/v1",
            "refs/heads/a..b",
            "refs/heads/a//b",
            "refs/heads/.hidden",
            "refs/heads/x.lock",
            "refs/heads/a b",
            "refs/heads/x@{1}",
        ] {
            let mut b = binding();
            b.branch_ref = bad.into();
            assert_eq!(
                b.validate(),
                Err(CodeBindingError::InvalidBranchRef),
                "{bad}"
            );
        }
        let mut g = generation();
        g.commit_oid = "bad".into();
        assert_eq!(g.validate(), Err(CodeBindingError::InvalidSource));
        let mut l = CodeLineage {
            repo_id: RepoId("r".into()),
            generation_id: "g".into(),
            source_path: "../secret".into(),
            contributors: vec!["x".into()],
        };
        assert_eq!(l.validate(), Err(CodeBindingError::InvalidSource));
        l.source_path = "src/a".into();
        l.contributors.clear();
        assert_eq!(l.validate(), Err(CodeBindingError::InvalidSource));
        let mut s = IndexedSource {
            repo_id: RepoId("r".into()),
            generation_id: "g".into(),
            commit_oid: "a".repeat(40),
            path: "/etc/passwd".into(),
            content_digest: "c".repeat(64),
        };
        assert_eq!(s.validate(), Err(CodeBindingError::InvalidSource));
        s.path = "src/a".into();
        s.content_digest = "no-digest".into();
        assert_eq!(s.validate(), Err(CodeBindingError::InvalidSource));
        let mut p = CodeResetPreview {
            binding: binding(),
            preview_hash: "bad".into(),
            target_ids: vec!["x".into()],
        };
        assert_eq!(p.validate(), Err(CodeBindingError::InvalidResetPreview));
        p.preview_hash = "d".repeat(64);
        p.target_ids.clear();
        assert_eq!(p.validate(), Err(CodeBindingError::InvalidResetPreview));
    }
    #[test]
    fn breaking_selection() {
        let b = binding();
        b.validate().unwrap();
        assert_eq!(b.branch_ref, "refs/heads/main");
        assert_eq!(b.active_generation, None);
    }
}
