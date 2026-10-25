//! Sourced-batch supersession executor (job 001-kg-source-provenance).
//!
//! Executes the sourced branches of the Facts / Entities / Relationships batch
//! steps: stamp every written assertion with the source `EvidenceRef`,
//! supersede the same source's prior batches (declared-set reconciliation over
//! ledger ids — never idempotency-key equality, never entity name), and record
//! what the current batch wrote in the source batch ledger.
//!
//! Semantics (per the accepted plan / domain doc):
//! - **Facts**: prior memory ids of other batch keys are forgotten with
//!   `DeleteMode::Tombstone` (reversible; keeps the lifecycle trail). Facts are
//!   a declared set — the new batch re-writes its facts wholesale, so every
//!   prior row is retracted, which is also what makes reordered identical
//!   facts converge (the reorder trap).
//! - **Entities / Relationships**: re-asserted records merge refs
//!   (read-modify-write union) and are restored if previously archived;
//!   non-re-asserted records lose this source's ref and archive only when the
//!   **last** ref goes (ref-counted shared records, ADR-0018/0027).
//! - **Immunity guard**: a prior record is touched only if its persisted refs
//!   still contain the source ref — records attributed another way (e.g.
//!   code-scan records keyed by `graph_id` / `stable_source_key`, or a foreign
//!   record adopted through a per-record idempotency-key collision) are
//!   skipped and counted `immune_skipped`, never matched by name or id alone.
//!   Facts carry the guard on `Provenance.evidence`; a forget that returns
//!   `NotFound` (record gone) is never counted as a forgotten fact.
//! - **Best-effort honesty**: every supersession item soft-fails per record
//!   (`counts.failed`, first error surfaced); a failed item's ledger row is
//!   **not** marked superseded, so re-running the same source converges via
//!   the ledger. No rollback of succeeded items is claimed or attempted.
//!
//! Concurrency stance (RISK-001, contract-entity precedent): the store
//! serializes individual calls but not read-modify-write pairs — a per-scope
//! **single writer** is assumed for sourced batches; concurrent same-scope
//! distill is outside the supported envelope.
//!
//! The functions are generic over the ports so tests can drive them with
//! stubs; the orchestrator (`sqlite::batch`) passes the concrete SQLite
//! stores. This module lives in the ADR-0022-exempt engine cell.

use std::collections::HashSet;

use chrono::Utc;
use engram_domain::{
    Actor, ActorKind, AllowedUse, DeleteMode, EntityId, EvidenceRef, EvidenceTargetType,
    ForgetRequest, ForgetStatus, ForgetTargetType, Id, KnowledgeEntity, KnowledgeRelationship,
    KnowledgeSource, MemoryId, MemoryRecord, Policy, Provenance, RelationshipId, Requester,
    Retention, Scope, Sensitivity, SourceKind, Visibility, WriteMemoryRequest,
};
use engram_knowledge::{
    KnowledgeRepository, SourceBatchLedger, SourceBatchRecord, SourceBatchRecordKind,
};
use engram_memory::MemoryService;
use engram_runtime::CoreError;

use crate::{BatchIngestRequest, StepStatus, SupersessionCounts};

/// Outcome of one sourced step: per-item failure (first error, typed), the
/// supersession counts to surface, and the write-side facts the orchestrator
/// needs to derive the legacy step status.
#[derive(Debug, Default)]
pub struct SourcedStepOutcome {
    /// First per-item error (write or supersession), if any. Succeeded items
    /// stay landed (no rollback).
    pub failure: Option<CoreError>,
    /// Supersession counts for this step (surfaced on the `StepOutcome`).
    pub counts: SupersessionCounts,
    /// Any record freshly written (not deduplicated).
    pub any_written: bool,
    /// All writes landed via dedup (only meaningful when a payload was
    /// present; starts `false`).
    pub all_deduplicated: bool,
}

impl SourcedStepOutcome {
    /// Derives the step status: `Failed` on any per-item error; `Succeeded`
    /// when supersession had effects; else the legacy write-based rules
    /// (`Skipped` for an empty payload with nothing superseded,
    /// `Deduplicated` when every write deduped, `Succeeded` otherwise).
    pub fn status(&self, empty_payload: bool) -> StepStatus {
        if self.failure.is_some() {
            return StepStatus::Failed;
        }
        if self.counts.has_effects() {
            return StepStatus::Succeeded;
        }
        if empty_payload {
            return StepStatus::Skipped;
        }
        if self.all_deduplicated && !self.any_written {
            StepStatus::Deduplicated
        } else {
            StepStatus::Succeeded
        }
    }
}

