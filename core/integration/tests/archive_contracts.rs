//! Archive apply must reject unsupported restore even for a fresh valid manifest.
use async_trait::async_trait;
use engram_domain::*;
#[cfg(feature = "sqlite")]
use engram_integration::sqlite::SqlMigrationService;
use engram_integration::{
    ExportImport, ImportData, MemoryImportRecord, MigrationManifest, MigrationService,
    VectorImportRecord,
};
use engram_integration::{
    archive_composition::{ArchiveExport, export_snapshot},
    migration_validation::CanonicalMigrationService,
    storage_composition::{ArchiveInventory, ArchiveSnapshot},
};
use engram_runtime::CoreError;
use futures::executor::block_on;
use std::sync::Arc;

fn empty_data() -> ImportData {
    ImportData {
        memories: vec![],
        knowledge_sources: vec![],
        knowledge_documents: vec![],
        knowledge_chunks: vec![],
        knowledge_entities: vec![],
        knowledge_relationships: vec![],
        concept_schemes: vec![],
        concepts: vec![],
        beliefs: vec![],
        hierarchy_nodes: vec![],
        vectors: vec![],
    }
}
#[test]
#[cfg(feature = "sqlite")]
fn fresh_valid_apply_is_explicitly_unsupported() {
    let service = SqlMigrationService::new(4);
    let data = empty_data();
    let report = service.dry_run_import(&data).unwrap();
    let result = service.apply_import(&MigrationManifest {
        import_data: data,
        validation_report: report,
    });
    assert!(
        matches!(result, Err(CoreError::CapabilityUnsupported { .. })),
        "archive apply must reject unsupported restore, got {result:?}"
    );
}

