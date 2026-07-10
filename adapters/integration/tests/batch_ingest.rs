//! Integration tests for `SqlBatchIngest` (engram-host-sdk brief, S3).
//!
//! These tests exercise the SQLite `BatchIngest` impl against in-memory memory
//! and knowledge stores. A well-formed batch lands every writable record and is
//! Complete; N=3 distinct memories under one batch key all land (the
//! per-record-key derivation guard); re-ingest reports the Facts step
//! Deduplicated and knowledge steps Succeeded; a failing step reports Failed
//! while the others still land (Partial). They mirror the block_on driving
//! style of `tests/provenance_query.rs` — no tokio.

use std::sync::Arc;

use engram_conformance::SqlBatchIngest;
use engram_domain::*;
use engram_integration::{
    BatchIngest, BatchIngestRequest, BatchStatus, BatchStep, StepStatus, TransactionGuarantee,
};
use engram_memory::MemoryEventRepository;
use engram_runtime::CoreError;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "tenant-batch".to_string(),
        subject: Some("subject-batch".to_string()),
        workspace: Some("workspace-batch".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("batch-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Batch Harness".to_string()),
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
        source: "batch-test".to_string(),
        actor: actor(),
        observed_at: chrono::Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some("manual".to_string()),
    }
}

fn memory_record(text: &str, suffix: &str) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(format!("batch-fact-{suffix}")),
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
        scope: scope(),
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

fn entity(id: &str) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Function,
        name: format!("entity-{id}"),
        aliases: Vec::new(),
        scope: scope(),
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

