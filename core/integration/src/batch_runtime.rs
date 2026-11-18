//! Canonical best-effort semantic batches over exact operation-session ports.
use std::sync::Arc;

use crate::storage_composition::{CompositionKnowledge, ObservedPorts, StorageComposition};
use async_trait::async_trait;
use chrono::Utc;
use engram_domain::{
    Actor, KnowledgeEntity, KnowledgeRelationship, KnowledgeSource, Requester, Scope,
    WriteMemoryRequest,
};
use engram_knowledge::KnowledgeRepository as _;
use engram_memory::MemoryService as _;
use engram_runtime::{CoreError, CoreResult};

use crate::{
    ALL_STEPS, BatchIngest, BatchIngestRequest, BatchOutcome, BatchStep, StepOutcome, StepStatus,
    TransactionGuarantee, aggregate_status,
};

use crate::batch_supersession::{
    SourcedStepOutcome, registry_source, run_entities_observed as run_entities_step,
    run_facts_observed as run_facts_step, run_relationships_observed as run_relationships_step,
};

/// Operation-local execution over the session-bound memory and knowledge ports.
pub struct BatchExecution {
    memory: Arc<ObservedPorts>,
    knowledge: Arc<ObservedPorts>,
}

impl BatchExecution {
    /// Wraps shared memory + knowledge store handles to expose batch ingest.
    /// Episode step: writes the optional source, documents, and chunks via the
    /// knowledge repository. An empty episode reports [`StepStatus::Skipped`].
    /// The batch [`Scope`] is authoritative — each record is re-scoped to it.
    /// A **sourced** batch additionally puts the registry `KnowledgeSource`
    /// row (id = source_id) so distill sources are queryable — guarded against
    /// foreign-scope id collisions (see [`put_registry_source`]) and soft-failed
    /// like the rest of the episode writes.
    async fn step_episode(&self, request: &BatchIngestRequest) -> StepOutcome {
        let step = BatchStep::Episode;
        let registry = request
            .source_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|sid| registry_source(sid, &request.scope));
        let has_episode = request.source.is_some()
            || !request.documents.is_empty()
            || !request.chunks.is_empty()
            || registry.is_some();
        if !has_episode {
            return StepOutcome::ok(step, StepStatus::Skipped);
        }
        let mut failure: Option<CoreError> = None;
        if let Some(source) = &request.source {
            let mut record = source.clone();
            record.scope = request.scope.clone();
            if let Err(e) = self.knowledge.put_source(record).await {
                failure = Some(e);
            }
        }
        if !self.memory.stopped()
            && let Some(registry) = registry
            && let Err(e) = put_registry_source(self.knowledge.as_ref(), registry).await
            && failure.is_none()
        {
            failure = Some(e);
        }
        for document in &request.documents {
            if self.memory.stopped() {
                break;
            }
            // SourceDocument carries no scope field; written through unchanged.
            // Callers must ensure its `source_id` references a batch-scoped source.
            if let Err(e) = self.knowledge.put_document(document.clone()).await
                && failure.is_none()
            {
                failure = Some(e);
            }
        }
        for chunk in &request.chunks {
            if self.memory.stopped() {
                break;
            }
            // KnowledgeChunk carries no scope field either; written through
            // unchanged (callers ensure its `source_id` is batch-scoped).
            if let Err(e) = self.knowledge.put_chunk(chunk.clone()).await
                && failure.is_none()
            {
                failure = Some(e);
            }
        }
        match failure {
            Some(e) => StepOutcome::failed(step, e),
            None => StepOutcome::ok(step, StepStatus::Succeeded),
        }
    }

    /// Facts step: writes each `MemoryRecord` via the memory service, deriving a
    /// per-record idempotency key `{batch_key}#{index}`. This derivation is
    /// load-bearing: memory's idempotency lookup binds the full scope and key with no per-record disambiguation, so reusing the batch
    /// key would dedupe records 2..N against record 1 (data loss). With derived
    /// keys, N distinct records all land on first ingest; re-ingest reproduces
    /// the same keys → the store deduplicates → [`StepStatus::Deduplicated`].
    ///
    /// Step status: empty → `Skipped`; all records deduplicated → `Deduplicated`;
    /// any record freshly written (none failed) → `Succeeded`; any record
    /// failed → `Failed` (the remaining records are still attempted so every
    /// record that can land does).
    ///
    /// A **sourced** batch runs the supersession executor instead (prior facts
    /// tombstoned, facts stamped + ledgered; same key derivation); an unsourced
    /// batch takes this path byte-identically to before.
    async fn step_facts(&self, request: &BatchIngestRequest) -> StepOutcome {
        let step = BatchStep::Facts;
        if request
            .source_id
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            let result =
                run_facts_step(self.memory.as_ref(), self.knowledge.as_ref(), request).await;
            return sourced_outcome(step, request.facts.is_empty(), result);
        }
        if request.facts.is_empty() {
            return StepOutcome::ok(step, StepStatus::Skipped);
        }
        let mut failure: Option<CoreError> = None;
        let mut any_written = false;
        let mut all_deduplicated = true;
        for (index, record) in request.facts.iter().enumerate() {
            if self.memory.stopped() {
                break;
            }
            // Per-record key: deterministic, so re-ingest reproduces it and
            // dedupes correctly. Distinct per index, so N records all land.
            let per_record_key = format!("{}#{}", request.idempotency_key, index);
            let write_request = WriteMemoryRequest {
                kind: record.kind.clone(),
                content: record.content.clone(),
                scope: request.scope.clone(),
                requester: requester_from(record.provenance.actor.clone()),
                provenance: record.provenance.clone(),
                policy: record.policy.clone(),
                links: record.links.clone(),
                verdict: record.verdict,
                idempotency_key: Some(per_record_key),
            };
            match self.memory.write_memory(write_request).await {
                Ok(response) => {
                    if response.deduplicated == Some(true) {
                        // deduplicated — stays counted as deduped
                    } else {
                        any_written = true;
                        all_deduplicated = false;
                    }
                }
                Err(e) => {
                    all_deduplicated = false;
                    if failure.is_none() {
                        failure = Some(e);
                    }
                }
            }
        }
        match failure {
            Some(e) => StepOutcome::failed(step, e),
            None if all_deduplicated && !any_written => {
                StepOutcome::ok(step, StepStatus::Deduplicated)
            }
            None => StepOutcome::ok(step, StepStatus::Succeeded),
        }
    }

    /// Entities step: upserts each entity via the knowledge repository (by
    /// record id). Re-ingest overwrites → [`StepStatus::Succeeded`] (the store
    /// has no key-based dedup, so `Deduplicated` is never reported here).
    /// A **sourced** batch runs the supersession executor instead.
    async fn step_entities(&self, request: &BatchIngestRequest) -> StepOutcome {
        if request
            .source_id
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            let result =
                run_entities_step(self.knowledge.as_ref(), self.knowledge.as_ref(), request).await;
            return sourced_outcome(BatchStep::Entities, request.entities.is_empty(), result);
        }
        write_entities(
            BatchStep::Entities,
            &request.entities,
            &request.scope,
            &self.knowledge,
        )
        .await
    }

    /// Relationships step: upserts each relationship via the knowledge
    /// repository (by record id). Re-ingest overwrites → `Succeeded`.
    /// A **sourced** batch runs the supersession executor instead.
    async fn step_relationships(&self, request: &BatchIngestRequest) -> StepOutcome {
        if request
            .source_id
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            let result =
                run_relationships_step(self.knowledge.as_ref(), self.knowledge.as_ref(), request)
                    .await;
            return sourced_outcome(
                BatchStep::Relationships,
                request.relationships.is_empty(),
                result,
            );
        }
        write_relationships(
            BatchStep::Relationships,
            &request.relationships,
            &request.scope,
            &self.knowledge,
        )
        .await
    }
}

