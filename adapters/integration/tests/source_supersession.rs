//! Six-case supersession matrix + regressions for sourced batches
//! (job 001-kg-source-provenance).
//!
//! All cases run REAL in-memory SQLite stores (memory + knowledge) through
//! `SqlBatchIngest` — the same conventions as `batch_ingest.rs`. The only
//! port-level stub is the injected-failure wrapper (TEST-010), which delegates
//! to a real store and fails one operation to prove honest partial-failure
//! reporting and retry convergence.
//!
//! Matrix (AC-005):
//! - TEST-001 case 1 — first sourced ingest: refs stamped, ledger bound,
//!   registry source queryable.
//! - TEST-002 case 2 — same-payload retry converges (dedup + ref merge, no
//!   duplicates).
//! - TEST-003 case 3 — revision (changed fact + removed relationship)
//!   converges to the declared set; archive not delete; tombstone events.
//! - TEST-004 case 4 — reordered identical facts converge (ledger-id
//!   matching, never `{batch_key}#index` key equality).
//! - TEST-005 case 5 — two-doc overlap ref-counts shared records.
//! - TEST-006 case 6 — retraction idempotency (second pass zero counts) +
//!   re-assert-after-archive restore.
//! Regressions: TEST-007 unsourced legacy behavior; TEST-008 code-entity
//! immunity (ref membership, never name); TEST-010 injected partial failure;
//! TEST-013 cross-scope isolation + empty source_id treated as unsourced;
//! review-r1 FIND-security-001 facts immunity (a foreign record adopted
//! through a per-record idempotency-key collision is never forgotten).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use engram_conformance::SqlBatchIngest;
use engram_domain::*;
use engram_integration::sqlite::batch_supersession::run_relationships_step;
use engram_integration::{
    BatchIngest, BatchIngestRequest, BatchOutcome, BatchStep, StepStatus, SupersessionCounts,
};
use engram_knowledge::{KnowledgeRepository, SourceBatchLedger, SourceBatchRecordKind};
use engram_memory::{MemoryEventRepository, MemoryRepository};
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::{SqlKnowledgeStore, SqlMemoryService};
use futures::executor::block_on;

// ---------- helpers -------------------------------------------------------

fn scope() -> Scope {
    Scope {
        tenant: "tenant-supersede".to_string(),
        subject: Some("subject-supersede".to_string()),
        workspace: Some("workspace-supersede".to_string()),
        session: None,
        environment: Some("test".to_string()),
    }
}

fn other_scope() -> Scope {
    Scope {
        workspace: Some("workspace-other".to_string()),
        ..scope()
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("supersede-agent"),
        kind: ActorKind::Agent,
        display_name: Some("Supersede Harness".to_string()),
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
        source: "supersede-test".to_string(),
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
        id: Id::from(format!("supersede-fact-{suffix}")),
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
    entity_with_refs(id, Vec::new())
}

fn entity_with_refs(id: &str, source_refs: Vec<EvidenceRef>) -> KnowledgeEntity {
    KnowledgeEntity {
        id: Id::from(id),
        graph_id: None,
        kind: EntityKind::Concept,
        name: format!("entity-{id}"),
        aliases: Vec::new(),
        scope: scope(),
        source_refs,
        concept_refs: Vec::new(),
        ontology_class_refs: Vec::new(),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        valid_from: None,
        valid_until: None,
        metadata: None,
        archived_at: None,
    }
}

fn relationship(id: &str, subject: &str, predicate: &str, object: &str) -> KnowledgeRelationship {
    KnowledgeRelationship {
        id: Id::from(id),
        graph_id: None,
        subject: EntityRef {
            id: Some(Id::from(subject)),
            kind: None,
            name: Some(subject.to_string()),
            aliases: Vec::new(),
        },
        predicate: predicate.to_string(),
        code_resolution: None,
        object: EntityRef {
            id: Some(Id::from(object)),
            kind: None,
            name: Some(object.to_string()),
            aliases: Vec::new(),
        },
        scope: scope(),
        evidence: Vec::new(),
        confidence: Some(0.9),
        provenance: provenance(),
        created_at: chrono::Utc::now(),
        updated_at: None,
        archived_at: None,
    }
}

/// A sourced batch request with the given payload slices.
fn sourced_request(
    batch_key: &str,
    source_id: &str,
    facts: Vec<MemoryRecord>,
    entities: Vec<KnowledgeEntity>,
    relationships: Vec<KnowledgeRelationship>,
) -> BatchIngestRequest {
    BatchIngestRequest {
        idempotency_key: batch_key.to_string(),
        scope: scope(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts,
        entities,
        relationships,
        evidence: Vec::new(),
        embeddings: Vec::new(),
        source_id: Some(source_id.to_string()),
    }
}

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

fn counts_of(outcome: &BatchOutcome, step: BatchStep) -> SupersessionCounts {
    let found = outcome
        .steps
        .iter()
        .find(|s| s.step == step)
        .unwrap_or_else(|| panic!("step {step:?} missing"));
    found.supersession.clone().unwrap_or_else(|| {
        panic!("step {step:?} must carry supersession counts for a sourced batch")
    })
}

fn step_status(outcome: &BatchOutcome, step: BatchStep) -> &StepStatus {
    &outcome
        .steps
        .iter()
        .find(|s| s.step == step)
        .unwrap_or_else(|| panic!("step {step:?} missing"))
        .status
}

/// Memory ids that have a Written event and no Forgotten event (tombstones
/// keep the record but mark it forgotten — the honest "inactive fact"
/// observable without a status-scoped list API).
fn active_memory_ids(memory: &SqlMemoryService, scope: &Scope) -> Vec<Id> {
    let events = block_on(memory.list_events_for_scope(scope)).expect("list events");
    let written: HashSet<Id> = events
        .iter()
        .filter(|e| e.kind == MemoryEventKind::Written)
        .filter_map(|e| e.memory_id.clone())
        .collect();
    let forgotten: HashSet<Id> = events
        .iter()
        .filter(|e| e.kind == MemoryEventKind::Forgotten)
        .filter_map(|e| e.memory_id.clone())
        .collect();
    let mut active: Vec<Id> = written.difference(&forgotten).cloned().collect();
    active.sort();
    active
}

/// Sorted text of every active memory record in scope.
fn active_fact_texts(memory: &SqlMemoryService, scope: &Scope) -> Vec<String> {
    let mut texts: Vec<String> = active_memory_ids(memory, scope)
        .into_iter()
        .map(|id| {
            let record = block_on(memory.get_memory(&id, scope))
                .expect("get memory")
                .expect("active id has a record");
            record.content.text
        })
        .collect();
    texts.sort();
    texts
}

/// Ledger rows (unsuperseded) for a source, filtered by kind.
fn ledger_rows(
    knowledge: &SqlKnowledgeStore,
    scope: &Scope,
    source_id: &str,
    kind: SourceBatchRecordKind,
) -> Vec<(String, String)> {
    block_on(knowledge.list_source_batch_records(scope, source_id))
        .expect("ledger list")
        .into_iter()
        .filter(|r| r.kind == kind)
        .map(|r| (r.batch_key, r.record_id))
        .collect()
}

fn source_ref(source_id: &str) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Source,
        target_id: Some(source_id.to_string()),
        uri: None,
        quote: None,
        location: None,
    }
}

