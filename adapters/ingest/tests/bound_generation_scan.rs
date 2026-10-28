use engram_domain::*;
use engram_ingest::{
    CommittedGenerationFiles, ScanOptions, content_hash,
    git_source::{CommittedFile, ResolvedCommit},
    prepare_generation, write_prepared_generation,
};
use engram_knowledge::{KnowledgeGraphRepository, KnowledgeRepository};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use std::collections::{HashMap, HashSet};
use tempfile::tempdir;

fn binding() -> CodeBinding {
    CodeBinding {
        scope: Scope {
            tenant: "generation-test".into(),
            workspace: Some("a".into()),
            subject: None,
            session: None,
            environment: None,
        },
        repo_id: RepoId("opaque-repository".into()),
        branch_ref: "refs/heads/main".into(),
        version: 1,
        active_generation: None,
        epoch: 0,
    }
}
fn generation(id: &str) -> CodeGeneration {
    CodeGeneration {
        id: id.into(),
        binding_version: 1,
        commit_oid: "a".repeat(40),
        snapshot_digest: "b".repeat(64),
        parser_fingerprint: "parser-1".into(),
        filter_fingerprint: "filter-1".into(),
    }
}
fn options(b: &CodeBinding) -> ScanOptions {
    ScanOptions {
        scope: b.scope.clone(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            allowed_uses: vec![AllowedUse::Retrieval],
            sensitivity: None,
            expires_at: None,
            delete_mode: None,
        },
        actor: Actor {
            id: Id::from("fixture"),
            kind: ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        source_name: "must-not-be-identity".into(),
        max_bytes: 0,
        manifest: HashMap::new(),
        scan_filter: Default::default(),
    }
}
fn file(path: &str, text: &str) -> CommittedFile {
    CommittedFile {
        path: path.into(),
        oid: "c".repeat(40),
        mode: "100644".into(),
        bytes: text.as_bytes().into(),
    }
}
fn prepare(
    b: &CodeBinding,
    id: &str,
    files: &[CommittedFile],
) -> engram_ingest::PreparedGeneration {
    let commit = ResolvedCommit {
        branch_ref: b.branch_ref.clone(),
        oid: "a".repeat(40),
        object_format: "sha1".into(),
    };
    prepare_generation(
        b,
        &generation(id),
        &CommittedGenerationFiles {
            commit: &commit,
            files,
        },
        &options(b),
    )
    .unwrap()
}

mod task_009 {
    use super::*;
    #[test]
    fn generation_ids_do_not_overwrite_active() {
        let scratch = tempdir().unwrap();
        let path = scratch.path().join("knowledge.db");
        let b = binding();
        let old = prepare(&b, "old", &[file("src/lib.rs", "pub fn old_only() {}\n")]);
        let store = SqlKnowledgeStore::open_file(&path).unwrap();
        block_on(write_prepared_generation(&store, &old)).unwrap();
        let old_chunk = &old.documents[0].records.chunks[0];
        let before = block_on(store.get_chunk(&old_chunk.id, &b.scope))
            .unwrap()
            .unwrap();
        let mut authority =
            engram_store_sqlite::knowledge::code_binding::SqlCodeBindingStore::open(&path).unwrap();
        authority.bind(&b).unwrap();
        let attempt = CodeAttempt {
            id: "attempt-old".into(),
            idempotency_key: "old-request".into(),
            binding_version: 1,
            expected_epoch: 0,
            expected_active_generation: None,
            fencing_token: 1,
            state: CodeAttemptState::Ready,
            receipt_kinds: vec![CodeReceiptKind::Knowledge],
        };
        authority
            .begin_attempt(&b.scope, &b.repo_id.0, &old.generation, &attempt)
            .unwrap();
        authority
            .record_receipt(
                &attempt.id,
                "knowledge",
                &content_hash(serde_json::to_vec(&old.lineage).unwrap()),
            )
            .unwrap();
        let bound = authority
            .publish(&b.scope, &b.repo_id.0, &attempt.id)
            .unwrap();
        let staged = prepare(
            &bound,
            "new",
            &[file("src/lib.rs", "pub fn new_only() {}\n")],
        );
        let old_ids: HashSet<_> = old.lineage.keys().collect();
        assert!(staged.lineage.keys().all(|id| !old_ids.contains(id)));
        block_on(write_prepared_generation(&store, &staged)).unwrap();
        assert_eq!(
            block_on(store.get_chunk(&old_chunk.id, &b.scope))
                .unwrap()
                .unwrap(),
            before
        );
        drop(store);
        let reopened = SqlKnowledgeStore::open_file(&path).unwrap();
        assert_eq!(
            block_on(reopened.get_chunk(&old_chunk.id, &b.scope))
                .unwrap()
                .unwrap(),
            before
        );
        assert!(
            block_on(reopened.get_graph(&old.documents[0].graph.graph.id, &b.scope))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            authority.binding(&b.scope, &b.repo_id.0).unwrap().unwrap(),
            bound
        );
        let chunk = &staged.documents[0].records.chunks[0];
        assert!(
            block_on(reopened.get_chunk(&chunk.id, &b.scope))
                .unwrap()
                .unwrap()
                .text
                .contains("new_only")
        );
    }
    #[test]
    fn same_symbol_paths() {
        let b = binding();
        let files = [
            file("one/lib.rs", "pub fn same() {}\n"),
            file("two/lib.rs", "pub fn same() {}\n"),
        ];
        let prepared = prepare(&b, "generation", &files);
        let mut ids = HashSet::new();
        for doc in &prepared.documents {
            let symbol = doc
                .graph
                .entities
                .iter()
                .find(|e| e.name == "same")
                .unwrap();
            assert!(
                ids.insert(symbol.id.clone()),
                "same symbol/path basename merged"
            );
            assert_eq!(
                doc.source.content_digest,
                content_hash(&files[0].bytes).trim_start_matches("sha256:")
            );
            assert_eq!(
                doc.records.source.repository_key.as_deref(),
                Some("opaque-repository")
            );
        }
        let mut other = b.clone();
        other.scope.workspace = Some("other".into());
        let other_stage = prepare(&other, "generation", &files);
        assert!(
            other_stage
                .lineage
                .keys()
                .all(|id| !prepared.lineage.contains_key(id)),
            "scope omitted from identity namespace"
        );
    }
    #[test]
    fn shared_contributor_lineage() {
        let b = binding();
        let prepared = prepare(
            &b,
            "generation",
            &[
                file("a.rs", "pub fn alpha() {}\n"),
                file("b.rs", "pub fn beta() {}\n"),
            ],
        );
        let repository = prepared.documents[0]
            .graph
            .entities
            .iter()
            .find(|e| e.kind == EntityKind::Repository)
            .unwrap();
        let lineage = &prepared.lineage[&repository.id.to_string()];
        let expected: HashSet<_> = prepared
            .documents
            .iter()
            .map(|d| d.records.document.id.to_string())
            .collect();
        assert_eq!(
            lineage.contributors.iter().cloned().collect::<HashSet<_>>(),
            expected
        );
        assert_eq!(lineage.source_path, ".");
        for doc in &prepared.documents {
            assert!(doc.graph.entities.iter().any(|e| e.id == repository.id));
            let row = &prepared.lineage[&doc.records.chunks[0].id.to_string()];
            assert_eq!(row.contributors, vec![doc.records.document.id.to_string()]);
        }
    }
}
