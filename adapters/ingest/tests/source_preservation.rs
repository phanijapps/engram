use engram_domain::*;
use engram_ingest::{ScanFilter, ScanOptions, scan_repository};
use engram_knowledge::{CodeSourceRepository, KnowledgeRepository};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use serde_json::json;
use std::fs;

fn opts() -> ScanOptions {
    ScanOptions {
        scope: serde_json::from_value(json!({"tenant":"source-test","workspace":"project"}))
            .unwrap(),
        actor: serde_json::from_value(json!({"id":"owner","kind":"agent"})).unwrap(),
        policy: serde_json::from_value(
            json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}),
        )
        .unwrap(),
        source_name: "source-test".into(),
        max_bytes: 0,
        manifest: Default::default(),
        scan_filter: ScanFilter::default(),
    }
}
fn read(store: &SqlKnowledgeStore, m: &CodeSourceGeneration) -> String {
    block_on(store.read_code_source(
        CodeSourceReadRequest {
            file: m.file.clone(),
            requester:
                serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap(),
            generation: Some(m.generation.clone()),
            max_bytes: 1024 * 1024,
        },
        &m.file.scope,
    ))
    .unwrap()
    .text
}

#[test]
fn scan_preserves_polyglot_source_empty_files_and_incremental_generations() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    let cases = [
        (
            "user.ts",
            "\u{feff}import { Database } from './db';\r\nexport class UserService {\r\n private db: Database;\r\n async getUser(\r\n id: string\r\n ) { const fee=1; function nested() {} return id+fee; }\r\n}\r\n",
        ),
        (
            "main.rs",
            "fn outer() { let fee = 1; fn nested() {} println!(\"{fee}\"); }\n",
        ),
        (
            "app.py",
            "def outer():\n    fee = 1\n    def nested():\n        pass\n    return fee\n",
        ),
        ("empty.ts", ""),
        ("blank.rs", " \t\r\n"),
        ("imports.ts", "import { Database } from './db';\r\n"),
        ("broken.ts", "function broken() { if ("),
        ("legacy.vim", "let value = 42\n"),
    ];
    for (path, text) in cases {
        fs::write(root.path().join(path), text).unwrap();
    }
    let (first, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(first.errors, 0);
    let sources = block_on(store.list_code_sources(&opts.scope)).unwrap();
    assert_eq!(sources.len(), cases.len());
    for (path, text) in cases {
        let source = sources.iter().find(|m| m.file.path == path).unwrap();
        assert_eq!(source.state, CodeSourceState::Active);
        assert_eq!(read(&store, source).as_bytes(), text.as_bytes());
        let result = block_on(store.read_code_source(
            CodeSourceReadRequest {
                file: source.file.clone(),
                requester:
                    serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap(),
                generation: None,
                max_bytes: 1024 * 1024,
            },
            &opts.scope,
        ))
        .unwrap();
        if path == "legacy.vim" {
            assert!(
                result
                    .coverage
                    .iter()
                    .any(|c| c.parse_quality == CodeParseQuality::Fallback
                        && !c.complete
                        && !c.unsupported_features.is_empty())
            );
        }
        if path == "broken.ts" {
            assert!(
                result
                    .coverage
                    .iter()
                    .all(|c| c.parse_quality == CodeParseQuality::Recovered)
            );
        }
        if path == "imports.ts" {
            assert!(
                result
                    .coverage
                    .iter()
                    .any(|c| c.role == CodeChunkRole::Residual)
            );
        }
    }
    opts.manifest = manifest;
    let (second, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(second.unchanged, cases.len());
}

#[test]
fn invalid_utf8_preserves_prior_source_and_removal_retracts_only_that_file() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
    fs::write(root.path().join("b.ts"), "function b() { return 2; }\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    let a = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .into_iter()
        .find(|m| m.file.path == "a.ts")
        .unwrap();
    fs::write(root.path().join("a.ts"), [0xff, 0xfe, 0x61]).unwrap();
    let (invalid, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(invalid.errors, 1);
    assert_eq!(
        invalid.admission_failures[0].kind,
        engram_ingest::ScanAdmissionFailureKind::UnsupportedEncoding
    );
    assert_eq!(read(&store, &a), "function a() { return 1; }\n");
    opts.manifest = manifest;
    fs::remove_file(root.path().join("a.ts")).unwrap();
    let (removed, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(removed.errors, 0);
    assert_eq!(
        block_on(store.get_code_source(&a.file))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Incomplete
    );
    let b = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .into_iter()
        .find(|m| m.file.path == "b.ts")
        .unwrap();
    assert_eq!(read(&store, &b), "function b() { return 2; }\n");
}

#[test]
fn graph_write_failure_is_incomplete_and_retry_converges_without_hiding_sibling() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
    fs::write(root.path().join("b.ts"), "function b() { return 2; }\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    store.lock().unwrap().execute_batch("CREATE TRIGGER fail_a_graph BEFORE INSERT ON knowledge_graphs_records WHEN NEW.path='a.ts' BEGIN SELECT RAISE(ABORT,'injected graph failure'); END;").unwrap();
    fs::write(root.path().join("a.ts"), "function a() { return 3; }\n").unwrap();
    let (failed, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(failed.errors, 1);
    let views = block_on(store.list_code_sources(&opts.scope)).unwrap();
    let a = views.iter().find(|m| m.file.path == "a.ts").unwrap();
    assert_eq!(a.state, CodeSourceState::Incomplete);
    let b = views.iter().find(|m| m.file.path == "b.ts").unwrap();
    assert_eq!(read(&store, b), "function b() { return 2; }\n");
    store
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_a_graph")
        .unwrap();
    opts.manifest = manifest;
    let (retried, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(retried.errors, 0);
    assert_eq!(retried.ingested, 1);
    let a = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .into_iter()
        .find(|m| m.file.path == "a.ts")
        .unwrap();
    assert_eq!(read(&store, &a), "function a() { return 3; }\n");
}

#[test]
fn legacy_sidecar_or_changed_fingerprint_cannot_skip_source_upgrade() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    fs::write(root.path().join("a.ts"), "function a() {}\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    // Simulates a reader/mapping version change while filesystem bytes and
    // the old host sidecar remain unchanged; a fresh scan must requeue it.
    store.lock().unwrap().execute("UPDATE code_source_generations SET record_json=json_set(record_json,'$.fingerprint','legacy-fingerprint')",[]).unwrap();
    let (upgraded, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(upgraded.errors, 0);
    assert_eq!(upgraded.ingested, 1);
    assert_eq!(upgraded.unchanged, 0);
    opts.manifest = manifest;
    let (unchanged, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(unchanged.unchanged, 1);
}

#[test]
fn exact_scope_file_views_remain_independent_for_the_same_repository_and_path() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut first = opts();
    fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &first, &store, |_| {}).unwrap();
    first.manifest = manifest;
    let a = block_on(store.list_code_sources(&first.scope))
        .unwrap()
        .pop()
        .unwrap();
    let mut second = opts();
    second.scope.session = Some("second-session".into());
    let (_, manifest) = scan_repository(root.path(), &second, &store, |_| {}).unwrap();
    second.manifest = manifest;
    fs::write(root.path().join("a.ts"), "function a() { return 2; }\n").unwrap();
    let (changed, _) = scan_repository(root.path(), &second, &store, |_| {}).unwrap();
    assert_eq!(changed.errors, 0);
    let b = block_on(store.list_code_sources(&second.scope))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(read(&store, &a), "function a() { return 1; }\n");
    assert_eq!(read(&store, &b), "function a() { return 2; }\n");
}

#[test]
fn explicit_policy_change_excludes_prior_source_when_new_bytes_are_invalid() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    let prior = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .pop()
        .unwrap();
    opts.policy.visibility = Visibility::Private;
    fs::write(root.path().join("a.ts"), [0xff]).unwrap();
    let (failed, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(failed.errors, 1);
    let current = block_on(store.get_code_source(&prior.file))
        .unwrap()
        .unwrap();
    assert_eq!(current.state, CodeSourceState::Incomplete);
    assert_eq!(current.policy, opts.policy);
    assert!(
        block_on(store.read_code_source(
            CodeSourceReadRequest {
                file: current.file.clone(),
                requester:
                    serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"}})).unwrap(),
                generation: None,
                max_bytes: 1024,
            },
            &opts.scope
        ))
        .is_err()
    );
    opts.manifest = manifest;
    fs::write(root.path().join("a.ts"), "function a() { return 2; }\n").unwrap();
    let (retried, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(retried.errors, 0);
    let current = block_on(store.get_code_source(&prior.file))
        .unwrap()
        .unwrap();
    assert_eq!(read(&store, &current), "function a() { return 2; }\n");
}

#[test]
fn snapshot_and_unit_failures_remain_unavailable_and_retry() {
    for (label, trigger) in [
        (
            "snapshot",
            "CREATE TRIGGER injected BEFORE INSERT ON knowledge_chunks_records WHEN json_extract(NEW.record_json,'$.code.role')='source_snapshot' AND json_extract(NEW.record_json,'$.code.file.path')='a.ts' BEGIN SELECT RAISE(ABORT,'snapshot failure'); END;",
        ),
        (
            "unit",
            "CREATE TRIGGER injected BEFORE INSERT ON knowledge_chunks_records WHEN json_extract(NEW.record_json,'$.code.role')='symbol' AND json_extract(NEW.record_json,'$.code.file.path')='a.ts' BEGIN SELECT RAISE(ABORT,'unit failure'); END;",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let store = SqlKnowledgeStore::open_in_memory().unwrap();
        let mut opts = opts();
        fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
        fs::write(root.path().join("b.ts"), "function b() { return 2; }\n").unwrap();
        let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
        opts.manifest = manifest;
        store.lock().unwrap().execute_batch(trigger).unwrap();
        fs::write(root.path().join("a.ts"), "function a() { return 3; }\n").unwrap();
        let (failed, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
        assert_eq!(failed.errors, 1, "{label}: {failed:?}");
        let views = block_on(store.list_code_sources(&opts.scope)).unwrap();
        let a = views.iter().find(|m| m.file.path == "a.ts").unwrap();
        assert_eq!(a.state, CodeSourceState::Incomplete, "{label}");
        let b = views.iter().find(|m| m.file.path == "b.ts").unwrap();
        assert_eq!(read(&store, b), "function b() { return 2; }\n");
        store
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER injected")
            .unwrap();
        opts.manifest = manifest;
        let (retry, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
        assert_eq!(retry.errors, 0, "{label}: {retry:?}");
        assert_eq!(retry.ingested, 1, "{label}");
        opts.manifest = manifest;
        let (same, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
        assert_eq!(same.unchanged, 2, "{label}");
        let a = block_on(store.list_code_sources(&opts.scope))
            .unwrap()
            .into_iter()
            .find(|m| m.file.path == "a.ts")
            .unwrap();
        assert_eq!(read(&store, &a), "function a() { return 3; }\n");
    }
}

#[test]
fn refresh_preserves_prior_canonical_chunks_without_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    fs::write(root.path().join("a.ts"), "function a() { return 1; }\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    let prior: Vec<(String, String)> = {
        let conn = store.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id,record_json FROM knowledge_chunks_records ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    store.lock().unwrap().execute_batch("CREATE TRIGGER forbid_cleanup BEFORE DELETE ON knowledge_chunks_records BEGIN SELECT RAISE(ABORT,'canonical history must survive refresh'); END;").unwrap();
    fs::write(root.path().join("a.ts"), "function a() { return 3; }\n").unwrap();
    let (summary, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(summary.errors, 0, "{summary:?}");
    assert_eq!(summary.ingested, 1);
    let current = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(read(&store, &current), "function a() { return 3; }\n");
    for (id, expected) in prior {
        let retained: String = store
            .lock()
            .unwrap()
            .query_row(
                "SELECT record_json FROM knowledge_chunks_records WHERE id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(retained, expected, "canonical history changed: {id}");
        assert!(
            block_on(store.get_chunk(&ChunkId::from(id), &opts.scope))
                .unwrap()
                .is_none(),
            "retained history must not be current evidence"
        );
    }
}

#[test]
fn oversize_replacement_does_not_activate_or_destroy_prior_source() {
    let root = tempfile::tempdir().unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let mut opts = opts();
    opts.max_bytes = 64;
    fs::write(root.path().join("a.ts"), "function a() {}\n").unwrap();
    let (_, manifest) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    opts.manifest = manifest;
    let prior = block_on(store.list_code_sources(&opts.scope))
        .unwrap()
        .pop()
        .unwrap();
    fs::write(root.path().join("a.ts"), [b'x'; 4096]).unwrap();
    let (summary, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(summary.ingested, 0);
    assert_eq!(summary.skipped, 1);
    assert_eq!(
        block_on(store.get_code_source(&prior.file))
            .unwrap()
            .unwrap(),
        prior
    );
    assert_eq!(read(&store, &prior), "function a() {}\n");
}

#[test]
fn native_nested_paths_use_portable_source_identity_and_read_back() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let text = "function nestedPath() {}\r\n";
    fs::write(root.path().join("src").join("a.ts"), text).unwrap();
    let store = SqlKnowledgeStore::open_in_memory().unwrap();
    let opts = opts();
    let (summary, _) = scan_repository(root.path(), &opts, &store, |_| {}).unwrap();
    assert_eq!(summary.errors, 0);
    let sources = block_on(store.list_code_sources(&opts.scope)).unwrap();
    assert_eq!(sources[0].file.path, "src/a.ts");
    assert_eq!(read(&store, &sources[0]), text);
}