fn active_entity_ids(knowledge: &SqlKnowledgeStore, scope: &Scope) -> Vec<String> {
    let mut ids: Vec<String> = block_on(knowledge.list_entities(scope))
        .expect("list entities")
        .into_iter()
        .map(|e| e.id.to_string())
        .collect();
    ids.sort();
    ids
}

fn active_relationship_ids(knowledge: &SqlKnowledgeStore, scope: &Scope) -> Vec<String> {
    let mut ids: Vec<String> = block_on(knowledge.list_relationships(scope))
        .expect("list relationships")
        .into_iter()
        .map(|r| r.id.to_string())
        .collect();
    ids.sort();
    ids
}

// ---------- matrix case 1 (TEST-001 / AC-001) ------------------------------

#[test]
fn case_1_first_sourced_ingest_stamps_refs_and_ledgers() {
    let (memory, knowledge, batch) = seeded();
    let outcome = block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![memory_record("The flange is aluminum", "0")],
        vec![entity("Widget")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    )))
    .expect("ingest");

    assert_eq!(
        step_status(&outcome, BatchStep::Episode),
        &StepStatus::Succeeded
    );
    assert_eq!(
        step_status(&outcome, BatchStep::Facts),
        &StepStatus::Succeeded
    );
    // First ingest: counts present, all zero.
    let facts_counts = counts_of(&outcome, BatchStep::Facts);
    assert_eq!(facts_counts, SupersessionCounts::default());
    assert_eq!(
        counts_of(&outcome, BatchStep::Entities),
        SupersessionCounts::default()
    );

    // Entities carry the source ref; queryable by source.
    let by_source =
        block_on(knowledge.list_entities_by_source_ref("docs/x.md", &scope())).expect("by ref");
    assert_eq!(by_source.len(), 1, "entity stamped + queryable by source");
    assert!(by_source[0].source_refs.contains(&source_ref("docs/x.md")));

    // Relationships carry the evidence ref.
    let rels = block_on(knowledge.list_relationships_by_source_ref("docs/x.md", &scope()))
        .expect("rels by ref");
    assert_eq!(rels.len(), 1);
    assert!(rels[0].evidence.contains(&source_ref("docs/x.md")));

    // Facts carry the Provenance.evidence ref (engine-assigned id).
    let active = active_memory_ids(&memory, &scope());
    assert_eq!(active.len(), 1);
    let record = block_on(memory.get_memory(&active[0], &scope()))
        .expect("get")
        .expect("present");
    assert!(
        record
            .provenance
            .evidence
            .contains(&source_ref("docs/x.md")),
        "fact provenance carries the source ref"
    );

    // The ledger binds (scope, source, batch key) to the written ids,
    // including the engine-assigned memory id the caller never sees.
    let fact_rows = ledger_rows(
        &knowledge,
        &scope(),
        "docs/x.md",
        SourceBatchRecordKind::MemoryRecord,
    );
    assert_eq!(fact_rows.len(), 1);
    assert_eq!(fact_rows[0].0, "doc-x@v1");
    assert_eq!(fact_rows[0].1, active[0].to_string());
    let entity_rows = ledger_rows(
        &knowledge,
        &scope(),
        "docs/x.md",
        SourceBatchRecordKind::Entity,
    );
    assert_eq!(entity_rows.len(), 1);
    assert_eq!(entity_rows[0].1, "Widget");

    // Registry KnowledgeSource visible in list_sources (queryable-by-source).
    let sources = block_on(knowledge.list_sources(&scope())).expect("list sources");
    assert!(
        sources.iter().any(|s| s.id.to_string() == "docs/x.md"),
        "registry source landed"
    );
}

// ---------- matrix case 2 (TEST-002 / AC-001, NFR-001) ---------------------

