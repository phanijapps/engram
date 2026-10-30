use engram_domain::{
    CodeAttempt, CodeAttemptState, CodeBinding, CodeBindingError, CodeGeneration, CodeLineage,
    CodeReceiptKind, CodeResetJournal, CodeResetPreview, IndexedSource, PreservationInventory,
    PreservationRecord, RepoId, Scope,
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
            span: None,
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
            expected_active_generation: None,
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
            targets: vec![],
            ambiguity: vec![],
            preservation_digests: vec![PreservationInventory::default().digest.clone()],
            preservation_inventory: PreservationInventory::default(),
            binding_versions: vec![],
            code_epoch: 0,
        };
        p.validate().unwrap();
        roundtrip(&p);
        let j = CodeResetJournal {
            reset_id: "reset-1".into(),
            preview_hash: p.preview_hash.clone(),
            target_fingerprints: vec![],
            authorizing_decisions: vec!["fixture".into()],
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
            span: None,
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
            targets: vec![],
            ambiguity: vec![],
            preservation_digests: vec![PreservationInventory::default().digest.clone()],
            preservation_inventory: PreservationInventory::default(),
            binding_versions: vec![],
            code_epoch: 0,
        };
        assert_eq!(p.validate(), Err(CodeBindingError::InvalidResetPreview));
        p.preview_hash = "d".repeat(64);
        p.target_ids.clear();
        assert_eq!(p.validate(), Err(CodeBindingError::InvalidResetPreview));
    }
    #[test]
    fn breaking_selection() {
        let mut b = binding();
        b.validate().unwrap();
        let wire = serde_json::to_value(&b).unwrap();
        assert_eq!(wire["branchRef"], "refs/heads/main");
        assert_eq!(wire["activeGeneration"], serde_json::Value::Null);
        assert!(wire.get("branch_ref").is_none());
        b.version = 0;
        assert_eq!(b.validate(), Err(CodeBindingError::InvalidBindingVersion));
        let mut b = binding();
        b.active_generation = Some(String::new());
        assert_eq!(b.validate(), Err(CodeBindingError::InvalidGeneration));
        let error = serde_json::to_value(CodeBindingError::BindingRequired).unwrap();
        assert_eq!(error, "binding_required");
        let mut attempt = CodeAttempt {
            id: "a".into(),
            idempotency_key: "k".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Ready,
            receipt_kinds: vec![],
        };
        assert_eq!(attempt.validate(), Err(CodeBindingError::InvalidAttempt));
        attempt.receipt_kinds.push(CodeReceiptKind::Knowledge);
        attempt.validate().unwrap();
        let journal = CodeResetJournal {
            reset_id: "reset-1".into(),
            preview_hash: "a".repeat(64),
            target_fingerprints: vec![],
            authorizing_decisions: vec!["fixture".into()],
            preview: CodeResetPreview {
                binding: binding(),
                preview_hash: "a".repeat(64),
                target_ids: vec!["x".into()],
                targets: vec![],
                ambiguity: vec![],
                preservation_digests: vec![PreservationInventory::default().digest.clone()],
                preservation_inventory: PreservationInventory::default(),
                binding_versions: vec![],
                code_epoch: 0,
            },
            checkpoints: vec!["start".into()],
            preservation_digests: vec!["b".repeat(64)],
        };
        let wire = serde_json::to_value(journal).unwrap();
        assert_eq!(wire["preservationDigests"][0], "b".repeat(64));
    }
}