fn relationship(id: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from("batch-entity-0")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("batch-entity-1")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope: scope(),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

fn source(id: &str) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(id),
        kind: SourceKind::Filesystem,
        scope: scope(),
        name: format!("source-{id}"),
        uri: None,
        version: None,
        policy: policy(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// Builds a fresh in-memory memory + knowledge store pair with a `SqlBatchIngest`.
fn seeded() -> (
    Arc<SqlMemoryService>,
    Arc<SqlKnowledgeStore>,
    SqlBatchIngest,
) {
    let memory = Arc::new(SqlMemoryService::open_in_memory().expect("memory open"));
    let knowledge = Arc::new(SqlKnowledgeStore::open_in_memory().expect("knowledge open"));
    let batch = SqlBatchIngest::new(memory.clone(), knowledge.clone());
    (memory, knowledge, batch)
}

/// Finds a step's outcome in the batch result by step kind.
fn step(
    outcome: &engram_integration::BatchOutcome,
    step: BatchStep,
) -> &engram_integration::StepOutcome {
    outcome
        .steps
        .iter()
        .find(|o| o.step == step)
        .unwrap_or_else(|| panic!("step {step:?} missing from outcome"))
}

// ---------- tests ---------------------------------------------------------

#[test]
fn well_formed_batch_is_complete_with_evidence_embeddings_skipped() {
    let (memory, knowledge, batch) = seeded();
    let request = BatchIngestRequest {
        idempotency_key: "batch-ok".to_string(),
        scope: scope(),
        source: Some(source("src-ok")),
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![memory_record("first fact lands", "0")],
        entities: vec![entity("batch-entity-0"), entity("batch-entity-1")],
        relationships: vec![relationship("batch-rel-0")],
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };

    let outcome = block_on(batch.ingest(request)).expect("ingest");
    assert_eq!(outcome.guarantee, TransactionGuarantee::BestEffort);
    assert_eq!(outcome.status, BatchStatus::Complete);
    // Writable steps succeeded.
    assert_eq!(
        step(&outcome, BatchStep::Episode).status,
        StepStatus::Succeeded
    );
    assert_eq!(
        step(&outcome, BatchStep::Facts).status,
        StepStatus::Succeeded
    );
    assert_eq!(
        step(&outcome, BatchStep::Entities).status,
        StepStatus::Succeeded
    );
    assert_eq!(
        step(&outcome, BatchStep::Relationships).status,
        StepStatus::Succeeded
    );
    // Evidence + Embeddings are honestly Skipped in v1.
    assert_eq!(
        step(&outcome, BatchStep::Evidence).status,
        StepStatus::Skipped
    );
    assert_eq!(
        step(&outcome, BatchStep::Embeddings).status,
        StepStatus::Skipped
    );

    // The records actually landed in the stores.
    let events = block_on(memory.list_events_for_scope(&scope())).expect("list events");
    let written: Vec<_> = events
        .iter()
        .filter(|e| e.kind == MemoryEventKind::Written)
        .collect();
    assert_eq!(written.len(), 1, "one fact written");
    let entities = block_on(knowledge.list_entities(&scope())).expect("list entities");
    assert_eq!(entities.len(), 2, "both entities landed");
    let rels = block_on(knowledge.list_relationships(&scope())).expect("list relationships");
    assert_eq!(rels.len(), 1, "relationship landed");
    let sources = block_on(knowledge.list_sources(&scope())).expect("list sources");
    assert_eq!(sources.len(), 1, "source landed");
}

#[test]
fn three_distinct_memories_under_one_batch_key_all_land() {
    // Critical guard: memory's idempotency lookup is (tenant, subject,
    // workspace, key) with NO per-record disambiguation. Reusing one batch key
    // per write would dedupe records 2..N against record 1 (data loss). The
    // `{batch_key}#{index}` derivation must make all three keys distinct.
    let (memory, _knowledge, batch) = seeded();
    let request = BatchIngestRequest {
        idempotency_key: "batch-triple".to_string(),
        scope: scope(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![
            memory_record("fact the first", "0"),
            memory_record("fact the second", "1"),
            memory_record("fact the third", "2"),
        ],
        entities: Vec::new(),
        relationships: Vec::new(),
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };

    let outcome = block_on(batch.ingest(request)).expect("ingest");
    assert_eq!(outcome.status, BatchStatus::Complete);
    assert_eq!(
        step(&outcome, BatchStep::Facts).status,
        StepStatus::Succeeded
    );

    // All three landed: three distinct Written events with distinct memory ids.
    let events = block_on(memory.list_events_for_scope(&scope())).expect("list events");
    let written: Vec<_> = events
        .iter()
        .filter(|e| e.kind == MemoryEventKind::Written)
        .collect();
    assert_eq!(
        written.len(),
        3,
        "all three distinct memories must land — per-record key derivation guard"
    );
    let mut memory_ids: Vec<_> = written.iter().filter_map(|e| e.memory_id.clone()).collect();
    memory_ids.sort();
    memory_ids.dedup();
    assert_eq!(
        memory_ids.len(),
        3,
        "three distinct memory ids — none deduplicated away"
    );
}

#[test]
fn reingest_deduplicates_facts_and_succeeds_knowledge() {
    let (_memory, knowledge, batch) = seeded();
    let make_request = || BatchIngestRequest {
        idempotency_key: "batch-reingest".to_string(),
        scope: scope(),
        source: Some(source("src-reingest")),
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![memory_record("reingestable fact", "0")],
        entities: vec![entity("reingest-entity")],
        relationships: vec![relationship("reingest-rel")],
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };

    // First ingest: everything fresh.
    let first = block_on(batch.ingest(make_request())).expect("first ingest");
    assert_eq!(first.status, BatchStatus::Complete);
    assert_eq!(step(&first, BatchStep::Facts).status, StepStatus::Succeeded);

    // Re-ingest the identical batch: Facts Deduplicated (memory key dedup),
    // knowledge steps Succeeded (upsert overwrote, NOT Deduplicated), overall
    // Complete.
    let second = block_on(batch.ingest(make_request())).expect("re-ingest");
    assert_eq!(second.guarantee, TransactionGuarantee::BestEffort);
    assert_eq!(second.status, BatchStatus::Complete);
    assert_eq!(
        step(&second, BatchStep::Facts).status,
        StepStatus::Deduplicated,
        "memory key dedup → Facts Deduplicated on re-ingest"
    );
    assert_eq!(
        step(&second, BatchStep::Entities).status,
        StepStatus::Succeeded,
        "knowledge upsert overwrote → Succeeded, not Deduplicated"
    );
    assert_eq!(
        step(&second, BatchStep::Relationships).status,
        StepStatus::Succeeded
    );

    // Knowledge records are still present (upsert did not lose them).
    let entities = block_on(knowledge.list_entities(&scope())).expect("list entities");
    assert_eq!(entities.len(), 1);
}

#[test]
fn failing_step_records_typed_error_others_land_overall_partial() {
    let (_memory, knowledge, batch) = seeded();
    // The Facts step fails deterministically: an empty content.text is rejected
    // by write_memory's validation. Entities + Relationships still land.
    let mut bad_fact = memory_record("will be blanked", "bad");
    bad_fact.content.text = String::new(); // validate_write_request rejects empty text
    let request = BatchIngestRequest {
        idempotency_key: "batch-partial".to_string(),
        scope: scope(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![bad_fact],
        entities: vec![entity("partial-entity")],
        relationships: vec![relationship("partial-rel")],
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };

    let outcome = block_on(batch.ingest(request)).expect("ingest");
    assert_eq!(outcome.guarantee, TransactionGuarantee::BestEffort);
    assert_eq!(outcome.status, BatchStatus::Partial);

    // The Facts step failed with a typed error.
    let facts = step(&outcome, BatchStep::Facts);
    assert_eq!(facts.status, StepStatus::Failed);
    let error = facts
        .error
        .as_ref()
        .expect("failed step carries a typed CoreError");
    match error {
        CoreError::InvalidRequest { .. } => {}
        other => panic!("expected InvalidRequest from empty-text validation, got {other:?}"),
    }

    // The other steps still landed (best-effort: no rollback).
    assert_eq!(
        step(&outcome, BatchStep::Entities).status,
        StepStatus::Succeeded
    );
    assert_eq!(
        step(&outcome, BatchStep::Relationships).status,
        StepStatus::Succeeded
    );
    let entities = block_on(knowledge.list_entities(&scope())).expect("list entities");
    assert_eq!(
        entities.len(),
        1,
        "entities landed despite the Facts step failing"
    );
}

#[test]
fn empty_writable_steps_report_skipped() {
    let (_memory, _knowledge, batch) = seeded();
    // A batch with only facts: Episode/Entities/Relationships are empty → Skipped.
    let request = BatchIngestRequest {
        idempotency_key: "batch-facts-only".to_string(),
        scope: scope(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![memory_record("only facts", "0")],
        entities: Vec::new(),
        relationships: Vec::new(),
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    let outcome = block_on(batch.ingest(request)).expect("ingest");
    assert_eq!(outcome.status, BatchStatus::Complete);
    assert_eq!(
        step(&outcome, BatchStep::Episode).status,
        StepStatus::Skipped
    );
    assert_eq!(
        step(&outcome, BatchStep::Facts).status,
        StepStatus::Succeeded
    );
    assert_eq!(
        step(&outcome, BatchStep::Entities).status,
        StepStatus::Skipped
    );
    assert_eq!(
        step(&outcome, BatchStep::Relationships).status,
        StepStatus::Skipped
    );
}