#[test]
fn case_2_retry_same_payload_converges() {
    let (memory, knowledge, batch) = seeded();
    let request = sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![
            memory_record("Retry fact", "0"),
            memory_record("Second fact", "1"),
        ],
        vec![entity("Widget")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    );
    let first = block_on(batch.ingest(request.clone())).expect("first");
    let first_ids = active_memory_ids(&memory, &scope());

    let second = block_on(batch.ingest(request)).expect("retry");

    // Facts deduped; zero supersession counts on the retry.
    assert_eq!(
        step_status(&second, BatchStep::Facts),
        &StepStatus::Deduplicated
    );
    assert_eq!(
        counts_of(&second, BatchStep::Facts),
        SupersessionCounts::default()
    );
    assert_eq!(
        counts_of(&second, BatchStep::Entities),
        SupersessionCounts::default()
    );

    // No new records; identical active set.
    assert_eq!(active_memory_ids(&memory, &scope()), first_ids);
    assert_eq!(active_fact_texts(&memory, &scope()).len(), 2);

    // Refs merged, not duplicated: exactly one source ref on the entity.
    let stamped = block_on(knowledge.get_entity(&Id::from("Widget"), &scope()))
        .expect("get")
        .expect("present");
    let matching = stamped
        .source_refs
        .iter()
        .filter(|r| *r == &source_ref("docs/x.md"))
        .count();
    assert_eq!(matching, 1, "ref union is idempotent — no duplicate refs");

    // One ledger row per record per batch key (upsert, not accumulation).
    assert_eq!(
        ledger_rows(
            &knowledge,
            &scope(),
            "docs/x.md",
            SourceBatchRecordKind::MemoryRecord
        )
        .len(),
        2
    );
}

// ---------- matrix case 3 (TEST-003 / AC-002, NFR-006) ---------------------

#[test]
fn case_3_revision_changed_fact_removed_relationship_converges() {
    let (memory, knowledge, batch) = seeded();
    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![memory_record("The flange is aluminum", "0")],
        vec![entity("Widget"), entity("Factory")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    )))
    .expect("v1");
    let v1_fact_ids = active_memory_ids(&memory, &scope());

    let v2 = block_on(batch.ingest(sourced_request(
        "doc-x@v2",
        "docs/x.md",
        vec![memory_record("The flange is carbon fiber", "0b")],
        vec![entity("Widget"), entity("Factory")],
        Vec::new(), // relationship removed
    )))
    .expect("v2");

    // Active set equals v2's declared set: one fact (the changed text), no
    // relationships.
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec!["The flange is carbon fiber".to_string()]
    );
    assert!(active_relationship_ids(&knowledge, &scope()).is_empty());

    // The prior fact is tombstoned (record kept, status Forgotten) — archived
    // semantics, not hard-deleted.
    let prior = block_on(memory.get_memory(&v1_fact_ids[0], &scope()))
        .expect("get prior")
        .expect("tombstoned record persists");
    assert_eq!(prior.status, MemoryStatus::Forgotten);
    // And a Forgotten lifecycle event exists for it.
    let events = block_on(memory.list_events_for_scope(&scope())).expect("events");
    assert!(
        events
            .iter()
            .any(|e| e.kind == MemoryEventKind::Forgotten
                && e.memory_id == Some(v1_fact_ids[0].clone())),
        "tombstone lifecycle event present for the superseded fact"
    );

    // The removed relationship is archived (row kept, archived_at set),
    // absent from active reads — NOT deleted.
    let rel = block_on(knowledge.get_relationship(&Id::from("Widget␟built_in␟Factory"), &scope()))
        .expect("get rel")
        .expect("archived row persists");
    assert!(
        rel.archived_at.is_some(),
        "superseded relationship archived, not deleted"
    );

    // Re-asserted entities stay active, updated not duplicated.
    assert_eq!(
        active_entity_ids(&knowledge, &scope()),
        vec!["Factory".to_string(), "Widget".to_string()]
    );

    // Counts surfaced per step: 1 fact forgotten, 1 relationship archived.
    assert_eq!(counts_of(&v2, BatchStep::Facts).forgotten_facts, 1);
    assert_eq!(
        counts_of(&v2, BatchStep::Relationships).archived_relationships,
        1
    );

    // Retry v2 → identical state (convergence is a fixed point).
    let retry = block_on(batch.ingest(sourced_request(
        "doc-x@v2",
        "docs/x.md",
        vec![memory_record("The flange is carbon fiber", "0b")],
        vec![entity("Widget"), entity("Factory")],
        Vec::new(),
    )))
    .expect("retry");
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec!["The flange is carbon fiber".to_string()]
    );
    assert!(active_relationship_ids(&knowledge, &scope()).is_empty());
    assert_eq!(
        counts_of(&retry, BatchStep::Facts),
        SupersessionCounts::default()
    );
    assert_eq!(
        counts_of(&retry, BatchStep::Relationships),
        SupersessionCounts::default()
    );
}

// ---------- matrix case 4 (TEST-004 / AC-002, RISK-003) --------------------

#[test]
fn case_4_reordered_identical_facts_converge() {
    let (memory, _knowledge, batch) = seeded();
    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![
            memory_record("alpha fact", "0"),
            memory_record("beta fact", "1"),
        ],
        Vec::new(),
        Vec::new(),
    )))
    .expect("v1");

    // Same facts, reordered, NEW batch key (a revision). Matching is by
    // ledger id — never `{batch_key}#index` key equality — so the prior set
    // is forgotten and the new set written: no stale-content-at-index, no
    // duplicates.
    let v2 = block_on(batch.ingest(sourced_request(
        "doc-x@v2",
        "docs/x.md",
        vec![
            memory_record("beta fact", "0"),
            memory_record("alpha fact", "1"),
        ],
        Vec::new(),
        Vec::new(),
    )))
    .expect("v2");

    assert_eq!(counts_of(&v2, BatchStep::Facts).forgotten_facts, 2);
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec!["alpha fact".to_string(), "beta fact".to_string()],
        "reordered facts converge with no duplicates and no stale content"
    );
    assert_eq!(active_memory_ids(&memory, &scope()).len(), 2);
}

