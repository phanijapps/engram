use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, CommittedGenerationFiles, ScanOptions, content_hash,
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
    fn resolves_cross_file_calls_after_registering_every_staged_document() {
        let b = binding();
        let prepared = prepare(
            &b,
            "generation",
            &[
                file("src/caller.rs", "pub fn caller() { target(); }\n"),
                file("src/target.rs", "pub fn target() {}\n"),
            ],
        );
        let target = prepared
            .documents
            .iter()
            .flat_map(|document| &document.graph.entities)
            .find(|entity| entity.name == "target")
            .expect("target entity");
        let call = prepared
            .documents
            .iter()
            .find(|document| document.source.path == "src/caller.rs")
            .expect("caller document")
            .graph
            .relationships
            .iter()
            .find(|relationship| relationship.predicate == "calls")
            .expect("caller relationship");
        assert_eq!(call.object.name.as_deref(), Some("target"));
        assert_eq!(call.object.id.as_ref(), Some(&target.id));
    }
    #[test]
    fn impl_methods_are_entity_only_with_canonical_lineage() {
        let b = binding();
        let prepared = prepare(
            &b,
            "generation",
            &[file(
                "src/query.rs",
                "struct TraversalSpec;\nimpl TraversalSpec {\n    fn blast_radius(&self) {}\n}\n",
            )],
        );
        let document = &prepared.documents[0];
        let method = document
            .graph
            .entities
            .iter()
            .find(|entity| entity.name == "TraversalSpec::blast_radius")
            .expect("implementation method entity");
        let lineage = &prepared.lineage[&method.id.to_string()];
        assert_eq!(lineage.source_path, "src/query.rs");
        assert_eq!(
            lineage.span,
            Some(CanonicalSpan {
                start_line: 3,
                end_line: 3
            })
        );
        assert_eq!(
            lineage.contributors,
            vec![document.records.document.id.to_string()]
        );
        assert!(document.records.chunks.iter().all(|chunk| {
            !chunk
                .entities
                .iter()
                .any(|entity_ref| entity_ref.id.as_ref() == Some(&method.id))
        }));
        let source =
            "struct TraversalSpec;\nimpl TraversalSpec {\n    fn blast_radius(&self) {}\n}\n";
        let candidates = CodeSymbolChunker.chunk_for_ext(source, "rs").unwrap();
        assert_eq!(document.records.chunks.len(), candidates.len());
        for (chunk, candidate) in document.records.chunks.iter().zip(candidates) {
            assert_eq!(
                chunk
                    .location
                    .as_ref()
                    .and_then(|location| location.start_line),
                candidate
                    .location
                    .as_ref()
                    .and_then(|location| location.start_line)
            );
            assert_eq!(
                chunk
                    .location
                    .as_ref()
                    .and_then(|location| location.end_line),
                candidate
                    .location
                    .as_ref()
                    .and_then(|location| location.end_line)
            );
        }
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
    use engram_ingest::{
        generation_manifest_key, stage_generation_records, stage_generation_records_with_options,
    };
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
    fn git(repo: &std::path::Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .env("GIT_AUTHOR_NAME", "Generation Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
            .env("GIT_COMMITTER_NAME", "Generation Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn native_staging_chain_and_idempotent_retry_reestablish_durability() {
        // Real local chain: acquire -> retain -> prepare -> stage with an injected
        // post-rename directory-sync failure -> reopen -> idempotent retry must
        // reestablish the publication durability boundary before readiness.
        let dir = tempdir().unwrap();
        let repo = dir.path().join("source");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        git(&repo, &["init", "-q", "-b", "main", "."]);
        std::fs::write(repo.join("src/lib.rs"), b"pub fn chain() {}\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "chain"]);
        let (commit, files) =
            engram_ingest::git_source::acquire_local_branch(&repo, "refs/heads/main").unwrap();
        let retained = engram_ingest::git_source::retain_snapshot(
            &dir.path().join("retained"),
            commit.clone(),
            &files,
        )
        .unwrap();
        assert!(retained.root.join("src/lib.rs").is_file());
        let b = binding();
        let mut chain_generation = generation("chain");
        chain_generation.commit_oid = commit.oid.clone();
        let prepared = prepare_generation(
            &b,
            &chain_generation,
            &CommittedGenerationFiles {
                commit: &commit,
                files: &files,
            },
            &options(&b),
        )
        .unwrap();
        let db = dir.path().join("knowledge.db");
        let store = SqlKnowledgeStore::open_file(&db).unwrap();
        let root = dir.path().join("manifests");
        let failed = block_on(stage_generation_records_with_options(
            &root,
            &b,
            &prepared,
            "attempt-chain",
            &store,
            true,
            false,
        ))
        .unwrap_err();
        assert!(
            matches!(&failed, CoreError::Adapter { message, .. } if message
                .contains("generation manifest operation failed")),
            "sanitized: {failed:?}"
        );
        let key = generation_manifest_key(&b, &prepared, "attempt-chain").unwrap();
        // The rename completed before the injected durability failure: the manifest
        // exists but readiness was NOT reported.
        let manifest_path = root.join(format!("{key}.json"));
        assert!(manifest_path.exists());
        let receipt = block_on(stage_generation_records_with_options(
            &root,
            &b,
            &prepared,
            "attempt-chain",
            &store,
            false,
            false,
        ))
        .unwrap();
        let reopened: engram_knowledge::StageReceipt =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(receipt, reopened);
        // A further idempotent retry reports readiness without re-staging.
        let again = block_on(stage_generation_records_with_options(
            &root,
            &b,
            &prepared,
            "attempt-chain",
            &store,
            false,
            false,
        ))
        .unwrap();
        assert_eq!(again, receipt);
    }
    fn wait_for_temp(root: &std::path::Path, key: &str) {
        let prefix = format!(".{key}.");
        for _ in 0..500 {
            let held = std::fs::read_dir(root)
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        e.file_name()
                            .to_str()
                            .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".tmp"))
                    })
                })
                .unwrap_or(false);
            if held {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("paused writer never published its temporary manifest");
    }
    #[test]
    fn manifest_writer_exclusion_same_process_allows_one_owner() {
        let dir = tempdir().unwrap();
        let b = binding();
        let p = prepare(&b, "exclusive", &[file("a.rs", "pub fn a() {}\n")]);
        let root = dir.path().join("manifests");
        let writer_db = dir.path().join("writer.db");
        let contender_db = dir.path().join("contender.db");
        let writer_store = SqlKnowledgeStore::open_file(&writer_db).unwrap();
        let contender_store = SqlKnowledgeStore::open_file(&contender_db).unwrap();
        let exclusive_key = generation_manifest_key(
            &binding(),
            &prepare(&binding(), "exclusive", &[file("a.rs", "pub fn a() {}\n")]),
            "attempt-exclusive",
        )
        .unwrap();
        let paused = std::thread::spawn(move || {
            block_on(stage_generation_records_with_options(
                &root,
                &b,
                &p,
                "attempt-exclusive",
                &writer_store,
                false,
                true,
            ))
        });
        // Order the race: only probe once the paused writer provably holds the lock
        // at temporary publication (its key-named temporary exists).
        wait_for_temp(&dir.path().join("manifests"), &exclusive_key);
        match block_on(stage_generation_records_with_options(
            &dir.path().join("manifests"),
            &binding(),
            &prepare(&binding(), "exclusive", &[file("a.rs", "pub fn a() {}\n")]),
            "attempt-exclusive",
            &contender_store,
            false,
            false,
        )) {
            Err(CoreError::Conflict { .. }) => {}
            Ok(_) => panic!("contender staged while the exclusive writer was paused"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
        let writer_receipt = paused.join().unwrap().unwrap();
        let contender_receipt = block_on(stage_generation_records_with_options(
            &dir.path().join("manifests"),
            &binding(),
            &prepare(&binding(), "exclusive", &[file("a.rs", "pub fn a() {}\n")]),
            "attempt-exclusive",
            &contender_store,
            false,
            false,
        ))
        .unwrap();
        assert_eq!(writer_receipt, contender_receipt);
    }
    #[test]
    fn manifest_writer_exclusion_cross_process_crash_and_retry() {
        const CHILD_MODE: &str = "ENGRAM_MANIFEST_LOCK_CHILD";
        const CHILD_ROOT: &str = "ENGRAM_MANIFEST_LOCK_CHILD_ROOT";
        const CHILD_DB: &str = "ENGRAM_MANIFEST_LOCK_CHILD_DB";
        let dir = tempdir().unwrap();
        let root = dir.path().join("manifests");
        let child_db = dir.path().join("child.db");
        if std::env::var_os(CHILD_MODE).is_some() {
            // Child role: stage into the PARENT-provided manifest root under the same
            // key, pausing at temporary publication until crashed or completed.
            let child_root =
                std::path::PathBuf::from(std::env::var_os(CHILD_ROOT).expect("child root env"));
            let child_db =
                std::path::PathBuf::from(std::env::var_os(CHILD_DB).expect("child db env"));
            let b = binding();
            let p = prepare(&b, "crash", &[file("c.rs", "pub fn c() {}\n")]);
            let store = SqlKnowledgeStore::open_file(&child_db).unwrap();
            let _ = block_on(stage_generation_records_with_options(
                &child_root,
                &b,
                &p,
                "attempt-crash",
                &store,
                false,
                true,
            ));
            std::process::exit(0);
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "task_010::manifest_writer_exclusion_cross_process_crash_and_retry",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD_MODE, "1")
            .env(CHILD_ROOT, &root)
            .env(CHILD_DB, &child_db)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let b = binding();
        let p = prepare(&b, "crash", &[file("c.rs", "pub fn c() {}\n")]);
        let parent_db = dir.path().join("parent.db");
        let store = SqlKnowledgeStore::open_file(&parent_db).unwrap();
        // Order the race: probe only after the child provably holds the lock at
        // temporary publication; exactly one owner exists while it is paused.
        let crash_key = generation_manifest_key(&b, &p, "attempt-crash").unwrap();
        wait_for_temp(&root, &crash_key);
        match block_on(stage_generation_records_with_options(
            &root,
            &b,
            &p,
            "attempt-crash",
            &store,
            false,
            false,
        )) {
            Err(CoreError::Conflict { .. }) => {}
            Ok(_) => panic!("parent staged while the paused child owned the lock"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
        // Crash the paused writer: the OS releases the kernel-held lock and its
        // temporary is a stale leftover with no receipt authority.
        child.kill().unwrap();
        let _ = child.wait();
        let receipt = block_on(stage_generation_records_with_options(
            &root,
            &b,
            &p,
            "attempt-crash",
            &store,
            false,
            false,
        ))
        .unwrap();
        assert!(
            root.join(format!(
                "{}.json",
                generation_manifest_key(&b, &p, "attempt-crash").unwrap()
            ))
            .exists()
        );
        // No stale temporary for this key remains after successful staging.
        let key_prefix = format!(
            ".{}.",
            generation_manifest_key(&b, &p, "attempt-crash").unwrap()
        );
        let leftovers = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(&key_prefix) && n.ends_with(".tmp"))
            })
            .count();
        assert_eq!(leftovers, 0, "stale writer temporary was not reclaimed");
        drop(receipt);
    }
}