/// The sourced source identity of this request, or `None` when unsourced.
/// An empty/whitespace string is treated as unsourced (the tool edge does the
/// same).
fn sourced_id(request: &BatchIngestRequest) -> Option<&str> {
    request
        .source_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
}

/// The registry `KnowledgeSource` row a sourced batch puts in the Episode step
/// — revision-stable id by construction, unlike a content-hash-derived id.
pub fn registry_source(source_id: &str, scope: &Scope) -> KnowledgeSource {
    KnowledgeSource {
        id: Id::from(source_id),
        kind: SourceKind::Upload,
        scope: scope.clone(),
        name: source_id.to_owned(),
        uri: None,
        version: None,
        repository_key: None,
        git_ref: None,
        policy: default_policy(),
        provenance: engine_provenance("source_registry"),
        created_at: Utc::now(),
        updated_at: None,
        metadata: None,
    }
}

/// True when `refs` carries a source ref targeting `source_id` — the
/// supersession membership guard (records without the ref are immune).
fn refs_contain_source(refs: &[EvidenceRef], source_id: &str) -> bool {
    refs.iter().any(|r| {
        r.target_type == EvidenceTargetType::Source && r.target_id.as_deref() == Some(source_id)
    })
}

/// The source `EvidenceRef` stamped on every assertion a sourced batch writes.
fn source_evidence_ref(source_id: &str) -> EvidenceRef {
    EvidenceRef {
        target_type: EvidenceTargetType::Source,
        target_id: Some(source_id.to_owned()),
        uri: None,
        quote: None,
        location: None,
    }
}

/// Stamps a fact's `Provenance.evidence` with the source ref (idempotent
/// union — a retried payload never duplicates the ref).
fn stamp_fact(record: &mut MemoryRecord, source_id: &str) {
    let stamp = source_evidence_ref(source_id);
    if !record.provenance.evidence.contains(&stamp) {
        record.provenance.evidence.push(stamp);
    }
}

/// Stamps an entity's `source_refs` (idempotent union).
fn stamp_entity(entity: &mut KnowledgeEntity, source_id: &str) {
    let stamp = source_evidence_ref(source_id);
    if !entity.source_refs.contains(&stamp) {
        entity.source_refs.push(stamp);
    }
}

/// Stamps a relationship's `evidence` (idempotent union).
fn stamp_relationship(relationship: &mut KnowledgeRelationship, source_id: &str) {
    let stamp = source_evidence_ref(source_id);
    if !relationship.evidence.contains(&stamp) {
        relationship.evidence.push(stamp);
    }
}

/// Unions `prior` refs onto `incoming` (no duplicates, order stable) — the
/// read-modify-write merge that keeps other sources' refs (no clobber).
fn union_refs(incoming: &mut Vec<EvidenceRef>, prior: &[EvidenceRef]) {
    for r in prior {
        if !incoming.contains(r) {
            incoming.push(r.clone());
        }
    }
}

/// Requester for sourced memory writes (permission declared honestly).
fn write_requester(actor: Actor) -> Requester {
    Requester {
        actor,
        roles: Vec::new(),
        permissions: vec!["memory.write".to_owned()],
        on_behalf_of: None,
    }
}

/// Requester for supersession forgets — the batch engine acting on behalf of
/// the source's distiller. The prior records' own actors are not fetched (no
/// per-id read on the memory port here), so a system actor attributes the
/// retraction; `memory.forget` is declared alongside `memory.write`.
fn forget_requester() -> Requester {
    Requester {
        actor: supersession_actor(),
        roles: Vec::new(),
        permissions: vec!["memory.write".to_owned(), "memory.forget".to_owned()],
        on_behalf_of: None,
    }
}

fn supersession_actor() -> Actor {
    Actor {
        id: Id::from("engram-batch-supersession"),
        kind: ActorKind::System,
        display_name: Some("engram batch supersession".to_owned()),
        metadata: None,
    }
}

fn engine_provenance(method: &str) -> Provenance {
    Provenance {
        source: "engram-batch".to_owned(),
        actor: supersession_actor(),
        observed_at: Utc::now(),
        evidence: Vec::new(),
        derivations: Vec::new(),
        confidence: Some(1.0),
        method: Some(method.to_owned()),
    }
}

fn default_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