// ---------- matrix case 5 (TEST-005 / AC-003, NFR-006) ---------------------

#[test]
fn case_5_two_doc_overlap_ref_counts_shared_records() {
    let (memory, knowledge, batch) = seeded();
    // Source A and source B both assert Shared; each asserts an own record.
    block_on(batch.ingest(sourced_request(
        "doc-a@v1",
        "docs/a.md",
        vec![memory_record("A-only fact", "a0")],
        vec![entity("Shared"), entity("AOnly")],
        vec![relationship(
            "Shared␟a_rel␟AOnly",
            "Shared",
            "a_rel",
            "AOnly",
        )],
    )))
    .expect("A v1");
    block_on(batch.ingest(sourced_request(
        "doc-b@v1",
        "docs/b.md",
        vec![memory_record("B-only fact", "b0")],
        vec![entity("Shared"), entity("BOnly")],
        vec![relationship(
            "Shared␟b_rel␟BOnly",
            "Shared",
            "b_rel",
            "BOnly",
        )],
    )))
    .expect("B v1");

    // Both refs coexist on the shared entity (no clobber).
    let shared = block_on(knowledge.get_entity(&Id::from("Shared"), &scope()))
        .expect("get shared")
        .expect("present");
    assert!(shared.source_refs.contains(&source_ref("docs/a.md")));
    assert!(shared.source_refs.contains(&source_ref("docs/b.md")));

    // Re-distill A: v2 declares nothing (the source no longer asserts any of
    // it) → A-only records archive, A's ref is removed from the shared record
    // which stays active with B's ref.
    let a2 = block_on(batch.ingest(sourced_request(
        "doc-a@v2",
        "docs/a.md",
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )))
    .expect("A v2");

    // Shared stays ACTIVE with only B's ref; A-only records archived.
    let shared = block_on(knowledge.get_entity(&Id::from("Shared"), &scope()))
        .expect("get shared")
        .expect("present");
    assert!(shared.archived_at.is_none(), "shared record stays active");
    assert!(
        !shared.source_refs.contains(&source_ref("docs/a.md")),
        "A's ref removed"
    );
    assert!(
        shared.source_refs.contains(&source_ref("docs/b.md")),
        "B's ref intact"
    );

    let a_only = block_on(knowledge.get_entity(&Id::from("AOnly"), &scope()))
        .expect("get AOnly")
        .expect("present");
    assert!(a_only.archived_at.is_some(), "A-only entity archived");
    let a_rel = block_on(knowledge.get_relationship(&Id::from("Shared␟a_rel␟AOnly"), &scope()))
        .expect("get a_rel")
        .expect("present");
    assert!(a_rel.archived_at.is_some(), "A-only relationship archived");

    // B's records are fully intact.
    assert_eq!(
        active_entity_ids(&knowledge, &scope()),
        vec!["BOnly".to_string(), "Shared".to_string()]
    );
    assert_eq!(
        active_relationship_ids(&knowledge, &scope()),
        vec!["Shared␟b_rel␟BOnly".to_string()]
    );
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec!["B-only fact".to_string()],
        "A's fact forgotten, B's untouched"
    );

    // Counts: A's entity step removed 1 ref + archived 1; relationships
    // archived 1; facts forgotten 1.
    let entities = counts_of(&a2, BatchStep::Entities);
    assert_eq!(
        entities.refs_removed, 1,
        "shared entity: ref removed, stays active"
    );
    assert_eq!(entities.archived_entities, 1);
    assert_eq!(
        counts_of(&a2, BatchStep::Relationships).archived_relationships,
        1
    );
    assert_eq!(counts_of(&a2, BatchStep::Facts).forgotten_facts, 1);
}

// ---------- matrix case 6 (TEST-006 / AC-005, NFR-001) ---------------------

#[test]
fn case_6_retraction_idempotent_and_reassert_restores() {
    let (memory, knowledge, batch) = seeded();
    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![
            memory_record("kept fact", "0"),
            memory_record("dropped fact", "1"),
        ],
        vec![entity("Widget"), entity("Dropped")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    )))
    .expect("v1");

    let v2_request = sourced_request(
        "doc-x@v2",
        "docs/x.md",
        vec![memory_record("kept fact", "0")],
        vec![entity("Widget")],
        Vec::new(), // relationship removed; Dropped entity not re-asserted
    );
    let v2 = block_on(batch.ingest(v2_request.clone())).expect("v2");
    // Facts are a declared set: BOTH prior facts of batch v1 are forgotten and
    // the kept one re-lands under the new key — the active set is exactly
    // v2's declared facts.
    assert_eq!(counts_of(&v2, BatchStep::Facts).forgotten_facts, 2);
    assert_eq!(counts_of(&v2, BatchStep::Entities).archived_entities, 1);

    // Re-run the same v2 (same payload, same key): zero-effect second pass —
    // every supersession count is zero and the state is identical.
    let second = block_on(batch.ingest(v2_request)).expect("second pass");
    assert_eq!(
        counts_of(&second, BatchStep::Facts),
        SupersessionCounts::default()
    );
    assert_eq!(
        counts_of(&second, BatchStep::Entities),
        SupersessionCounts::default()
    );
    assert_eq!(
        counts_of(&second, BatchStep::Relationships),
        SupersessionCounts::default()
    );
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec!["kept fact".to_string()]
    );
    assert_eq!(
        active_entity_ids(&knowledge, &scope()),
        vec!["Widget".to_string()]
    );

    // Re-assert the dropped entity in v3: restored (active again), ref
    // re-stamped — archive was reversible.
    let v3 = block_on(batch.ingest(sourced_request(
        "doc-x@v3",
        "docs/x.md",
        vec![memory_record("kept fact", "0")],
        vec![entity("Widget"), entity("Dropped")],
        Vec::new(),
    )))
    .expect("v3");
    assert_eq!(
        counts_of(&v3, BatchStep::Entities).restored,
        1,
        "re-assert restores the archive"
    );
    assert_eq!(
        active_entity_ids(&knowledge, &scope()),
        vec!["Dropped".to_string(), "Widget".to_string()]
    );
    let dropped = block_on(knowledge.get_entity(&Id::from("Dropped"), &scope()))
        .expect("get dropped")
        .expect("present");
    assert!(dropped.archived_at.is_none());
    assert!(dropped.source_refs.contains(&source_ref("docs/x.md")));
}

