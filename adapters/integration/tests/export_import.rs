//! Integration tests for `SqlExportImport` (engram-host-sdk brief, S5).
//!
//! These tests exercise the SQLite `ExportImport` impl against in-memory
//! knowledge + memory stores. They mirror the block_on driving style of
//! `tests/batch_ingest.rs` — no tokio.
//!
//! Cases:
//! 1. Export from a seeded scope → `ImportData` with the correct record types
//!    and counts (knowledge sources/documents/chunks/entities/relationships +
//!    memory).
//! 2. Round-trip: export → `MigrationService::dry_run_import` →
//!    `apply_import` → Ok, with the validation row counts matching the export
//!    (parity through the validation-only migration pipeline).
//! 3. Scope isolation: a second tenant's records do not leak into another
//!    scope's export.
//! 4. Empty scope → empty `ImportData`.

use std::sync::Arc;

use engram_conformance::{SqlExportImport, SqlMigrationService};
use engram_domain::*;
use engram_integration::{ExportImport, MigrationManifest, MigrationService as _};
use engram_knowledge::KnowledgeRepository as _;
use engram_memory::MemoryRepository as _;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn scope(tenant: &str) -> Scope {
    Scope {
        tenant: tenant.to_string(),
        subject: Some(format!("subject-{tenant}")),
        workspace: Some(format!("workspace-{tenant}")),
        session: None,
        environment: Some("test".to_string()),
    }
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
        observed_at: chrono::Utc::now(),
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
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
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
        created_at: chrono::Utc::now(),
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
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

fn entity(id: &str, scope: &Scope) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Function,
        name: format!("entity-{id}"),
        aliases: Vec::new(),
        scope: scope.clone(),
        source_refs: Vec::new(),
        concept_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
    }
}

fn relationship(id: &str, scope: &Scope) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("export-entity-a")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("export-entity-b")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope: scope.clone(),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
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
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// Seeds `tenant-a` with one source, one document, one chunk, two entities, one
/// relationship, and one memory record. Returns the wired `SqlExportImport`.
fn seeded_exporter() -> (
    SqlExportImport,
    Arc<SqlKnowledgeStore>,
    Arc<SqlMemoryService>,
) {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let scope_a = scope("tenant-a");

    block_on(knowledge.put_source(source("export-src", &scope_a))).expect("put source");
    block_on(knowledge.put_document(document("export-doc", "export-src"))).expect("put document");
    block_on(knowledge.put_chunk(chunk("export-chunk", "export-doc", "export-src")))
        .expect("put chunk");
    block_on(knowledge.put_entity(entity("export-entity-a", &scope_a))).expect("put entity a");
    block_on(knowledge.put_entity(entity("export-entity-b", &scope_a))).expect("put entity b");
    block_on(knowledge.put_relationship(relationship("export-rel", &scope_a))).expect("put rel");
    block_on(memory.put_memory(memory_record("export-mem", "exported fact", &scope_a)))
        .expect("put memory");

    let exporter = SqlExportImport::new(knowledge.clone(), memory.clone());
    (exporter, knowledge, memory)
}

// ---------- tests ---------------------------------------------------------

#[test]
fn export_reads_seeded_scope_with_correct_counts() {
    let (exporter, _knowledge, _memory) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");

    // Knowledge family.
    assert_eq!(data.knowledge_sources.len(), 1, "one source");
    assert_eq!(data.knowledge_documents.len(), 1, "one document");
    assert_eq!(data.knowledge_chunks.len(), 1, "one chunk");
    assert_eq!(data.knowledge_entities.len(), 2, "two entities");
    assert_eq!(data.knowledge_relationships.len(), 1, "one relationship");
    // Memory family.
    assert_eq!(data.memories.len(), 1, "one memory");
    // Deferred families are empty in v1.
    assert!(data.concept_schemes.is_empty(), "concept schemes deferred");
    assert!(data.concepts.is_empty(), "concepts deferred");
    assert!(data.beliefs.is_empty(), "beliefs deferred");
    assert!(data.hierarchy_nodes.is_empty(), "hierarchy deferred");
    assert!(data.vectors.is_empty(), "vectors not in v1 export");

    // The exported memory carries the seeded content.
    assert_eq!(data.memories[0].content, "exported fact");
    // The source carries its kind + uri.
    assert_eq!(data.knowledge_sources[0].source_type, "filesystem");
    assert_eq!(data.knowledge_sources[0].uri, "file:///repo/export-src");
    // The chunk carries its text.
    assert_eq!(data.knowledge_chunks[0].content, "chunk text export-chunk");
    // The relationship maps subject/object entity ids + predicate.
    assert_eq!(data.knowledge_relationships[0].source_id, "export-entity-a");
    assert_eq!(data.knowledge_relationships[0].target_id, "export-entity-b");
    assert_eq!(data.knowledge_relationships[0].kind, "calls");
}