/// Soft-fail helper: records the first error and bumps `failed`.
fn soft_fail(out: &mut SourcedStepOutcome, error: CoreError) {
    out.counts.failed += 1;
    if out.failure.is_none() {
        out.failure = Some(error);
    }
}

/// Lists the source's unsuperseded prior rows; a ledger read failure is
/// soft-failed and yields no rows (nothing can be superseded this pass; a
/// retry re-reads the ledger and converges).
async fn prior_rows<L>(
    ledger: &L,
    scope: &Scope,
    source_id: &str,
    out: &mut SourcedStepOutcome,
) -> Vec<SourceBatchRecord>
where
    L: SourceBatchLedger + ?Sized,
{
    match ledger.list_source_batch_records(scope, source_id).await {
        Ok(rows) => rows,
        Err(e) => {
            soft_fail(out, e);
            Vec::new()
        }
    }
}

/// Marks one ledger row superseded (soft-fail). Called only after the record
/// reached a terminal state (forgotten / archived / ref-removed / immune), so
/// a retry never redoes work — the second-pass idempotency anchor.
async fn mark_superseded<L>(
    ledger: &L,
    scope: &Scope,
    source_id: &str,
    row: &SourceBatchRecord,
    out: &mut SourcedStepOutcome,
) where
    L: SourceBatchLedger + ?Sized,
{
    if let Err(e) = ledger
        .mark_source_batch_superseded(scope, source_id, &row.batch_key, row.kind, &row.record_id)
        .await
    {
        soft_fail(out, e);
    }
}

/// Records the current batch's written ids in the ledger (soft-fail).
async fn record_writes<L>(
    ledger: &L,
    scope: &Scope,
    source_id: &str,
    batch_key: &str,
    kind: SourceBatchRecordKind,
    ids: &[String],
    out: &mut SourcedStepOutcome,
) where
    L: SourceBatchLedger + ?Sized,
{
    if ids.is_empty() {
        return;
    }
    if let Err(e) = ledger
        .record_source_batch_writes(scope, source_id, batch_key, kind, ids)
        .await
    {
        soft_fail(out, e);
    }
}