// ---------- AC-006 regression (TEST-007) ------------------------------------

#[test]
fn unsourced_batch_is_legacy_append_only() {
    let (memory, knowledge, batch) = seeded();
    let mut request = sourced_request(
        "legacy-key",
        "docs/x.md",
        vec![memory_record("legacy fact", "0")],
        vec![entity("Widget")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    );
    request.source_id = None;

    let outcome = block_on(batch.ingest(request)).expect("ingest");
    assert_eq!(
        step_status(&outcome, BatchStep::Episode),
        &StepStatus::Skipped
    );

    // No supersession counts on any step (unsourced outcomes carry None).
    for step in &outcome.steps {
        assert!(
            step.supersession.is_none(),
            "unsourced step carries no counts"
        );
    }

    // No refs stamped, no registry source, no ledger rows.
    let stamped = block_on(knowledge.get_entity(&Id::from("Widget"), &scope()))
        .expect("get")
        .expect("present");
    assert!(stamped.source_refs.is_empty(), "no source refs stamped");
    let rel = block_on(knowledge.get_relationship(&Id::from("Widget␟built_in␟Factory"), &scope()))
        .expect("get rel")
        .expect("present");
    assert!(rel.evidence.is_empty());
    let active = active_memory_ids(&memory, &scope());
    let record = block_on(memory.get_memory(&active[0], &scope()))
        .expect("get")
        .expect("present");
    assert!(record.provenance.evidence.is_empty());
    assert!(
        block_on(knowledge.list_sources(&scope()))
            .expect("sources")
            .is_empty(),
        "no registry source for unsourced batches"
    );
    for kind in [
        SourceBatchRecordKind::Entity,
        SourceBatchRecordKind::Relationship,
        SourceBatchRecordKind::MemoryRecord,
    ] {
        assert!(
            ledger_rows(&knowledge, &scope(), "docs/x.md", kind).is_empty(),
            "unsourced batches write no ledger rows"
        );
    }

    // Legacy behavior: a second identical unsourced batch does NOT retract
    // anything (append-only), and re-asserting overlapping content leaves
    // prior records active.
    let second = block_on(batch.ingest(sourced_request(
        "legacy-key-2",
        "docs/x.md",
        vec![memory_record("another legacy fact", "1")],
        Vec::new(),
        Vec::new(),
    )));
    let _ = second.expect("second unsourced ingest");
    assert_eq!(
        active_fact_texts(&memory, &scope()).len(),
        2,
        "append-only: both legacy facts stay active"
    );
}

// ---------- AC-007 regression (TEST-008 / RISK-006) ------------------------

#[test]
fn code_attributed_records_are_immune() {
    let (_memory, knowledge, batch) = seeded();
    // A code-scan-attributed entity sharing the doc entity's NAME (different
    // id — code attribution rides graph_id, no doc source ref), plus a doc
    // entity written by the sourced batch.
    let mut code_entity = entity_with_refs("src/widget.rs#Widget", Vec::new());
    code_entity.graph_id = Some(Id::from("graph-code"));
    code_entity.name = "entity-Widget".to_string(); // same NAME as the doc entity
    block_on(knowledge.put_entity(code_entity.clone())).expect("seed code entity");

    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        Vec::new(),
        vec![entity("Widget")],
        Vec::new(),
    )))
    .expect("v1");

    // v2 drops the doc entity → supersession archives it; the code entity
    // (no ledger row, no doc ref) is never touched.
    let v2 = block_on(batch.ingest(sourced_request(
        "doc-x@v2",
        "docs/x.md",
        Vec::new(),
        Vec::new(), // doc entity dropped
        Vec::new(),
    )))
    .expect("v2");
    assert_eq!(counts_of(&v2, BatchStep::Entities).archived_entities, 1);

    let code = block_on(knowledge.get_entity(&Id::from("src/widget.rs#Widget"), &scope()))
        .expect("get code entity")
        .expect("present");
    assert!(code.archived_at.is_none(), "code entity untouched");
    assert_eq!(
        code.source_refs, code_entity.source_refs,
        "code entity refs untouched"
    );
    assert_eq!(code.graph_id, Some(Id::from("graph-code")));

    // A ledgered record whose refs were replaced by another writer is immune
    // too (skipped + counted — matching is by ref membership, never name/id).
    block_on(batch.ingest(sourced_request(
        "doc-y@v1",
        "docs/y.md",
        Vec::new(),
        vec![entity("ReAttributed")],
        Vec::new(),
    )))
    .expect("seed re-attributed");
    let mut rewritten = entity_with_refs("ReAttributed", vec![source_ref("other:writer")]);
    rewritten.created_at = chrono::Utc::now();
    block_on(knowledge.put_entity(rewritten)).expect("another writer re-attributed the record");

    let y2 = block_on(batch.ingest(sourced_request(
        "doc-y@v2",
        "docs/y.md",
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )))
    .expect("supersede y");
    let counts = counts_of(&y2, BatchStep::Entities);
    assert_eq!(
        counts.immune_skipped, 1,
        "ref-less record skipped as immune"
    );
    assert_eq!(counts.archived_entities, 0, "immune record not archived");
    let reattributed = block_on(knowledge.get_entity(&Id::from("ReAttributed"), &scope()))
        .expect("get")
        .expect("present");
    assert!(
        reattributed.archived_at.is_none(),
        "immune record stays active"
    );
    assert_eq!(
        reattributed.source_refs,
        vec![source_ref("other:writer")],
        "other writer's attribution preserved"
    );
}