#[test]
fn export_round_trips_through_migration_service() {
    let (exporter, _knowledge, _memory) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");

    // dry_run over the exported payload — row counts must match the export.
    let migration = SqlMigrationService::new(384);
    let report = migration.dry_run_import(&data).expect("dry_run");
    assert_eq!(report.row_counts.knowledge_sources, 1);
    assert_eq!(report.row_counts.knowledge_documents, 1);
    assert_eq!(report.row_counts.knowledge_chunks, 1);
    assert_eq!(report.row_counts.knowledge_entities, 2);
    assert_eq!(report.row_counts.knowledge_relationships, 1);
    assert_eq!(report.row_counts.memory, 1);
    // Scope translation succeeds (every record carries a non-empty tenant).
    assert_eq!(report.scope_translation.failed_count, 0);

    // apply_import with a fresh manifest succeeds (parity through the
    // validation-only migration pipeline).
    let manifest = MigrationManifest {
        validation_report: report,
        import_data: data,
    };
    migration.apply_import(&manifest).expect("apply");
}

#[test]
fn export_isolates_by_tenant_scope() {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let scope_a = scope("tenant-a");
    let scope_b = scope("tenant-b");

    // Seed one source + memory in each tenant.
    block_on(knowledge.put_source(source("src-a", &scope_a))).expect("put source a");
    block_on(knowledge.put_source(source("src-b", &scope_b))).expect("put source b");
    block_on(memory.put_memory(memory_record("mem-a", "fact a", &scope_a))).expect("put mem a");
    block_on(memory.put_memory(memory_record("mem-b", "fact b", &scope_b))).expect("put mem b");

    let exporter = SqlExportImport::new(knowledge, memory);
    let data_a = block_on(exporter.export(&scope_a)).expect("export a");
    let data_b = block_on(exporter.export(&scope_b)).expect("export b");

    // Each tenant sees only its own records — no cross-tenant leak.
    assert_eq!(data_a.knowledge_sources.len(), 1);
    assert_eq!(data_a.memories.len(), 1);
    assert_eq!(data_a.knowledge_sources[0].id, "src-a");
    assert_eq!(data_a.memories[0].content, "fact a");
    assert_eq!(data_b.knowledge_sources.len(), 1);
    assert_eq!(data_b.memories.len(), 1);
    assert_eq!(data_b.knowledge_sources[0].id, "src-b");
    assert_eq!(data_b.memories[0].content, "fact b");
}

#[test]
fn export_empty_scope_returns_empty_import_data() {
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let exporter = SqlExportImport::new(knowledge, memory);
    let data = block_on(exporter.export(&scope("tenant-empty"))).expect("export");
    assert!(data.memories.is_empty());
    assert!(data.knowledge_sources.is_empty());
    assert!(data.knowledge_entities.is_empty());
}

#[test]
fn export_document_scope_inherited_from_owning_source() {
    // Documents and chunks carry no scope of their own; their scope is resolved
    // from their owning source. A document whose owning source is scope-visible
    // is exported with that source's scope string.
    let (exporter, _knowledge, _memory) = seeded_exporter();
    let scope_a = scope("tenant-a");
    let data = block_on(exporter.export(&scope_a)).expect("export");
    // The document + chunk carry a non-empty scope resolved from the source.
    assert!(
        !data.knowledge_documents[0].scope.is_empty(),
        "document scope must be resolved from owning source"
    );
    assert!(
        data.knowledge_documents[0].scope.contains("tenant-a"),
        "document scope resolves to owning source's tenant"
    );
    assert!(
        data.knowledge_chunks[0].scope.contains("tenant-a"),
        "chunk scope resolves to owning source's tenant"
    );
}