#[async_trait]
impl BatchIngest for SharedBatchIngest {
    fn transaction_guarantee(&self) -> TransactionGuarantee {
        TransactionGuarantee::BestEffort
    }

    async fn ingest(&self, request: BatchIngestRequest) -> CoreResult<BatchOutcome> {
        // Fixed step order (Episode → Facts → Entities → Relationships →
        // Evidence → Embeddings). A failed step is recorded and the batch
        // proceeds — best-effort, no rollback of succeeded steps.
        let session = self.storage.session(&request.scope)?;
        if session.scope() != &request.scope {
            return Err(CoreError::PolicyDenied {
                reason: "composition session scope mismatch".into(),
            });
        }
        let observed = Arc::new(ObservedPorts::new(session));
        let execution = BatchExecution {
            memory: observed.clone(),
            knowledge: observed,
        };
        let mut steps = Vec::with_capacity(ALL_STEPS.len());
        for step in [
            BatchStep::Episode,
            BatchStep::Facts,
            BatchStep::Entities,
            BatchStep::Relationships,
        ] {
            if execution.memory.stopped() {
                steps.push(StepOutcome::ok(step, StepStatus::Skipped));
                continue;
            }
            let mut outcome = match step {
                BatchStep::Episode => execution.step_episode(&request).await,
                BatchStep::Facts => execution.step_facts(&request).await,
                BatchStep::Entities => execution.step_entities(&request).await,
                BatchStep::Relationships => execution.step_relationships(&request).await,
                _ => unreachable!("fixed implemented steps"),
            };
            outcome.error = execution.memory.supersede_failure(outcome.error.take());
            if outcome.error.is_some() {
                outcome.status = StepStatus::Failed;
            }
            steps.push(outcome);
        }
        // Evidence is not a separate batch write in v1 — honest `Skipped`, never
        // silent. Evidence is embedded in the records' `Provenance.evidence` and
        // lands via the existing per-store writes; attaching evidence to records
        // *not* in the batch is the ADR-0023 write-side on `ProvenanceQuery`
        // (a port-level rewrite), not a batch step.
        steps.push(StepOutcome::ok(BatchStep::Evidence, StepStatus::Skipped));
        // Skipped per ADR-0024 (deferred reindex model): the batch records
        // `EmbeddingRef` metadata via Provenance; actual vectors are generated by
        // a separate reindex job via `VectorIndex`.
        steps.push(StepOutcome::ok(BatchStep::Embeddings, StepStatus::Skipped));

        let status = aggregate_status(&steps);
        Ok(BatchOutcome {
            guarantee: TransactionGuarantee::BestEffort,
            status,
            steps,
        })
    }
}