// ---------- facts immunity via ref membership (FIND-security-001) ---------

#[test]
fn foreign_fact_adopted_by_key_collision_is_immune() {
    let (memory, knowledge, batch) = seeded();
    // Batch A writes one fact under batch key "shared-key" — the per-record
    // idempotency key is "shared-key#0".
    block_on(batch.ingest(sourced_request(
        "shared-key",
        "docs/a.md",
        vec![memory_record("fact from source A", "0")],
        Vec::new(),
        Vec::new(),
    )))
    .expect("A v1");
    let a_ids = active_memory_ids(&memory, &scope());
    assert_eq!(a_ids.len(), 1, "A's fact landed");

    // Batch B reuses the SAME batch key for a DIFFERENT source: the write
    // path dedups against "shared-key#0" and returns A's stored record, so
    // B's ledger adopts A's record id — but the record carries only A's ref
    // (the B stamp never lands on a deduplicated write).
    let b = block_on(batch.ingest(sourced_request(
        "shared-key",
        "docs/b.md",
        vec![memory_record("fact from source B", "1")],
        Vec::new(),
        Vec::new(),
    )))
    .expect("B v1");
    assert_eq!(b.steps[1].status, StepStatus::Deduplicated);
    assert_eq!(
        active_memory_ids(&memory, &scope()),
        a_ids,
        "the colliding write adopted A's record, no second record landed"
    );
    let adopted = ledger_rows(
        &knowledge,
        &scope(),
        "docs/b.md",
        SourceBatchRecordKind::MemoryRecord,
    );
    assert_eq!(
        adopted.len(),
        1,
        "B's ledger adopted A's record id — the guard must make that harmless"
    );
    assert_eq!(adopted[0].1, a_ids[0].to_string());

    // B's revision (new batch key) supersedes its prior rows. The adopted
    // record's Provenance.evidence carries docs/a.md, not docs/b.md →
    // immune: no forget, A's fact stays active with its content intact.
    let b2 = block_on(batch.ingest(sourced_request(
        "doc-b@v2",
        "docs/b.md",
        vec![memory_record("fact from source B v2", "2")],
        Vec::new(),
        Vec::new(),
    )))
    .expect("B v2");
    let counts = counts_of(&b2, BatchStep::Facts);
    assert_eq!(counts.forgotten_facts, 0, "foreign record not forgotten");
    assert_eq!(counts.immune_skipped, 1, "adopted row retired as immune");
    let a_record = block_on(memory.get_memory(&a_ids[0], &scope()))
        .expect("get A fact")
        .expect("A's fact still exists");
    assert_eq!(
        a_record.status,
        MemoryStatus::Active,
        "A's fact untouched (no tombstone, no content wipe)"
    );
    assert_eq!(a_record.content.text, "fact from source A");

    // Second pass of B v2: the immune row retired, so every count is zero —
    // idempotent convergence despite the adoption.
    let second = block_on(batch.ingest(sourced_request(
        "doc-b@v2",
        "docs/b.md",
        vec![memory_record("fact from source B v2", "2")],
        Vec::new(),
        Vec::new(),
    )))
    .expect("second pass");
    assert_eq!(
        counts_of(&second, BatchStep::Facts),
        SupersessionCounts::default(),
        "second pass counts zero"
    );
    assert_eq!(
        active_fact_texts(&memory, &scope()),
        vec![
            "fact from source A".to_string(),
            "fact from source B v2".to_string()
        ],
        "both sources' facts active, neither adopted away"
    );
}

// ---------- canonical optional-dim scope (FIND-security-002 / FIND-data-001) --