mod task_002 {
    use super::*;
    use std::{fs, path::PathBuf, process::Command};
    fn schema() -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/schemas/engram-v1.schema.json");
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }
    fn valid_values() -> Vec<(&'static str, serde_json::Value)> {
        let b = binding();
        let g = generation();
        let l = CodeLineage {
            repo_id: b.repo_id.clone(),
            generation_id: g.id.clone(),
            source_path: "src/lib.rs".into(),
            contributors: vec!["src/lib.rs".into()],
            span: None,
        };
        let source = IndexedSource {
            repo_id: b.repo_id.clone(),
            generation_id: g.id.clone(),
            commit_oid: g.commit_oid.clone(),
            path: l.source_path.clone(),
            content_digest: "c".repeat(64),
        };
        let attempt = CodeAttempt {
            id: "a1".into(),
            idempotency_key: "k1".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Ready,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        };
        let preview = CodeResetPreview {
            binding: b,
            preview_hash: "d".repeat(64),
            target_ids: vec!["target".into()],
            targets: vec![],
            ambiguity: vec![],
            preservation_digests: vec![PreservationInventory::default().digest.clone()],
            preservation_inventory: PreservationInventory::default(),
            binding_versions: vec![],
            code_epoch: 0,
        };
        vec![
            ("CodeBinding", serde_json::to_value(binding()).unwrap()),
            ("CodeGeneration", serde_json::to_value(g).unwrap()),
            ("CodeLineage", serde_json::to_value(l).unwrap()),
            ("IndexedSource", serde_json::to_value(source).unwrap()),
            ("CodeAttempt", serde_json::to_value(attempt).unwrap()),
            (
                "CodeResetPreview",
                serde_json::to_value(preview.clone()).unwrap(),
            ),
            (
                "CodeResetJournal",
                serde_json::to_value(CodeResetJournal {
                    reset_id: "reset-1".into(),
                    preview_hash: preview.preview_hash.clone(),
                    target_fingerprints: vec![],
                    authorizing_decisions: vec!["fixture".into()],
                    preview,
                    checkpoints: vec!["start".into()],
                    preservation_digests: vec!["e".repeat(64)],
                })
                .unwrap(),
            ),
        ]
    }
    fn accepted(def: &str) -> serde_json::Value {
        serde_json::json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":format!("#/$defs/{def}"),"$defs":schema()["$defs"].clone()})
    }
    #[test]
    fn generated_shapes_match_model() {
        let doc = schema();
        for (name, value) in valid_values() {
            assert!(doc["$defs"].get(name).is_some(), "missing {name}");
            assert!(
                jsonschema::draft202012::validate(&accepted(name), &value).is_ok(),
                "invalid {name}: {value}"
            );
        }
        let wire = serde_json::to_value(binding()).unwrap();
        assert!(wire.get("repoId").is_some());
        assert!(wire.get("repo_id").is_none());
    }
    #[test]
    fn nonempty_preservation_preview_and_journal_match_schema() {
        let mut inventory = PreservationInventory {
            records: [
                "source",
                "document",
                "chunk",
                "entity",
                "relationship",
                "graph",
            ]
            .into_iter()
            .map(|record_class| PreservationRecord {
                record_class: record_class.into(),
                record_id: format!("{record_class}-1"),
                content_digest: "a".repeat(64),
                contributor_digest: "b".repeat(64),
            })
            .collect(),
            digest: String::new(),
        };
        inventory.digest = inventory.content_hash().unwrap();
        let preview = CodeResetPreview {
            binding: binding(),
            preview_hash: "d".repeat(64),
            target_ids: vec!["target".into()],
            targets: vec![],
            ambiguity: vec![],
            preservation_digests: vec![inventory.digest.clone()],
            preservation_inventory: inventory,
            binding_versions: vec![],
            code_epoch: 0,
        };
        preview.validate().unwrap();
        let journal = CodeResetJournal {
            reset_id: "reset-1".into(),
            preview_hash: preview.preview_hash.clone(),
            target_fingerprints: vec![],
            authorizing_decisions: vec!["fixture".into()],
            preview: preview.clone(),
            checkpoints: vec!["start".into()],
            preservation_digests: vec![preview.preservation_inventory.digest.clone()],
        };
        for (name, value, records_path) in [
            (
                "CodeResetPreview",
                serde_json::to_value(preview).unwrap(),
                "/preservationInventory/records",
            ),
            (
                "CodeResetJournal",
                serde_json::to_value(journal).unwrap(),
                "/preview/preservationInventory/records",
            ),
        ] {
            let records = value.pointer(records_path).unwrap().as_array().unwrap();
            assert_eq!(
                records.len(),
                6,
                "{name} must serialize all six preservation classes"
            );
            assert!(
                records
                    .iter()
                    .all(|record| record.get("recordClass").is_some()),
                "{name} must serialize recordClass in camelCase"
            );
            assert!(
                jsonschema::draft202012::validate(&accepted(name), &value).is_ok(),
                "actual nonempty {name} must match the published schema: {value}"
            );
        }
    }
    #[test]
    fn generated_output_reproducibility() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before =
            fs::read(root.join("packages/contracts/src/generated/schema.generated.ts")).unwrap();
        let status = Command::new("pnpm")
            .args(["run", "contracts:generate"])
            .current_dir(&root)
            .status()
            .expect("run contract generator");
        assert!(status.success());
        let after =
            fs::read(root.join("packages/contracts/src/generated/schema.generated.ts")).unwrap();
        assert_eq!(before, after, "generation must be byte reproducible");
    }
    #[test]
    fn breaking_compatibility_no_migration() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = fs::read_to_string(root.join("contracts/v1/compatibility.md")).unwrap();
        assert!(text.contains("breaking behavior change"));
        assert!(text.contains("MUST NOT infer bindings"));
        assert!(!text.contains("contracts/v2/"));
        let mut invalid = binding();
        invalid.branch_ref = "refs/heads/a..b".into();
        assert!(invalid.validate().is_err());
        assert!(
            jsonschema::draft202012::validate(
                &accepted("CodeBinding"),
                &serde_json::to_value(invalid).unwrap()
            )
            .is_err()
        );
    }
}
