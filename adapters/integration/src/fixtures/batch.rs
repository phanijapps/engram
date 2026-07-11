//! Atomic-batch capability fixture (engram-host-sdk brief, S3).
//!
//! Ingests a full batch (all six steps) through [`SqlBatchIngest`] against
//! in-memory memory + knowledge stores, recovers the writable records, then
//! re-ingests for the `Deduplicated` path and forces a partial failure for the
//! `Partial` path. The `atomic_batch` capability is only reported `Supported`
//! when this fixture passes during bootstrap. This is the cross-cutting
//! integration test for the batch ingest port: it proves a best-effort batch
//! lands every writable record, that the `{batch_key}#{index}` per-record-key
//! derivation keeps N distinct memories distinct, that re-ingest deduplicates
//! the Facts step, and that a step failure is reported with a typed error while
//! the other steps still land — all under `TransactionGuarantee::BestEffort`.

use std::sync::Arc;

use engram_domain::*;
use engram_integration::{
    BatchIngest, BatchIngestRequest, BatchStatus, BatchStep, StepStatus, TransactionGuarantee,
};
use engram_runtime::{CoreError, CoreResult};
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use engram_store_sql::SqlMemoryService;
use futures::executor::block_on;

use super::support::{policy, provenance, scope};
use engram_integration::sqlite::SqlBatchIngest;