#[test]
fn supersession_respects_canonical_session_and_subject_scope() {
    let (memory, knowledge, batch) = seeded();
    // v1 runs under session-1: rows land in that scope envelope.
    let mut v1 = sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![memory_record("session-1 fact", "0")],
        vec![entity("SessionEntity")],
        Vec::new(),
    );
    v1.scope.session = Some("session-1".to_string());
    block_on(batch.ingest(v1)).expect("v1");
    let s1_scope = Scope {
        session: Some("session-1".to_string()),
        ..scope()
    };
    let s1_fact_ids = active_memory_ids(&memory, &s1_scope);
    assert_eq!(s1_fact_ids.len(), 1, "session-1 fact landed");

    // Re-distill the SAME source_id under a DRIFTED session: canonically a
    // foreign scope — s1's ledger rows must be invisible (no counts, no
    // churn), never listed-then-unretirable.
    let mut v2 = sourced_request("doc-x@v2", "docs/x.md", Vec::new(), Vec::new(), Vec::new());
    v2.scope.session = Some("session-2".to_string());
    let v2_outcome = block_on(batch.ingest(v2.clone())).expect("v2 under drifted session");
    assert_eq!(
        counts_of(&v2_outcome, BatchStep::Facts),
        SupersessionCounts::default(),
        "drifted session sees no foreign ledger rows"
    );
    assert_eq!(
        counts_of(&v2_outcome, BatchStep::Entities),
        SupersessionCounts::default()
    );
    // …and s1's rows/records are untouched (still listed, still active).
    assert_eq!(
        ledger_rows(
            &knowledge,
            &s1_scope,
            "docs/x.md",
            SourceBatchRecordKind::MemoryRecord
        )
        .len(),
        1,
        "owning scope's ledger row not retired by a foreign session"
    );
    assert_eq!(
        active_fact_texts(&memory, &s1_scope),
        vec!["session-1 fact".to_string()],
        "owning scope's fact untouched"
    );

    // The OWNING scope retires its own rows: second pass counts zero.
    let mut v3 = sourced_request("doc-x@v3", "docs/x.md", Vec::new(), Vec::new(), Vec::new());
    v3.scope.session = Some("session-1".to_string());
    let v3_outcome = block_on(batch.ingest(v3.clone())).expect("v3 from owning scope");
    assert_eq!(counts_of(&v3_outcome, BatchStep::Facts).forgotten_facts, 1);
    assert_eq!(
        counts_of(&v3_outcome, BatchStep::Entities).archived_entities,
        1
    );
    let v3_second = block_on(batch.ingest(v3)).expect("second pass");
    for step in [
        BatchStep::Facts,
        BatchStep::Entities,
        BatchStep::Relationships,
    ] {
        assert_eq!(
            counts_of(&v3_second, step),
            SupersessionCounts::default(),
            "own rows retire — zero-count second pass"
        );
    }
    assert!(active_memory_ids(&memory, &s1_scope).is_empty());

    // NULL-vs-Some subject: rows recorded by a subject=None run are invisible
    // to a narrower Some(subject) re-distill of the same source id (a NULL
    // record column is NOT a wildcard), and the wide run's rows retire only
    // from the wide scope.
    let wide_scope = Scope {
        subject: None,
        ..scope()
    };
    let mut wide_v1 = sourced_request(
        "doc-w@v1",
        "docs/w.md",
        vec![memory_record("wide fact", "w0")],
        vec![entity("WideEntity")],
        Vec::new(),
    );
    wide_v1.scope = wide_scope.clone();
    block_on(batch.ingest(wide_v1)).expect("wide v1");
    let narrow = sourced_request("doc-w@v2", "docs/w.md", Vec::new(), Vec::new(), Vec::new());
    let narrow_outcome = block_on(batch.ingest(narrow)).expect("narrow v2");
    assert_eq!(
        counts_of(&narrow_outcome, BatchStep::Facts),
        SupersessionCounts::default(),
        "subject=None rows invisible to a Some(subject) request"
    );
    assert_eq!(
        counts_of(&narrow_outcome, BatchStep::Entities),
        SupersessionCounts::default()
    );
    assert_eq!(
        active_fact_texts(&memory, &wide_scope),
        vec!["wide fact".to_string()],
        "wide run's fact untouched by the narrow re-distill"
    );

    // By-source-ref reads follow the same canonical rule: the narrow scope
    // cannot see the wide run's stamped entity, the wide scope can.
    assert!(
        block_on(knowledge.list_entities_by_source_ref("docs/w.md", &scope()))
            .expect("narrow by-ref")
            .is_empty(),
        "by-source-ref read must not widen a NULL subject to Some"
    );
    assert_eq!(
        block_on(knowledge.list_entities_by_source_ref("docs/w.md", &wide_scope))
            .expect("wide by-ref")
            .len(),
        1
    );
}

// ---------- injected partial failure (TEST-010 / NFR-003, RISK-002) ---------

/// A port-level stub that delegates to a real store but fails every
/// `put_relationship` (the injected failure): proves honest partial counts,
/// a Failed step, no rollback claims, and retry convergence.
struct FailingPutsStore {
    inner: Arc<SqlKnowledgeStore>,
}

#[async_trait]
impl KnowledgeRepository for FailingPutsStore {
    async fn put_source(&self, source: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.inner.put_source(source).await
    }
    async fn put_document(&self, document: SourceDocument) -> CoreResult<SourceDocument> {
        self.inner.put_document(document).await
    }
    async fn put_chunk(&self, chunk: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        self.inner.put_chunk(chunk).await
    }
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        self.inner.get_chunk(id, scope).await
    }
    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        self.inner.get_entity(id, scope).await
    }
    async fn put_entity(&self, entity: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.inner.put_entity(entity).await
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        self.inner.get_relationship(id, scope).await
    }
    async fn put_relationship(
        &self,
        _relationship: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        Err(CoreError::Adapter {
            adapter: "injected-failure".to_owned(),
            message: "put_relationship injected failure".to_owned(),
        })
    }
    async fn restore_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        self.inner.restore_entity(id, scope).await
    }
    async fn restore_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        self.inner.restore_relationship(id, scope).await
    }
}

#[async_trait]
impl SourceBatchLedger for FailingPutsStore {
    async fn list_source_batch_records(
        &self,
        scope: &Scope,
        source_id: &str,
    ) -> CoreResult<Vec<engram_knowledge::SourceBatchRecord>> {
        self.inner.list_source_batch_records(scope, source_id).await
    }
    async fn record_source_batch_writes(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_ids: &[String],
    ) -> CoreResult<()> {
        self.inner
            .record_source_batch_writes(scope, source_id, batch_key, kind, record_ids)
            .await
    }
    async fn mark_source_batch_superseded(
        &self,
        scope: &Scope,
        source_id: &str,
        batch_key: &str,
        kind: SourceBatchRecordKind,
        record_id: &str,
    ) -> CoreResult<()> {
        self.inner
            .mark_source_batch_superseded(scope, source_id, batch_key, kind, record_id)
            .await
    }
}