/// Facts step (sourced): supersede prior facts (tombstone forget), then write
/// the declared facts with the unchanged `{batch_key}#{index}` per-record key
/// derivation plus the source stamp, then ledger the written memory ids.
///
/// The per-record key derivation is load-bearing and is NEVER reused as a
/// supersession matching key (atomic-batch-ingest invariant) — prior facts are
/// matched by ledger id only, which is what makes reordered facts converge.
pub async fn run_facts_step<M, L>(
    memory: &M,
    ledger: &L,
    request: &BatchIngestRequest,
) -> SourcedStepOutcome
where
    M: MemoryService + ?Sized,
    L: SourceBatchLedger + ?Sized,
{
    let mut out = SourcedStepOutcome {
        all_deduplicated: true,
        ..SourcedStepOutcome::default()
    };
    let Some(source_id) = sourced_id(request) else {
        return out;
    };
    let scope = &request.scope;

    // 1. Supersede the source's prior facts (other batch keys, unsuperseded).
    let rows = prior_rows(ledger, scope, source_id, &mut out).await;
    for row in rows.into_iter().filter(|r| {
        r.kind == SourceBatchRecordKind::MemoryRecord && r.batch_key != request.idempotency_key
    }) {
        out.counts.examined += 1;
        // Immunity guard (mirror of the entity/relationship guard): fetch the
        // record first. A ledger row whose record no longer exists retires as
        // immune (nothing to retract, never counted forgotten); a record whose
        // `Provenance.evidence` does not carry this source's ref is NEVER
        // forgotten — e.g. a foreign record adopted into this source's ledger
        // through a `{batch_key}#{index}` idempotency-key collision keeps its
        // owning source's ref and stays active (FIND-security-001).
        let prior = match memory
            .get_memory(&MemoryId::from(row.record_id.clone()), scope)
            .await
        {
            Ok(prior) => prior,
            Err(e) => {
                soft_fail(&mut out, e);
                continue;
            }
        };
        let Some(prior) = prior else {
            out.counts.immune_skipped += 1;
            mark_superseded(ledger, scope, source_id, &row, &mut out).await;
            continue;
        };
        if !refs_contain_source(&prior.provenance.evidence, source_id) {
            out.counts.immune_skipped += 1;
            mark_superseded(ledger, scope, source_id, &row, &mut out).await;
            continue;
        }
        let forget = ForgetRequest {
            target_type: ForgetTargetType::Memory,
            target_id: row.record_id.clone(),
            scope: scope.clone(),
            requester: forget_requester(),
            mode: DeleteMode::Tombstone,
            reason: Some(format!(
                "superseded by batch {} for source {}",
                request.idempotency_key, source_id
            )),
        };
        match memory.forget(forget).await {
            // A policy denial is a surfaced failure, not a retraction; the
            // ledger row stays unsuperseded so a retry re-attempts it.
            Ok(result) if result.status == ForgetStatus::Denied => {
                out.counts.failed += 1;
                if out.failure.is_none() {
                    out.failure = Some(CoreError::PolicyDenied {
                        reason: format!("forget of prior fact {} was denied", row.record_id),
                    });
                }
            }
            // The record vanished between the guard read and the forget:
            // nothing was retracted — retire the row without counting a
            // forgotten fact (NotFound is not a retraction).
            Ok(result) if result.status == ForgetStatus::NotFound => {
                out.counts.immune_skipped += 1;
                mark_superseded(ledger, scope, source_id, &row, &mut out).await;
            }
            Ok(_) => {
                out.counts.forgotten_facts += 1;
                mark_superseded(ledger, scope, source_id, &row, &mut out).await;
            }
            Err(e) => soft_fail(&mut out, e),
        }
    }

    // 2. Write the declared facts (stamped). Key derivation unchanged.
    let mut written_ids: Vec<String> = Vec::with_capacity(request.facts.len());
    for (index, record) in request.facts.iter().enumerate() {
        let per_record_key = format!("{}#{}", request.idempotency_key, index);
        let mut stamped = record.clone();
        stamp_fact(&mut stamped, source_id);
        let write_request = WriteMemoryRequest {
            kind: stamped.kind.clone(),
            content: stamped.content.clone(),
            scope: scope.clone(),
            requester: write_requester(stamped.provenance.actor.clone()),
            provenance: stamped.provenance.clone(),
            policy: stamped.policy.clone(),
            links: stamped.links.clone(),
            idempotency_key: Some(per_record_key),
        };
        match memory.write_memory(write_request).await {
            Ok(response) => {
                if response.deduplicated != Some(true) {
                    out.any_written = true;
                    out.all_deduplicated = false;
                }
                // The engine-assigned id (or the stored id on dedup) is what
                // a later supersession pass must be able to find.
                written_ids.push(response.record.id.to_string());
            }
            Err(e) => {
                out.all_deduplicated = false;
                soft_fail(&mut out, e);
            }
        }
    }

    // 3. Ledger the written memory ids (idempotent upsert).
    record_writes(
        ledger,
        scope,
        source_id,
        &request.idempotency_key,
        SourceBatchRecordKind::MemoryRecord,
        &written_ids,
        &mut out,
    )
    .await;
    out
}

/// Entities step (sourced): supersede non-re-asserted prior entities
/// (ref-remove; archive only when the last ref goes), then write the declared
/// entities (stamp + ref union + restore-if-archived), then ledger the ids.
pub async fn run_entities_step<K, L>(
    knowledge: &K,
    ledger: &L,
    request: &BatchIngestRequest,
) -> SourcedStepOutcome
where
    K: KnowledgeRepository + ?Sized,
    L: SourceBatchLedger + ?Sized,
{
    let mut out = SourcedStepOutcome::default();
    let Some(source_id) = sourced_id(request) else {
        return out;
    };
    let scope = &request.scope;
    let reasserted: HashSet<String> = request.entities.iter().map(|e| e.id.to_string()).collect();

    supersede_prior_entities(
        knowledge,
        ledger,
        scope,
        source_id,
        &request.idempotency_key,
        &reasserted,
        &mut out,
    )
    .await;

    let mut written: Vec<String> = Vec::with_capacity(request.entities.len());
    for record in &request.entities {
        let mut entity = record.clone();
        entity.scope = scope.clone();
        stamp_entity(&mut entity, source_id);
        if write_sourced_entity(knowledge, scope, entity, &mut out).await {
            out.any_written = true;
            written.push(record.id.to_string());
        }
    }
    record_writes(
        ledger,
        scope,
        source_id,
        &request.idempotency_key,
        SourceBatchRecordKind::Entity,
        &written,
        &mut out,
    )
    .await;
    out
}

