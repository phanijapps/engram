//! Authorized export/import must write a real active source, not acknowledge it.
use engram_conformance::{SqlExportImport, SqlMigrationService};
use engram_domain::*;
use engram_integration::{ExportImport, MigrationManifest, MigrationService};
use engram_knowledge::{CodeSourceRepository, CodeSourceWrite, KnowledgeRepository};
use engram_store_sqlite::{SqlKnowledgeStore, SqlMemoryService};
use futures::executor::block_on;
use serde_json::json;
use std::sync::Arc;
fn seeded() -> (
    Arc<SqlKnowledgeStore>,
    CodeSourceGeneration,
    RetrievalRequest,
) {
    let store = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let policy =
        json!({"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]});
    let provenance = json!({"source":"test","actor":{"id":"owner","kind":"agent"},"observedAt":"2026-10-14T00:00:00Z"});
    let hash = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let file = json!({"scope":{"tenant":"t"},"repository":"repo","path":"a.ts"});
    let m:CodeSourceGeneration=serde_json::from_value(json!({"file":file,"generation":"v1","attemptId":"first","sourceHash":hash,"fingerprint":"code-v1","state":"updating","byteLength":5,"documentId":"doc","snapshotIds":["snapshot"],"records":[{"kind":"source","id":"source"},{"kind":"document","id":"doc"},{"kind":"chunk","id":"snapshot"},{"kind":"chunk","id":"unit"},{"kind":"entity","id":"one"},{"kind":"entity","id":"two"},{"kind":"relationship","id":"call"}],"policy":policy,"provenance":provenance})).unwrap();
    block_on(store.begin_code_source(m.clone(), None)).unwrap();
    block_on(store.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Source(serde_json::from_value(json!({"id":"source","kind":"filesystem","scope":m.file.scope,"name":"test","metadata":{"stableSourceKey":"repo"},"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Document(serde_json::from_value(json!({"id":"doc","sourceId":"source","kind":"code","path":"a.ts","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    for (id, role) in [("snapshot", "source_snapshot"), ("unit", "symbol")] {
        block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Chunk(serde_json::from_value(json!({"id":id,"sourceId":"source","documentId":"doc","kind":"code_block","documentKind":"code","text":"hello","contentHash":hash,"policy":policy,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z","code":{"role":role,"file":file,"generation":"v1","sourceHash":hash,"fingerprint":"code-v1","ranges":[{"start":0,"end":5}],"complete":true,"parseQuality":"clean"}})).unwrap()))).unwrap();
    }
    for (id, name) in [("one", "getUser"), ("two", "deleteUser")] {
        block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Entity(serde_json::from_value(json!({"id":id,"kind":"function","name":name,"scope":m.file.scope,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    }
    block_on(store.write_code_record(&m.file,&m.attempt_id,CodeSourceWrite::Relationship(serde_json::from_value(json!({"id":"call","subject":{"id":"one"},"predicate":"calls","object":{"id":"two"},"scope":m.file.scope,"provenance":provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap()))).unwrap();
    let mut active = m.clone();
    active.state = CodeSourceState::Active;
    block_on(store.activate_code_source(active)).unwrap();
    let request=serde_json::from_value(json!({"scope":m.file.scope,"query":"getUser","requester":{"actor":{"id":"owner","kind":"agent"}}})).unwrap();
    (store, m, request)
}

fn requester() -> Requester {
    serde_json::from_value(json!({"actor":{"id":"owner","kind":"agent"},"permissions":["code:source:export","code:source:write","code:source:activate"]})).unwrap()
}

#[test]
fn authorized_transfer_activates_exact_source_and_legacy_paths_cannot_export_it() {
    let (source, m, _) = seeded();
    let memory = Arc::new(SqlMemoryService::open_in_memory().unwrap());
    let export = SqlExportImport::new(source, memory);
    let legacy = block_on(export.export(&m.file.scope)).unwrap();
    assert!(legacy.knowledge_chunks.is_empty());
    assert!(legacy.knowledge_sources.is_empty());
    let data = block_on(export.export_authorized(&requester(), &m.file.scope)).unwrap();
    assert_eq!(data.code_sources.len(), 1);
    let target = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let svc = SqlMigrationService::new(4).with_code_source_target(target.clone());
    assert!(svc.dry_run_import(&data).is_err());
    let validation_report =
        block_on(svc.dry_run_import_authorized(&data, &requester(), &m.file.scope)).unwrap();
    let manifest = MigrationManifest {
        validation_report,
        import_data: data,
    };
    block_on(svc.apply_import_authorized(&manifest, &requester(), &m.file.scope)).unwrap();
    let result = block_on(target.read_code_source(
        CodeSourceReadRequest {
            file: m.file.clone(),
            requester: requester(),
            generation: None,
            max_bytes: 1024,
        },
        &m.file.scope,
    ))
    .unwrap();
    assert_eq!(result.text, "hello");
    let chunks = block_on(engram_integration::KnowledgeQuery::list_chunks_by_document(
        target.as_ref(),
        m.document_id.as_ref().unwrap(),
        &m.file.scope,
    ))
    .unwrap();
    assert_eq!(chunks.len(), 1);
    assert!(
        chunks
            .iter()
            .all(|c| c.code.as_ref().unwrap().role != CodeChunkRole::SourceSnapshot)
    );
    assert_eq!(
        block_on(target.list_chunks(&m.file.scope)).unwrap().len(),
        1
    );
    let mut stale = manifest.clone();
    stale.import_data.code_sources[0].chunks[0].text = "tampered".into();
    assert!(block_on(svc.apply_import_authorized(&stale, &requester(), &m.file.scope)).is_err());
    let mut denied = requester();
    denied.permissions.clear();
    assert!(block_on(export.export_authorized(&denied, &m.file.scope)).is_err());
    assert!(block_on(svc.apply_import_authorized(&manifest, &denied, &m.file.scope)).is_err());
    let mut other = m.file.scope.clone();
    other.session = Some("other".into());
    assert!(
        block_on(svc.dry_run_import_authorized(&manifest.import_data, &requester(), &other))
            .is_err()
    );
}

#[test]
fn failed_import_is_incomplete_and_retryable_and_versions_fail_before_writes() {
    let (source, m, _) = seeded();
    let export = SqlExportImport::new(
        source,
        Arc::new(SqlMemoryService::open_in_memory().unwrap()),
    );
    let data = block_on(export.export_authorized(&requester(), &m.file.scope)).unwrap();
    let target = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let svc = SqlMigrationService::new(4).with_code_source_target(target.clone());
    let mut incompatible = data.clone();
    incompatible.code_sources[0].version = 2;
    assert!(
        block_on(svc.dry_run_import_authorized(&incompatible, &requester(), &m.file.scope))
            .is_err()
    );
    assert!(block_on(target.get_code_source(&m.file)).unwrap().is_none());
    let validation_report =
        block_on(svc.dry_run_import_authorized(&data, &requester(), &m.file.scope)).unwrap();
    let manifest = MigrationManifest {
        validation_report,
        import_data: data,
    };
    target.lock().unwrap().execute_batch("CREATE TRIGGER fail_source_import BEFORE INSERT ON knowledge_chunks_records WHEN json_extract(NEW.record_json,'$.code.role')='source_snapshot' BEGIN SELECT RAISE(ABORT,'injected snapshot failure'); END;").unwrap();
    assert!(block_on(svc.apply_import_authorized(&manifest, &requester(), &m.file.scope)).is_err());
    assert_eq!(
        block_on(target.get_code_source(&m.file))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Incomplete
    );
    assert!(
        block_on(target.list_chunks(&m.file.scope))
            .unwrap()
            .is_empty()
    );
    target
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_source_import")
        .unwrap();
    block_on(svc.apply_import_authorized(&manifest, &requester(), &m.file.scope)).unwrap();
    assert_eq!(
        block_on(target.get_code_source(&m.file))
            .unwrap()
            .unwrap()
            .state,
        CodeSourceState::Active
    );
    assert_eq!(
        block_on(target.list_chunks(&m.file.scope)).unwrap().len(),
        1
    );
}

#[test]
fn transfer_keeps_repository_owned_context_without_file_ownership() {
    let (source, mut m, _) = seeded();
    let root_id = engram_integration::record_key_hash("t\u{1f}\u{1f}\u{1f}\u{1f}\u{1f}repo");
    let root:KnowledgeEntity=serde_json::from_value(json!({"id":format!("repo-{root_id}"),"kind":"repository","name":"repo","scope":m.file.scope,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z","metadata":{"stableSourceKey":"repo"}})).unwrap();
    block_on(source.ensure_repository_context(&m.file, root.clone())).unwrap();
    m.attempt_id = "context-attempt".into();
    m.state = CodeSourceState::Updating;
    m.records.push(CodeSourceRecord {
        kind: CodeSourceRecordKind::Relationship,
        id: "belongs".into(),
    });
    block_on(source.begin_code_source(m.clone(), Some("first"))).unwrap();
    block_on(source.stage_code_records(&m.file, &m.attempt_id, &m.records)).unwrap();
    for (owner, record) in block_on(source.prior_code_records(&m.file, &m.attempt_id)).unwrap() {
        block_on(source.remove_code_record(&m.file, &m.attempt_id, &owner, &record)).unwrap();
    }
    let relation=serde_json::from_value(json!({"id":"belongs","subject":{"id":"one"},"predicate":"belongs_to","object":{"id":root.id},"scope":m.file.scope,"provenance":m.provenance,"createdAt":"2026-10-14T00:00:00Z"})).unwrap();
    block_on(source.write_code_record(
        &m.file,
        &m.attempt_id,
        CodeSourceWrite::Relationship(relation),
    ))
    .unwrap();
    m.state = CodeSourceState::Active;
    block_on(source.activate_code_source(m.clone())).unwrap();
    let export = SqlExportImport::new(
        source,
        Arc::new(SqlMemoryService::open_in_memory().unwrap()),
    );
    let data = block_on(export.export_authorized(&requester(), &m.file.scope)).unwrap();
    assert_eq!(data.code_sources[0].context_entities, vec![root.clone()]);
    let target = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let svc = SqlMigrationService::new(4).with_code_source_target(target.clone());
    let validation_report =
        block_on(svc.dry_run_import_authorized(&data, &requester(), &m.file.scope)).unwrap();
    let manifest = MigrationManifest {
        validation_report,
        import_data: data,
    };
    block_on(svc.apply_import_authorized(&manifest, &requester(), &m.file.scope)).unwrap();
    let restored = block_on(target.get_entity(&root.id, &m.file.scope))
        .unwrap()
        .unwrap();
    assert_eq!(restored, root);
    let mut changed = root.clone();
    changed.provenance.source = "ignored refresh".into();
    block_on(target.ensure_repository_context(&m.file, changed)).unwrap();
    assert_eq!(
        block_on(target.get_entity(&root.id, &m.file.scope))
            .unwrap()
            .unwrap(),
        root
    );
    assert!(
        !block_on(target.get_code_source(&m.file))
            .unwrap()
            .unwrap()
            .records
            .iter()
            .any(|r| r.id == root.id.as_str())
    );
    let mut bad = manifest.import_data.clone();
    bad.code_sources[0].context_entities[0].kind = EntityKind::Function;
    assert!(block_on(svc.dry_run_import_authorized(&bad, &requester(), &m.file.scope)).is_err());
}

#[test]
fn target_grants_do_not_replace_per_input_export_and_private_owner_authority() {
    let (source, m, _) = seeded();
    let export = SqlExportImport::new(
        source,
        Arc::new(SqlMemoryService::open_in_memory().unwrap()),
    );
    let mut data = block_on(export.export_authorized(&requester(), &m.file.scope)).unwrap();
    let bundle = &mut data.code_sources[0];
    bundle.manifest.policy.visibility = Visibility::Private;
    for s in &mut bundle.sources {
        s.policy.visibility = Visibility::Private;
    }
    for d in &mut bundle.documents {
        d.policy.visibility = Visibility::Private;
    }
    for c in &mut bundle.chunks {
        c.policy.visibility = Visibility::Private;
    }
    let target = Arc::new(SqlKnowledgeStore::open_in_memory().unwrap());
    let svc = SqlMigrationService::new(4).with_code_source_target(target.clone());
    let mut no_export = requester();
    no_export.permissions.retain(|p| p != "code:source:export");
    assert!(matches!(
        block_on(svc.dry_run_import_authorized(&data, &no_export, &m.file.scope)),
        Err(engram_runtime::CoreError::PolicyDenied { .. })
    ));
    let mut other = requester();
    other.actor.id = Id::from("other");
    other.permissions.push("code:source:read".into());
    assert!(matches!(
        block_on(svc.dry_run_import_authorized(&data, &other, &m.file.scope)),
        Err(engram_runtime::CoreError::PolicyDenied { .. })
    ));
    let validation_report =
        block_on(svc.dry_run_import_authorized(&data, &requester(), &m.file.scope)).unwrap();
    let manifest = MigrationManifest {
        validation_report,
        import_data: data,
    };
    assert!(matches!(
        block_on(svc.apply_import_authorized(&manifest, &no_export, &m.file.scope)),
        Err(engram_runtime::CoreError::PolicyDenied { .. })
    ));
    assert!(
        block_on(target.list_code_sources(&m.file.scope))
            .unwrap()
            .is_empty()
    );
}
