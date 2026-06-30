//! Assertion-backed belief synthesis for in-memory consolidation.
//!
//! This module owns deterministic conversion from explicit memory assertions to
//! belief records. It does not infer claims from free text, resolve entities,
//! detect contradictions, merge beliefs, or expose belief retrieval behavior.

use std::collections::BTreeSet;

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Synthesizes active beliefs from scoped active memory assertions.
///
/// Each assertion target is stable within a memory record and deduplicated
/// against active belief sources already stored in the in-memory adapter.
pub(super) fn synthesize_assertion_beliefs(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut read_count = 0_u64;
    let mut skipped_count = 0_u64;
    let mut created_memory_ids = Vec::new();

    let existing_assertion_targets = state
        .beliefs
        .values()
        .filter(|belief| {
            scope_allows(&belief.scope, &request.scope) && belief.status == BeliefStatus::Active
        })
        .flat_map(|belief| {
            belief
                .sources
                .iter()
                .filter(|source| source.target_type == BeliefSourceTargetType::Assertion)
                .map(|source| source.target_id.clone())
        })
        .collect::<BTreeSet<_>>();

    let candidates = state
        .memories
        .values()
        .filter_map(|record| {
            if !scope_allows(&record.scope, &request.scope) {
                return None;
            }
            read_count += 1;
            if !belief_candidate(record, started_at) {
                skipped_count += 1;
                return None;
            }
            Some(record.clone())
        })
        .collect::<Vec<_>>();

    for record in candidates {
        if record.assertions.is_empty() {
            skipped_count += 1;
            continue;
        }
        for (index, assertion) in record.assertions.iter().enumerate() {
            let assertion_id = assertion_target_id(&record.id, index);
            if existing_assertion_targets.contains(&assertion_id) {
                skipped_count += 1;
                continue;
            }
            let belief_id = service.ids.new_id("belief");
            let belief = synthesized_belief(
                belief_id.clone(),
                &record,
                assertion,
                assertion_id.clone(),
                request,
                now,
            );
            let event =
                belief_synthesized_event(service, request, &record, &belief_id, &assertion_id, now);
            state.beliefs.insert(belief_id.to_string(), belief);
            state.events.push(event);
            created_memory_ids.push(record.id.clone());
        }
    }

    let created_count = created_memory_ids.len() as u64;
    add_counter(&mut stats.memories_read, read_count);
    stats.memories_written = Some(0);
    add_counter(&mut stats.beliefs_synthesized, created_count);
    stats.model_calls = Some(0);

    Ok(ConsolidationTaskResult {
        task: ConsolidationTaskKind::BeliefSynthesis,
        status: ConsolidationTaskStatus::Completed,
        started_at,
        completed_at: Some(now),
        items_read: Some(read_count),
        items_written: Some(created_count),
        items_updated: Some(0),
        items_skipped: Some(skipped_count),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: created_memory_ids.into_iter().map(memory_ref).collect(),
    })
}

fn belief_candidate(record: &MemoryRecord, now: Timestamp) -> bool {
    if record.status != MemoryStatus::Active {
        return false;
    }
    record
        .policy
        .expires_at
        .is_none_or(|expires_at| expires_at > now)
}

fn assertion_target_id(memory_id: &MemoryId, index: usize) -> String {
    format!("{memory_id}#assertion-{index}")
}

fn synthesized_belief(
    belief_id: BeliefId,
    record: &MemoryRecord,
    assertion: &MemoryAssertion,
    assertion_id: String,
    request: &ConsolidationRequest,
    now: Timestamp,
) -> Belief {
    Belief {
        id: belief_id,
        scope: record.scope.clone(),
        subject: BeliefSubject {
            key: belief_subject_key(assertion),
            entity_ref: Some(assertion.subject.clone()),
            concept_ref: None,
            aliases: assertion.subject.aliases.clone(),
        },
        content: belief_content(assertion),
        status: BeliefStatus::Active,
        confidence: assertion
            .confidence
            .or(record.provenance.confidence)
            .unwrap_or(1.0),
        sources: vec![
            BeliefSource {
                target_type: BeliefSourceTargetType::Assertion,
                target_id: assertion_id,
                weight: Some(1.0),
                confidence: assertion.confidence,
                valid_from: assertion.valid_from,
                valid_until: assertion.valid_until,
            },
            BeliefSource {
                target_type: BeliefSourceTargetType::Memory,
                target_id: record.id.to_string(),
                weight: Some(1.0),
                confidence: record.provenance.confidence,
                valid_from: None,
                valid_until: record.policy.expires_at,
            },
        ],
        valid_from: assertion.valid_from,
        valid_until: assertion.valid_until.or(record.policy.expires_at),
        superseded_by: None,
        stale: Some(false),
        synthesizer: Some(DerivationRef {
            kind: DerivationKind::Consolidation,
            model: None,
            prompt_hash: None,
            input_refs: vec![memory_ref(record.id.clone())],
            created_at: now,
        }),
        reasoning: Some("Derived from explicit memory assertion.".to_owned()),
        embedding_refs: Vec::new(),
        policy: record.policy.clone(),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![memory_ref(record.id.clone())],
            derivations: Vec::new(),
            confidence: assertion.confidence.or(record.provenance.confidence),
            method: Some("assertion_belief_synthesis".to_owned()),
        },
        created_at: now,
        updated_at: None,
        metadata: None,
    }
}

fn belief_subject_key(assertion: &MemoryAssertion) -> String {
    assertion
        .subject
        .id
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| assertion.subject.name.clone())
        .unwrap_or_else(|| "unknown-subject".to_owned())
}

fn belief_content(assertion: &MemoryAssertion) -> String {
    format!(
        "{} {} {}",
        belief_subject_key(assertion),
        assertion.predicate,
        assertion.object
    )
}

fn belief_synthesized_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    record: &MemoryRecord,
    belief_id: &BeliefId,
    assertion_id: &str,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::BeliefSynthesized,
        scope: record.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(record.id.clone()),
        payload: json!({
            "reason": "assertion_belief_synthesis",
            "memoryId": record.id.to_string(),
            "assertionId": assertion_id,
            "beliefId": belief_id.to_string(),
        }),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![memory_ref(record.id.clone())],
            derivations: Vec::new(),
            confidence: record.provenance.confidence,
            method: Some("assertion_belief_synthesis".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    }
}
