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

mod task_010 {
    use super::*;
    use async_trait::async_trait;
    use engram_ingest::{generation_manifest_key, stage_generation_records};
    use engram_knowledge::{CodeGenerationStaging, CoreError, CoreResult};

    struct FailedSourceStore(SqlKnowledgeStore);
    #[async_trait]
    impl KnowledgeRepository for FailedSourceStore {
        async fn put_source(&self, s: KnowledgeSource) -> CoreResult<KnowledgeSource> {
            self.0.put_source(s).await
        }
        async fn put_document(&self, d: SourceDocument) -> CoreResult<SourceDocument> {
            self.0.put_document(d).await
        }
        async fn put_chunk(&self, c: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
            self.0.put_chunk(c).await
        }
        async fn get_chunk(
            &self,
            id: &ChunkId,
            scope: &Scope,
        ) -> CoreResult<Option<KnowledgeChunk>> {
            self.0.get_chunk(id, scope).await
        }
        async fn put_entity(&self, e: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
            self.0.put_entity(e).await
        }
        async fn put_relationship(
            &self,
            r: KnowledgeRelationship,
        ) -> CoreResult<KnowledgeRelationship> {
            self.0.put_relationship(r).await
        }
    }
    #[async_trait]
    impl KnowledgeGraphRepository for FailedSourceStore {
        async fn put_graph(&self, g: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
            self.0.put_graph(g).await
        }
        async fn get_graph(
            &self,
            id: &KnowledgeGraphId,
            scope: &Scope,
        ) -> CoreResult<Option<KnowledgeGraph>> {
            self.0.get_graph(id, scope).await
        }
        async fn neighbors(
            &self,
            id: &KnowledgeGraphId,
            node: &EntityId,
            scope: &Scope,
            limit: Option<u32>,
        ) -> CoreResult<Vec<KnowledgeRelationship>> {
            self.0.neighbors(id, node, scope, limit).await
        }
    }
    #[async_trait]
    impl CodeGenerationStaging for FailedSourceStore {
        async fn stage_lineage(
            &self,
            scope: &Scope,
            id: &str,
            lineage: &CodeLineage,
        ) -> CoreResult<()> {
            self.0.stage_lineage(scope, id, lineage).await
        }
        async fn stage_source(&self, _: &Scope, _: &IndexedSource) -> CoreResult<()> {
            Err(CoreError::Adapter {
                adapter: "fixture-source-stage".into(),
                message: "injected durable source failure".into(),
            })
        }
    }
    #[test]
    fn durable_receipt_order() {
        let d = tempdir().unwrap();
        let db = d.path().join("knowledge.db");
        let b = binding();
        let p = prepare(&b, "stage", &[file("a.rs", "pub fn stored() {}\n")]);
        let failed = FailedSourceStore(SqlKnowledgeStore::open_file(&db).unwrap());
        let root = d.path().join("manifests");
        let key = generation_manifest_key(&b, &p, "attempt").unwrap();
        let error =
            block_on(stage_generation_records(&root, &b, &p, "attempt", &failed)).unwrap_err();
        assert!(
            matches!(error, CoreError::Adapter { adapter, .. } if adapter == "fixture-source-stage")
        );
        assert!(
            !root.join(format!("{key}.json")).exists(),
            "failed stage emitted readiness"
        );
        drop(failed);
        let store = SqlKnowledgeStore::open_file(&db).unwrap();
        let receipt = block_on(stage_generation_records(&root, &b, &p, "attempt", &store)).unwrap();
        drop(store);
        let store = SqlKnowledgeStore::open_file(&db).unwrap();
        let connection = store.shared_connection();
        let conn = connection.lock().unwrap();
        let source_count: usize = conn
            .query_row("SELECT count(*) FROM code_stage_sources", [], |r| r.get(0))
            .unwrap();
        let lineage_count: usize = conn
            .query_row("SELECT count(*) FROM code_stage_lineage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(source_count, p.documents.len());
        assert_eq!(lineage_count, p.lineage.len());
        let saved: engram_knowledge::StageReceipt =
            serde_json::from_slice(&std::fs::read(root.join(format!("{key}.json"))).unwrap())
                .unwrap();
        assert_eq!(saved, receipt);
    }
    #[test]
    fn manifest_binding_keys() {
        let b = binding();
        let files = [file("a.rs", "pub fn same() {}\n")];
        let p = prepare(&b, "stage", &files);
        let base = generation_manifest_key(&b, &p, "attempt").unwrap();
        let mut keys = HashSet::from([base]);
        assert!(keys.insert(generation_manifest_key(&b, &p, "other-attempt").unwrap()));
        let mut other = b.clone();
        other.scope.workspace = Some("b".into());
        assert!(keys.insert(
            generation_manifest_key(&other, &prepare(&other, "stage", &files), "attempt").unwrap()
        ));
        let mut other = b.clone();
        other.repo_id = RepoId("different-repo".into());
        assert!(keys.insert(
            generation_manifest_key(&other, &prepare(&other, "stage", &files), "attempt").unwrap()
        ));
        let mut p = prepare(&b, "different-generation", &files);
        assert!(keys.insert(generation_manifest_key(&b, &p, "attempt").unwrap()));
        p.generation.parser_fingerprint = "changed-parser".into();
        assert!(keys.insert(generation_manifest_key(&b, &p, "attempt").unwrap()));
        p.generation.filter_fingerprint = "changed-filter".into();
        assert!(keys.insert(generation_manifest_key(&b, &p, "attempt").unwrap()));
    }
    #[test]
    fn idempotent_stage_resume() {
        let d = tempdir().unwrap();
        let b = binding();
        let p = prepare(&b, "stage", &[file("a.rs", "pub fn retry() {}\n")]);
        let db = d.path().join("knowledge.db");
        let root = d.path().join("manifests");
        let store = SqlKnowledgeStore::open_file(&db).unwrap();
        let first = block_on(stage_generation_records(&root, &b, &p, "attempt", &store)).unwrap();
        let chunk = block_on(store.get_chunk(&p.documents[0].records.chunks[0].id, &b.scope))
            .unwrap()
            .unwrap();
        drop(store);
        let store = SqlKnowledgeStore::open_file(&db).unwrap();
        let second = block_on(stage_generation_records(&root, &b, &p, "attempt", &store)).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            block_on(store.get_chunk(&chunk.id, &b.scope))
                .unwrap()
                .unwrap(),
            chunk
        );
        let mut conflicting = prepare(&b, "stage", &[file("a.rs", "pub fn replaced() {}\n")]);
        conflicting.generation = p.generation.clone();
        assert!(
            matches!(block_on(stage_generation_records(&root, &b, &conflicting, "attempt", &store)), Err(CoreError::InvalidRequest { reason }) if reason.contains("manifest"))
        );
        assert_eq!(
            block_on(store.get_chunk(&chunk.id, &b.scope))
                .unwrap()
                .unwrap(),
            chunk
        );
    }
    #[test]
    fn no_active_predelete() {
        let d = tempdir().unwrap();
        let b = binding();
        let old = prepare(&b, "old", &[file("old.rs", "pub fn old() {}\n")]);
        let store = SqlKnowledgeStore::open_file(d.path().join("knowledge.db")).unwrap();
        let root = d.path().join("manifests");
        block_on(stage_generation_records(
            &root,
            &b,
            &old,
            "attempt-old",
            &store,
        ))
        .unwrap();
        let old_graph = block_on(store.get_graph(&old.documents[0].graph.graph.id, &b.scope))
            .unwrap()
            .unwrap();
        let new = prepare(&b, "new", &[file("new.rs", "pub fn new() {}\n")]);
        block_on(stage_generation_records(
            &root,
            &b,
            &new,
            "attempt-new",
            &store,
        ))
        .unwrap();
        assert_eq!(
            block_on(store.get_graph(&old_graph.id, &b.scope))
                .unwrap()
                .unwrap(),
            old_graph
        );
        assert!(
            block_on(store.get_chunk(&old.documents[0].records.chunks[0].id, &b.scope))
                .unwrap()
                .is_some()
        );
    }
}
