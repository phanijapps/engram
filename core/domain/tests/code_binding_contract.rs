use engram_domain::{CodeBinding, CodeBindingError, CodeGeneration, CodeLineage, CodeResetPreview, IndexedSource, RepoId, Scope};
use serde_json;

fn binding() -> CodeBinding {
    CodeBinding { scope: Scope { tenant: "t1".into(), subject: None, workspace: Some("w1".into()), session: None, environment: None }, repo_id: RepoId("repo-1".into()), branch_ref: "refs/heads/main".into(), version: 1, active_generation: None, epoch: 0 }
}

#[test]
fn task_001_model_roundtrip() {
    let value = binding();
    value.validate().unwrap();
    assert_eq!(serde_json::from_str::<CodeBinding>(&serde_json::to_string(&value).unwrap()).unwrap(), value);
    let generation = CodeGeneration { id: "g1".into(), binding_version: 1, commit_oid: "a".repeat(40), snapshot_digest: "b".repeat(64) };
    let lineage = CodeLineage { repo_id: value.repo_id.clone(), generation_id: generation.id.clone(), source_path: "src/lib.rs".into() };
    let source = IndexedSource { repo_id: value.repo_id.clone(), generation_id: generation.id.clone(), commit_oid: generation.commit_oid.clone(), path: lineage.source_path.clone(), content_digest: "c".repeat(64) };
    let reset = CodeResetPreview { binding: value, preview_hash: "d".repeat(64), target_ids: vec!["target-1".into()] };
    for encoded in [serde_json::to_string(&generation).unwrap(), serde_json::to_string(&lineage).unwrap(), serde_json::to_string(&source).unwrap(), serde_json::to_string(&reset).unwrap()] { assert!(!encoded.is_empty()); }
}

#[test]
fn task_001_invalid_binding_generation_lineage_source_reset() {
    let mut value = binding(); value.repo_id = RepoId(" ".into()); assert_eq!(value.validate(), Err(CodeBindingError::InvalidRepoId));
    let mut value = binding(); value.branch_ref = "main".into(); assert_eq!(value.validate(), Err(CodeBindingError::InvalidBranchRef));
    let mut value = binding(); value.branch_ref = "refs/heads/x..y".into(); assert_eq!(value.validate(), Err(CodeBindingError::InvalidBranchRef));
}

#[test]
fn task_001_breaking_selection() { assert_eq!(binding().branch_ref, "refs/heads/main"); }