/// Relationships step (sourced): same shape as [`run_entities_step`] over
/// relationship `evidence` refs.
pub async fn run_relationships_step<K, L>(
    knowledge: &K,
    ledger: &L,
    request: &BatchIngestRequest,
) -> SourcedStepOutcome
where
    K: KnowledgeRepository + ?Sized,
    L: SourceBatchLedger + ?Sized,
{
    let mut out = SourcedStepOutcome::default();
    let Some(source_id) = sourced_id(request) else {
        return out;
    };
    let scope = &request.scope;
    let reasserted: HashSet<String> = request
        .relationships
        .iter()
        .map(|r| r.id.to_string())
        .collect();

    supersede_prior_relationships(
        knowledge,
        ledger,
        scope,
        source_id,
        &request.idempotency_key,
        &reasserted,
        &mut out,
    )
    .await;

    let mut written: Vec<String> = Vec::with_capacity(request.relationships.len());
    for record in &request.relationships {
        let mut relationship = record.clone();
        relationship.scope = scope.clone();
        stamp_relationship(&mut relationship, source_id);
        if write_sourced_relationship(knowledge, scope, relationship, &mut out).await {
            out.any_written = true;
            written.push(record.id.to_string());
        }
    }
    record_writes(
        ledger,
        scope,
        source_id,
        &request.idempotency_key,
        SourceBatchRecordKind::Relationship,
        &written,
        &mut out,
    )
    .await;
    out
}

/// Declared-set supersession over the source's prior entities. Invariants: a
/// record is retracted only if its persisted `source_refs` contain the source
/// ref (immune otherwise); archived only when its last ref is removed;
/// re-asserted rows are skipped (the write path re-records them under the
/// current batch key); a failed item keeps its ledger row unsuperseded so a
/// retry converges.
async fn supersede_prior_entities<K, L>(
    knowledge: &K,
    ledger: &L,
    scope: &Scope,
    source_id: &str,
    current_batch_key: &str,
    reasserted: &HashSet<String>,
    out: &mut SourcedStepOutcome,
) where
    K: KnowledgeRepository + ?Sized,
    L: SourceBatchLedger + ?Sized,
{
    let rows = prior_rows(ledger, scope, source_id, out).await;
    let now = Utc::now();
    for row in rows
        .into_iter()
        .filter(|r| r.kind == SourceBatchRecordKind::Entity && r.batch_key != current_batch_key)
    {
        out.counts.examined += 1;
        if reasserted.contains(&row.record_id) {
            // Re-asserted: the write path re-records it under the current key;
            // retire the old key's row so one active row remains per record.
            mark_superseded(ledger, scope, source_id, &row, out).await;
            continue;
        }
        match knowledge
            .get_entity(&EntityId::from(row.record_id.clone()), scope)
            .await
        {
            Ok(None) => {
                // Record gone (hard-deleted outside this flow): nothing to
                // retract, row retires.
                out.counts.immune_skipped += 1;
                mark_superseded(ledger, scope, source_id, &row, out).await;
            }
            Ok(Some(prior)) => {
                if !refs_contain_source(&prior.source_refs, source_id) {
                    // Immunity guard: never matched by name/id — only ref
                    // membership (code-scan records, foreign-attribution).
                    out.counts.immune_skipped += 1;
                    mark_superseded(ledger, scope, source_id, &row, out).await;
                    continue;
                }
                let mut updated = prior;
                updated.source_refs.retain(|r| {
                    !(r.target_type == EvidenceTargetType::Source
                        && r.target_id.as_deref() == Some(source_id))
                });
                updated.updated_at = Some(now);
                updated.provenance.observed_at = now;
                updated.provenance.method = Some("superseded".to_owned());
                let archive_now = updated.source_refs.is_empty();
                if archive_now {
                    updated.archived_at = Some(now);
                }
                match knowledge.put_entity(updated).await {
                    Ok(_) => {
                        if archive_now {
                            out.counts.archived_entities += 1;
                        } else {
                            out.counts.refs_removed += 1;
                        }
                        mark_superseded(ledger, scope, source_id, &row, out).await;
                    }
                    Err(e) => soft_fail(out, e),
                }
            }
            Err(e) => soft_fail(out, e),
        }
    }
}