#[test]
fn injected_partial_failure_is_honest_and_converges_on_retry() {
    let (_memory, knowledge, batch) = seeded();
    let rel_id = Id::from("Widget␟built_in␟Factory");
    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        Vec::new(),
        vec![entity("Widget")],
        vec![relationship(
            "Widget␟built_in␟Factory",
            "Widget",
            "built_in",
            "Factory",
        )],
    )))
    .expect("v1 (healthy)");

    // v2 through the failing store: superseding R1's ref-trim put fails, the
    // new relationship write fails — but entities still land and counts are
    // surfaced. No rollback is claimed or attempted.
    let failing = FailingPutsStore {
        inner: knowledge.clone(),
    };
    let v2_request = sourced_request(
        "doc-x@v2",
        "docs/x.md",
        Vec::new(),
        vec![entity("Widget"), entity("NewInV2")],
        vec![relationship(
            "Widget␟v2_rel␟NewInV2",
            "Widget",
            "v2_rel",
            "NewInV2",
        )],
    );
    let result = block_on(run_relationships_step(
        &failing,
        knowledge.as_ref(),
        &v2_request,
    ));
    assert!(result.failure.is_some(), "injected failure surfaced");
    assert!(
        result.counts.failed >= 1,
        "failed count is honest: {:?}",
        result.counts
    );
    assert_eq!(result.counts.examined, 1);
    assert_eq!(
        result.counts.archived_relationships, 0,
        "nothing falsely counted as archived"
    );
    assert_eq!(
        result.status(false),
        StepStatus::Failed,
        "the failed step reports Failed"
    );
    // The entity step (not run here) would still land — per-step isolation is
    // the orchestrator's; here the relationship ledger row stays unsuperseded
    // so a retry converges.
    let pending = ledger_rows(
        &knowledge,
        &scope(),
        "docs/x.md",
        SourceBatchRecordKind::Relationship,
    );
    assert_eq!(pending.len(), 1, "failed item's ledger row stays eligible");
    assert_eq!(pending[0].1, rel_id.to_string());

    // Re-run the same source with the healthy store: converges.
    let retry = block_on(batch.ingest(v2_request)).expect("retry");
    assert_eq!(
        counts_of(&retry, BatchStep::Relationships).archived_relationships,
        1
    );
    assert_eq!(
        active_relationship_ids(&knowledge, &scope()),
        vec!["Widget␟v2_rel␟NewInV2".to_string()],
        "converged to v2's declared relationship set"
    );
}

// ---------- scope isolation + empty source id (TEST-013) --------------------

#[test]
fn supersession_is_scope_bounded_and_empty_source_id_is_unsourced() {
    let (memory, knowledge, batch) = seeded();
    // Scope A v1 for the source.
    block_on(batch.ingest(sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![memory_record("scope A fact", "0")],
        vec![entity("WidgetA")],
        Vec::new(),
    )))
    .expect("A v1");

    // Scope B holds the SAME source id with its own records (distinct entity
    // id — knowledge entity ids are global row keys, scoped by columns).
    let mut b_request = sourced_request(
        "doc-x@v1",
        "docs/x.md",
        vec![memory_record("scope B fact", "b0")],
        vec![entity("WidgetB")],
        Vec::new(),
    );
    b_request.scope = other_scope();
    block_on(batch.ingest(b_request)).expect("B v1");

    // Supersede in scope A (v2 declares nothing).
    block_on(batch.ingest(sourced_request(
        "doc-x@v2",
        "docs/x.md",
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )))
    .expect("A v2");

    // Scope A converged…
    assert!(
        active_memory_ids(&memory, &scope()).is_empty(),
        "A's fact superseded"
    );
    assert!(
        active_entity_ids(&knowledge, &scope()).is_empty(),
        "A's entity archived"
    );
    // …while scope B is completely untouched.
    assert_eq!(
        active_fact_texts(&memory, &other_scope()),
        vec!["scope B fact".to_string()],
        "scope B fact untouched"
    );
    let b_entity = block_on(knowledge.get_entity(&Id::from("WidgetB"), &other_scope()))
        .expect("get B entity")
        .expect("present");
    assert!(b_entity.archived_at.is_none(), "scope B entity untouched");
    assert!(b_entity.source_refs.contains(&source_ref("docs/x.md")));
    assert_eq!(
        ledger_rows(
            &knowledge,
            &other_scope(),
            "docs/x.md",
            SourceBatchRecordKind::MemoryRecord
        )
        .len(),
        1,
        "scope B ledger untouched"
    );
    assert_eq!(
        ledger_rows(
            &knowledge,
            &other_scope(),
            "docs/x.md",
            SourceBatchRecordKind::Entity
        )
        .len(),
        1,
        "scope B entity ledger untouched"
    );

    // An empty-string source_id behaves exactly like unsourced.
    let mut empty = sourced_request(
        "empty-source",
        "docs/x.md",
        vec![memory_record("empty source id fact", "e0")],
        vec![entity("EmptySource")],
        Vec::new(),
    );
    empty.source_id = Some(String::new());
    let outcome = block_on(batch.ingest(empty)).expect("ingest");
    for step in &outcome.steps {
        assert!(
            step.supersession.is_none(),
            "empty source_id carries no counts"
        );
    }
    // The empty-source-id entity landed WITHOUT any ledger row (unsourced).
    let empty_rows = ledger_rows(
        &knowledge,
        &scope(),
        "docs/x.md",
        SourceBatchRecordKind::Entity,
    );
    assert!(
        empty_rows.iter().all(|(_, id)| id != "EmptySource"),
        "empty source_id writes no ledger rows for its records"
    );
}
