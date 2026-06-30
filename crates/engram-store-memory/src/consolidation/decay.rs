//! Policy-expiry decay for in-memory consolidation.
//!
//! This module owns policy-expiry lifecycle mutation for the in-memory adapter.
//! It marks due active records expired and emits audit events, while leaving
//! pruning, redaction, deletion, confidence decay, and scheduling to separate
//! future specs.

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Marks scoped active memories expired when their policy expiry is due.
///
/// Legal-hold records are skipped even if their `expires_at` timestamp has
/// passed. The due decision uses the consolidation run start timestamp so all
/// records in a run are evaluated against one stable clock boundary.
pub(super) fn expire_due_memories(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut read_count = 0_u64;
    let mut skipped_count = 0_u64;
    let mut expired_ids = Vec::new();

    for record in state.memories.values() {
        if !scope_allows(&record.scope, &request.scope) {
            continue;
        }
        read_count += 1;
        if decay_candidate(record, started_at) {
            expired_ids.push(record.id.clone());
        } else {
            skipped_count += 1;
        }
    }

    for expired_id in &expired_ids {
        let event = {
            let Some(record) = state.memories.get_mut(expired_id.as_str()) else {
                continue;
            };
            record.status = MemoryStatus::Expired;
            record.updated_at = Some(now);
            expired_event(service, request, record, now)
        };
        state.events.push(event);
    }

    let expired_count = expired_ids.len() as u64;
    add_counter(&mut stats.memories_read, read_count);
    stats.memories_written = Some(0);
    add_counter(&mut stats.records_decayed, expired_count);
    stats.model_calls = Some(0);

    Ok(ConsolidationTaskResult {
        task: ConsolidationTaskKind::Decay,
        status: ConsolidationTaskStatus::Completed,
        started_at,
        completed_at: Some(now),
        items_read: Some(read_count),
        items_written: Some(0),
        items_updated: Some(expired_count),
        items_skipped: Some(skipped_count),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: expired_ids.into_iter().map(memory_ref).collect(),
    })
}

fn decay_candidate(record: &MemoryRecord, now: Timestamp) -> bool {
    if record.status != MemoryStatus::Active {
        return false;
    }
    if record.policy.retention == Retention::LegalHold {
        return false;
    }
    record
        .policy
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
}

fn expired_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    record: &MemoryRecord,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::Expired,
        scope: record.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(record.id.clone()),
        payload: json!({
            "reason": "policy_expiry_decay",
            "expiredMemoryId": record.id.to_string(),
            "policyExpiresAt": record.policy.expires_at.map(|timestamp| timestamp.to_rfc3339()),
        }),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![memory_ref(record.id.clone())],
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("decay".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    }
}