/// Declared-set supersession over the source's prior relationships — the
/// relationship mirror of [`supersede_prior_entities`] (evidence refs).
async fn supersede_prior_relationships<K, L>(
    knowledge: &K,
    ledger: &L,
    scope: &Scope,
    source_id: &str,
    current_batch_key: &str,
    reasserted: &HashSet<String>,
    out: &mut SourcedStepOutcome,
) where
    K: KnowledgeRepository + ?Sized,
    L: SourceBatchLedger + ?Sized,
{
    let rows = prior_rows(ledger, scope, source_id, out).await;
    let now = Utc::now();
    for row in rows.into_iter().filter(|r| {
        r.kind == SourceBatchRecordKind::Relationship && r.batch_key != current_batch_key
    }) {
        out.counts.examined += 1;
        if reasserted.contains(&row.record_id) {
            mark_superseded(ledger, scope, source_id, &row, out).await;
            continue;
        }
        match knowledge
            .get_relationship(&RelationshipId::from(row.record_id.clone()), scope)
            .await
        {
            Ok(None) => {
                out.counts.immune_skipped += 1;
                mark_superseded(ledger, scope, source_id, &row, out).await;
            }
            Ok(Some(prior)) => {
                if !refs_contain_source(&prior.evidence, source_id) {
                    out.counts.immune_skipped += 1;
                    mark_superseded(ledger, scope, source_id, &row, out).await;
                    continue;
                }
                let mut updated = prior;
                updated.evidence.retain(|r| {
                    !(r.target_type == EvidenceTargetType::Source
                        && r.target_id.as_deref() == Some(source_id))
                });
                updated.updated_at = Some(now);
                updated.provenance.observed_at = now;
                updated.provenance.method = Some("superseded".to_owned());
                let archive_now = updated.evidence.is_empty();
                if archive_now {
                    updated.archived_at = Some(now);
                }
                match knowledge.put_relationship(updated).await {
                    Ok(_) => {
                        if archive_now {
                            out.counts.archived_relationships += 1;
                        } else {
                            out.counts.refs_removed += 1;
                        }
                        mark_superseded(ledger, scope, source_id, &row, out).await;
                    }
                    Err(e) => soft_fail(out, e),
                }
            }
            Err(e) => soft_fail(out, e),
        }
    }
}

/// Writes one sourced entity: incoming content is authoritative, refs union
/// with the prior record's (other sources keep their refs — no clobber), and a
/// prior archive is restored (a plain put preserves `archived_at`). Returns
/// whether the record landed.
async fn write_sourced_entity<K>(
    knowledge: &K,
    scope: &Scope,
    mut entity: KnowledgeEntity,
    out: &mut SourcedStepOutcome,
) -> bool
where
    K: KnowledgeRepository + ?Sized,
{
    let prior = match knowledge.get_entity(&entity.id, scope).await {
        Ok(prior) => prior,
        Err(e) => {
            soft_fail(out, e);
            return false;
        }
    };
    let mut restore_needed = false;
    if let Some(prior) = &prior {
        union_refs(&mut entity.source_refs, &prior.source_refs);
        restore_needed = prior.archived_at.is_some();
    }
    if let Err(e) = knowledge.put_entity(entity).await {
        soft_fail(out, e);
        return false;
    }
    if restore_needed {
        let id = prior.as_ref().expect("restore implies prior").id.clone();
        match knowledge.restore_entity(&id, scope).await {
            Ok(true) => out.counts.restored += 1,
            Ok(false) => {}
            Err(e) => {
                soft_fail(out, e);
                return false;
            }
        }
    }
    true
}

/// Writes one sourced relationship — the mirror of [`write_sourced_entity`].
async fn write_sourced_relationship<K>(
    knowledge: &K,
    scope: &Scope,
    mut relationship: KnowledgeRelationship,
    out: &mut SourcedStepOutcome,
) -> bool
where
    K: KnowledgeRepository + ?Sized,
{
    let prior = match knowledge.get_relationship(&relationship.id, scope).await {
        Ok(prior) => prior,
        Err(e) => {
            soft_fail(out, e);
            return false;
        }
    };
    let mut restore_needed = false;
    if let Some(prior) = &prior {
        union_refs(&mut relationship.evidence, &prior.evidence);
        restore_needed = prior.archived_at.is_some();
    }
    if let Err(e) = knowledge.put_relationship(relationship).await {
        soft_fail(out, e);
        return false;
    }
    if restore_needed {
        let id = prior.as_ref().expect("restore implies prior").id.clone();
        match knowledge.restore_relationship(&id, scope).await {
            Ok(true) => out.counts.restored += 1,
            Ok(false) => {}
            Err(e) => {
                soft_fail(out, e);
                return false;
            }
        }
    }
    true
}