/// Writes the entities step: upserts each entity via the knowledge repository
/// (by record id). Re-ingest overwrites → [`StepStatus::Succeeded`] (the store
/// has no key-based dedup). The batch `scope` is authoritative — each record is
/// re-scoped to it. An empty payload reports [`StepStatus::Skipped`].
async fn write_entities(
    step: BatchStep,
    records: &[KnowledgeEntity],
    scope: &Scope,
    knowledge: &Arc<ObservedPorts>,
) -> StepOutcome {
    if records.is_empty() {
        return StepOutcome::ok(step, StepStatus::Skipped);
    }
    let mut failure: Option<CoreError> = None;
    for record in records {
        if knowledge.stopped() {
            break;
        }
        let mut entity = record.clone();
        entity.scope = scope.clone();
        if let Err(e) = knowledge.put_entity(entity).await
            && failure.is_none()
        {
            failure = Some(e);
        }
    }
    match failure {
        Some(e) => StepOutcome::failed(step, e),
        None => StepOutcome::ok(step, StepStatus::Succeeded),
    }
}

/// Writes the relationships step: upserts each relationship via the knowledge
/// repository (by record id). Same shape and semantics as [`write_entities`].
async fn write_relationships(
    step: BatchStep,
    records: &[KnowledgeRelationship],
    scope: &Scope,
    knowledge: &Arc<ObservedPorts>,
) -> StepOutcome {
    if records.is_empty() {
        return StepOutcome::ok(step, StepStatus::Skipped);
    }
    let mut failure: Option<CoreError> = None;
    for record in records {
        if knowledge.stopped() {
            break;
        }
        let mut relationship = record.clone();
        relationship.scope = scope.clone();
        if let Err(e) = knowledge.put_relationship(relationship).await
            && failure.is_none()
        {
            failure = Some(e);
        }
    }
    match failure {
        Some(e) => StepOutcome::failed(step, e),
        None => StepOutcome::ok(step, StepStatus::Succeeded),
    }
}

/// Builds a [`Requester`] from a record's provenance [`Actor`]. The memory
/// authorizer is allow-all in v1, but `memory.write` is declared honestly.
fn requester_from(actor: Actor) -> Requester {
    Requester {
        actor,
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string()],
        on_behalf_of: None,
    }
}

/// Puts the sourced-batch registry `KnowledgeSource` with a collision guard
/// (FIND-security-003): `put_source` upserts on the GLOBAL id, so a
/// caller-supplied `source_id` that collides with an existing registry row of
/// a different scope envelope must never re-scope or overwrite it — the put
/// is skipped and the mismatch surfaced as a failure instead. An existing
/// row in the same scope envelope (canonical `scope_allows` in both
/// directions, i.e. five-field equality) is updated in place on
/// `name`/`updated_at` only, keeping its kind, uri, version, policy,
/// provenance, created_at, and metadata.
async fn put_registry_source(
    knowledge: &ObservedPorts,
    registry: KnowledgeSource,
) -> CoreResult<()> {
    let existing = knowledge.registry_source(&registry.id).await?;
    let source = match existing {
        None => registry,
        Some(existing) => {
            if existing.scope != registry.scope {
                return Err(CoreError::PolicyDenied {
                    reason: format!(
                        "source_id {} is already registered in a different scope; \
                         registry write skipped",
                        registry.id
                    ),
                });
            }
            let mut updated = existing;
            updated.name = registry.name;
            updated.updated_at = Some(Utc::now());
            updated
        }
    };
    knowledge.put_source(source).await.map(|_| ())
}

/// Projects a sourced step result into a [`StepOutcome`]: the derived status
/// plus the supersession counts (attached even when `Failed` — the counts for
/// what did land are surfaced, never hidden).
fn sourced_outcome(
    step: BatchStep,
    empty_payload: bool,
    result: SourcedStepOutcome,
) -> StepOutcome {
    let status = result.status(empty_payload);
    let counts = result.counts;
    match result.failure {
        Some(e) => StepOutcome::failed(step, e).with_counts(counts),
        None => StepOutcome::ok(step, status).with_counts(counts),
    }
}

#[cfg(test)]
mod tests {
    //! The BatchExecution integration tests live in
    //! `adapters/integration/tests/batch_ingest.rs` so they can share the
    //! fixture helpers. This module is reserved for any future inline unit
    //! tests that do not require a store.
}

pub struct SharedBatchIngest {
    storage: Arc<dyn StorageComposition>,
}
impl SharedBatchIngest {
    pub fn new(storage: Arc<dyn StorageComposition>) -> Self {
        Self { storage }
    }
}