fn actor() -> Actor {
    Actor {
        id: Id::from("export-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Export Harness".to_string()),
        metadata: None,
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn provenance() -> Provenance {
    Provenance {
        source: "export-test".to_string(),
        actor: actor(),
        observed_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn source(id: &str, scope: &Scope) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(id),
        kind: SourceKind::Filesystem,
        scope: scope.clone(),
        name: format!("source-{id}"),
        uri: Some(format!("file:///repo/{id}")),
        version: None,
        repository_key: None,
        git_ref: None,
        policy: policy(),
        provenance: provenance(),
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}

fn document(id: &str, source_id: &str) -> SourceDocument {
    SourceDocument {
        id: Id::from(id),
        source_id: Id::from(source_id),
        kind: SourceDocumentKind::Markdown,
        uri: None,
        path: Some(format!("docs/{id}.md")),
        title: Some(format!("Document {id}")),
        mime_type: None,
        language: None,
        version: None,
        content_hash: format!("sha256:{id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}

fn chunk(id: &str, document_id: &str, source_id: &str) -> KnowledgeChunk {
    KnowledgeChunk {
        id: Id::from(id),
        document_id: Id::from(document_id),
        source_id: Id::from(source_id),
        kind: KnowledgeChunkKind::Paragraph,
        text: format!("chunk text {id}"),
        summary: None,
        location: None,
        entities: Vec::new(),
        concepts: Vec::new(),
        embedding_refs: Vec::new(),
        content_hash: format!("sha256:{id}"),
        provenance: provenance(),
        policy: policy(),
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}

fn memory_record(id: &str, text: &str, scope: &Scope) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(id),
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: text.to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope.clone(),
        provenance: provenance(),
        policy: policy(),
        status: MemoryStatus::Active,
        links: Vec::new(),
        assertions: Vec::new(),
        verdict: None,
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
        metadata: None,
    }
}

fn concept_scheme(id: &str, scope: &Scope) -> ConceptScheme {
    ConceptScheme {
        id: Id::from(id),
        uri: format!("https://example.com/schemes/{id}"),
        name: format!("scheme-{id}"),
        scope: scope.clone(),
        version: "1.0".to_string(),
        provenance: provenance(),
        policy: policy(),
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
    }
}

fn concept(id: &str, scheme_id: &str) -> Concept {
    Concept {
        id: Id::from(id),
        uri: format!("https://example.com/concepts/{id}"),
        scheme_id: Id::from(scheme_id),
        pref_label: ConceptLabel {
            value: format!("concept-{id}"),
            language: Some("en".to_string()),
        },
        alt_labels: Vec::new(),
        definition: Some("export-test concept".to_string()),
        notation: None,
        status: ConceptStatus::Active,
        provenance: provenance(),
        created_at: "2026-11-01T12:00:00Z".parse().unwrap(),
        updated_at: None,
    }
}

fn full_scope() -> Scope {
    Scope {
        tenant: "tenant".into(),
        subject: Some("owner".into()),
        workspace: Some("workspace".into()),
        session: Some("session".into()),
        environment: Some("test".into()),
    }
}
#[test]
fn canonical_export_filters_full_scope_and_inherits_source_and_scheme_scope() {
    let scope = full_scope();
    let mut snapshot = ArchiveSnapshot::default();
    for (index, dimension) in [
        "same",
        "tenant",
        "subject",
        "workspace",
        "session",
        "environment",
    ]
    .iter()
    .enumerate()
    {
        let mut value = serde_json::to_value(&scope).unwrap();
        if *dimension != "same" {
            value[*dimension] = serde_json::json!("foreign");
        }
        let stored_scope: Scope = serde_json::from_value(value).unwrap();
        let sid = format!("source-{index}");
        let did = format!("document-{index}");
        let scheme_id = format!("scheme-{index}");
        snapshot.sources.push(source(&sid, &stored_scope));
        snapshot.documents.push(document(&did, &sid));
        snapshot
            .chunks
            .push(chunk(&format!("chunk-{index}"), &did, &sid));
        snapshot.memories.push(memory_record(
            &format!("memory-{index}"),
            "existing v1 content",
            &stored_scope,
        ));
        snapshot
            .concept_schemes
            .push(concept_scheme(&scheme_id, &stored_scope));
        snapshot.concepts.push((
            concept(&format!("concept-{index}"), &scheme_id),
            stored_scope,
        ));
    }
    let exported = export_snapshot(snapshot, &scope).unwrap();
    assert_eq!(exported.memories.len(), 1);
    assert_eq!(exported.memories[0].id, "memory-0");
    assert_eq!(exported.knowledge_sources.len(), 1);
    assert_eq!(exported.knowledge_documents.len(), 1);
    assert_eq!(exported.knowledge_chunks.len(), 1);
    assert_eq!(exported.concept_schemes.len(), 1);
    assert_eq!(exported.concepts.len(), 1);
    for encoded in [
        &exported.memories[0].scope,
        &exported.knowledge_sources[0].scope,
        &exported.knowledge_documents[0].scope,
        &exported.knowledge_chunks[0].scope,
        &exported.concept_schemes[0].scope,
        &exported.concepts[0].scope,
    ] {
        assert_eq!(serde_json::from_str::<Scope>(encoded).unwrap(), scope);
    }
    // These are existing v1 omissions, not a promise of lossless restore.
    assert!(exported.knowledge_documents[0].content.is_empty());
    assert_eq!(exported.knowledge_chunks[0].sequence, 0);
    assert!(exported.concept_schemes[0].description.is_none());
    assert_eq!(exported.concepts[0].metadata, "{}");
    assert!(exported.vectors.is_empty());
}

struct FailedInventory;
#[async_trait]
impl ArchiveInventory for FailedInventory {
    async fn snapshot(&self, _: &Scope) -> engram_runtime::CoreResult<ArchiveSnapshot> {
        Err(CoreError::PolicyDenied {
            reason: "inventory denied".into(),
        })
    }
}
#[test]
fn archive_inventory_failure_is_not_an_empty_success() {
    let exporter = ArchiveExport::new(Arc::new(FailedInventory));
    assert!(matches!(
        block_on(exporter.export(&full_scope())),
        Err(CoreError::PolicyDenied { .. })
    ));
}
#[test]
fn neutral_empty_export_preserves_empty_families() {
    let data = export_snapshot(ArchiveSnapshot::default(), &full_scope()).unwrap();
    for family in serde_json::to_value(data)
        .unwrap()
        .as_object()
        .unwrap()
        .values()
    {
        assert!(family.as_array().unwrap().is_empty());
    }
}
#[test]
fn dry_run_keeps_key_fingerprints_and_existing_dimension_and_empty_tenant_errors() {
    let service = CanonicalMigrationService::new(4);
    let mut data = empty_data();
    data.memories.push(MemoryImportRecord {
        id: "memory".into(),
        scope: "tenant".into(),
        content: "before".into(),
        timestamp: 1,
        policy: "{}".into(),
    });
    let first = service.dry_run_import(&data).unwrap();
    data.memories[0].content = "different content, same legacy key".into();
    let second = service.dry_run_import(&data).unwrap();
    assert_eq!(first.manifest_fingerprint, second.manifest_fingerprint);
    data.memories[0].id = "different-id".into();
    assert_ne!(
        first.manifest_fingerprint,
        service.dry_run_import(&data).unwrap().manifest_fingerprint
    );
    data.memories[0].scope = " ".into();
    data.vectors.push(VectorImportRecord {
        target_id: "vector".into(),
        target_type: "chunk".into(),
        embedding: vec![0.1],
        embedding_space: "fixture".into(),
        model: "none".into(),
        content_hash: "hash".into(),
    });
    let invalid = service.dry_run_import(&data).unwrap();
    assert_eq!(invalid.row_counts.memory, 1);
    assert_eq!(invalid.row_counts.vectors, 1);
    assert_eq!(invalid.scope_translation.failed_count, 1);
    assert!(!invalid.embedding_space_validation.is_valid);
    assert_eq!(invalid.unsupported_mappings[0].reason, "dimension_mismatch");
    for report in [first, invalid] {
        assert!(matches!(
            service.apply_import(&MigrationManifest {
                validation_report: report,
                import_data: data.clone()
            }),
            Err(CoreError::CapabilityUnsupported { .. })
        ));
    }
}