/// Runs the atomic-batch fixture.
///
/// Writes a full batch (source, facts, entities, relationships; evidence +
/// embeddings present but `Skipped` in v1), recovers every writable record,
/// then re-ingests (Facts `Deduplicated`, knowledge `Succeeded`) and forces a
/// partial failure (a failing step carries a typed error; the others land).
///
/// # Errors
///
/// Returns `CoreError::Adapter` if any write/read or outcome assertion fails.
pub fn run_batch_fixture() -> CoreResult<()> {
    let memory: Arc<SqlMemoryService> = Arc::new(SqlMemoryService::open_in_memory()?);
    let knowledge: Arc<SqlKnowledgeStore> = Arc::new(SqlKnowledgeStore::open_in_memory()?);
    let batch = SqlBatchIngest::new(memory.clone(), knowledge.clone());

    // The handle reports best-effort, never atomic.
    if batch.transaction_guarantee() != TransactionGuarantee::BestEffort {
        return Err(err("transaction_guarantee")(CoreError::Conflict {
            reason: "batch handle must report BestEffort".to_string(),
        }));
    }

    let batch_scope = scope("tenant-batch");

    // ---- 1. full batch → Complete (Evidence/Embeddings Skipped) ------------
    let full = BatchIngestRequest {
        idempotency_key: "fixture-full".to_string(),
        scope: batch_scope.clone(),
        source: Some(source("src-batch")),
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![fact("fixture fact one", "0"), fact("fixture fact two", "1")],
        entities: vec![entity("ent-batch-0"), entity("ent-batch-1")],
        relationships: vec![relationship("rel-batch-0")],
        // Evidence + embeddings are present in the request but reported Skipped
        // in v1 — honest, not silent.
        evidence: vec![evidence_ref()],
        embeddings: Vec::new(),
    };

    let outcome = block_on(batch.ingest(full)).map_err(err("ingest(full)"))?;
    if outcome.guarantee != TransactionGuarantee::BestEffort {
        return Err(err("ingest(full)")(CoreError::Conflict {
            reason: "outcome guarantee must be BestEffort".to_string(),
        }));
    }
    if outcome.status != BatchStatus::Complete {
        return Err(err("ingest(full)")(CoreError::Conflict {
            reason: format!("full batch should be Complete, got {:?}", outcome.status),
        }));
    }
    assert_step(
        &outcome,
        BatchStep::Episode,
        StepStatus::Succeeded,
        "ingest(full)",
    )?;
    assert_step(
        &outcome,
        BatchStep::Facts,
        StepStatus::Succeeded,
        "ingest(full)",
    )?;
    assert_step(
        &outcome,
        BatchStep::Entities,
        StepStatus::Succeeded,
        "ingest(full)",
    )?;
    assert_step(
        &outcome,
        BatchStep::Relationships,
        StepStatus::Succeeded,
        "ingest(full)",
    )?;
    assert_step(
        &outcome,
        BatchStep::Evidence,
        StepStatus::Skipped,
        "ingest(full)",
    )?;
    assert_step(
        &outcome,
        BatchStep::Embeddings,
        StepStatus::Skipped,
        "ingest(full)",
    )?;

    // Recover the writable records — every one landed.
    let entities = block_on(knowledge.list_entities(&batch_scope)).map_err(err("list_entities"))?;
    if entities.len() != 2 {
        return Err(err("list_entities")(CoreError::Conflict {
            reason: format!("expected 2 entities, found {}", entities.len()),
        }));
    }
    let rels =
        block_on(knowledge.list_relationships(&batch_scope)).map_err(err("list_relationships"))?;
    if rels.len() != 1 {
        return Err(err("list_relationships")(CoreError::Conflict {
            reason: format!("expected 1 relationship, found {}", rels.len()),
        }));
    }
    let sources = block_on(knowledge.list_sources(&batch_scope)).map_err(err("list_sources"))?;
    if sources.is_empty() {
        return Err(err("list_sources")(CoreError::Conflict {
            reason: "source did not land".to_string(),
        }));
    }
    // Two distinct facts land — the per-record-key derivation guard.
    let written = count_written_memories(&memory, &batch_scope)?;
    if written != 2 {
        return Err(err("count_written_memories")(CoreError::Conflict {
            reason: format!("expected 2 written memories, found {written}"),
        }));
    }

    // ---- 2. re-ingest → Facts Deduplicated, knowledge Succeeded ------------
    let reingest = BatchIngestRequest {
        idempotency_key: "fixture-full".to_string(),
        scope: batch_scope.clone(),
        source: Some(source("src-batch")),
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![fact("fixture fact one", "0"), fact("fixture fact two", "1")],
        entities: vec![entity("ent-batch-0")],
        relationships: vec![relationship("rel-batch-0")],
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    let re_outcome = block_on(batch.ingest(reingest)).map_err(err("ingest(re-ingest)"))?;
    if re_outcome.status != BatchStatus::Complete {
        return Err(err("ingest(re-ingest)")(CoreError::Conflict {
            reason: format!("re-ingest should be Complete, got {:?}", re_outcome.status),
        }));
    }
    assert_step(
        &re_outcome,
        BatchStep::Facts,
        StepStatus::Deduplicated,
        "re-ingest",
    )?;
    assert_step(
        &re_outcome,
        BatchStep::Entities,
        StepStatus::Succeeded,
        "re-ingest",
    )?;

    // ---- 3. forced partial failure → Partial + typed error -----------------
    let mut bad_fact = fact("will be blanked", "bad");
    bad_fact.content.text = String::new(); // empty text → write_memory validation rejects
    let partial = BatchIngestRequest {
        idempotency_key: "fixture-partial".to_string(),
        scope: batch_scope.clone(),
        source: None,
        documents: Vec::new(),
        chunks: Vec::new(),
        facts: vec![bad_fact],
        entities: vec![entity("ent-partial")],
        relationships: Vec::new(),
        evidence: Vec::new(),
        embeddings: Vec::new(),
    };
    let partial_outcome = block_on(batch.ingest(partial)).map_err(err("ingest(partial)"))?;
    if partial_outcome.status != BatchStatus::Partial {
        return Err(err("ingest(partial)")(CoreError::Conflict {
            reason: format!(
                "partial batch should be Partial, got {:?}",
                partial_outcome.status
            ),
        }));
    }
    let facts_step = partial_outcome
        .steps
        .iter()
        .find(|o| o.step == BatchStep::Facts)
        .ok_or_else(|| {
            err("ingest(partial)")(CoreError::Conflict {
                reason: "Facts step missing".to_string(),
            })
        })?;
    if facts_step.status != StepStatus::Failed {
        return Err(err("ingest(partial)")(CoreError::Conflict {
            reason: format!(
                "Facts should be Failed on empty-text fact, got {:?}",
                facts_step.status
            ),
        }));
    }
    // The failed step carries a typed CoreError (not a string).
    if facts_step.error.is_none() {
        return Err(err("ingest(partial)")(CoreError::Conflict {
            reason: "failed Facts step must carry a typed CoreError".to_string(),
        }));
    }
    // The other writable step still landed (best-effort: no rollback).
    assert_step(
        &partial_outcome,
        BatchStep::Entities,
        StepStatus::Succeeded,
        "partial",
    )?;

    Ok(())
}

/// Counts `Written` lifecycle events in `scope` — each landed fact produces one.
fn count_written_memories(memory: &Arc<SqlMemoryService>, scope: &Scope) -> CoreResult<usize> {
    use engram_memory::MemoryEventRepository;
    let events = block_on(memory.list_events_for_scope(scope)).map_err(err("list_events"))?;
    Ok(events
        .iter()
        .filter(|e| e.kind == MemoryEventKind::Written)
        .count())
}

/// Asserts a step's status, returning an Adapter error naming `op` on mismatch.
fn assert_step(
    outcome: &engram_integration::BatchOutcome,
    step: BatchStep,
    expected: StepStatus,
    op: &str,
) -> CoreResult<()> {
    let actual = outcome
        .steps
        .iter()
        .find(|o| o.step == step)
        .map(|o| &o.status)
        .ok_or_else(|| {
            err(op)(CoreError::Conflict {
                reason: format!("step {step:?} missing"),
            })
        })?;
    if actual != &expected {
        return Err(err(op)(CoreError::Conflict {
            reason: format!("step {step:?}: expected {expected:?}, got {actual:?}"),
        }));
    }
    Ok(())
}

fn err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.batch".to_string(),
        message: format!("{op}: {e}"),
    }
}

// ---------- domain constructors -------------------------------------------

fn fact(text: &str, suffix: &str) -> MemoryRecord {
    MemoryRecord {
        id: Id::from(format!("fixture-fact-{suffix}")),
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
        scope: scope("tenant-batch"),
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
        scope: scope("tenant-batch"),
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
            id: Some(Id::from("ent-batch-0")),
            kind: Some("function".to_string()),
            name: Some("caller".to_string()),
            aliases: Vec::new(),
        },
        predicate: "calls".to_string(),
        object: EntityRef {
            id: Some(Id::from("ent-batch-1")),
            kind: Some("function".to_string()),
            name: Some("callee".to_string()),
            aliases: Vec::new(),
        },
        scope: scope("tenant-batch"),
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
        scope: scope("tenant-batch"),
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

fn evidence_ref() -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Entity,
        target_id: Some("ent-batch-0".to_string()),
        uri: None,
        quote: Some("supports the claim".to_string()),
        location: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_fixture_passes() {
        if let Err(e) = run_batch_fixture() {
            panic!("batch fixture failed: {e}");
        }
    }
}
