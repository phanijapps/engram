//! Temporal assertion drift detection for in-memory consolidation.
//!
//! This module owns deterministic drift review records for explicit assertions.
//! It compares structured assertion objects over time only; embeddings, fuzzy
//! semantic matching, belief mutation, and resolution policy stay separate.

use std::collections::{BTreeMap, BTreeSet};

use engram_domain::*;
use engram_runtime::CoreResult;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Detects scoped temporal assertion drift and writes review records.
///
/// Drift is a later explicit assertion for the same normalized subject and
/// predicate with a different JSON object value. Existing open contradiction
/// records with the same assertion pair are treated as already reported.
pub(super) fn detect_assertion_drift(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut read_count = 0_u64;
    let mut skipped_count = 0_u64;
    let mut groups = BTreeMap::<ClaimKey, Vec<DriftCandidate>>::new();

    for record in state.memories.values() {
        if !scope_allows(&record.scope, &request.scope) {
            continue;
        }
        read_count += 1;
        if !drift_candidate(record, started_at) {
            skipped_count += record.assertions.len().max(1) as u64;
            continue;
        }
        if record.assertions.is_empty() {
            skipped_count += 1;
            continue;
        }
        for (index, assertion) in record.assertions.iter().enumerate() {
            groups
                .entry(ClaimKey::new(assertion))
                .or_default()
                .push(DriftCandidate::new(record, assertion, index));
        }
    }

    let existing_pairs = state
        .contradictions
        .values()
        .filter(|contradiction| {
            scope_allows(&contradiction.scope, &request.scope)
                && contradiction.status == ContradictionStatus::Open
        })
        .filter_map(contradiction_pair_key)
        .collect::<BTreeSet<_>>();

    let mut created_pairs = BTreeSet::new();
    let mut contradictions = Vec::new();
    let mut events = Vec::new();
    for candidates in groups.values_mut() {
        candidates.sort_by(|left, right| {
            left.effective_at
                .cmp(&right.effective_at)
                .then_with(|| left.assertion_id.cmp(&right.assertion_id))
        });
        for (previous_index, previous) in candidates.iter().enumerate() {
            for current in candidates.iter().skip(previous_index + 1) {
                if previous.object_key == current.object_key {
                    skipped_count += 1;
                    continue;
                }
                if previous.effective_at == current.effective_at {
                    skipped_count += 1;
                    continue;
                }
                let pair_key = assertion_pair_key(&previous.assertion_id, &current.assertion_id);
                if existing_pairs.contains(&pair_key) || !created_pairs.insert(pair_key) {
                    skipped_count += 1;
                    continue;
                }
                let contradiction_id = service.ids.new_id("contradiction");
                contradictions.push(drift_record(
                    contradiction_id.clone(),
                    request,
                    previous,
                    current,
                    now,
                ));
                events.push(drift_event(
                    service,
                    request,
                    previous,
                    &contradiction_id,
                    "previous",
                    now,
                ));
                events.push(drift_event(
                    service,
                    request,
                    current,
                    &contradiction_id,
                    "current",
                    now,
                ));
            }
        }
    }

    let created_count = contradictions.len() as u64;
    let output_refs = contradictions
        .iter()
        .flat_map(|contradiction| {
            contradiction
                .targets
                .iter()
                .filter(|target| target.target_type == ContradictionTargetType::Memory)
                .map(|target| memory_ref(MemoryId::from(target.target_id.clone())))
        })
        .collect::<Vec<_>>();

    for contradiction in contradictions {
        state
            .contradictions
            .insert(contradiction.id.to_string(), contradiction);
    }
    state.events.extend(events);

    add_counter(&mut stats.memories_read, read_count);
    stats.memories_written = Some(0);
    add_counter(&mut stats.contradictions_detected, created_count);
    stats.model_calls = Some(0);

    Ok(ConsolidationTaskResult {
        task: ConsolidationTaskKind::SemanticDriftDetection,
        status: ConsolidationTaskStatus::Completed,
        started_at,
        completed_at: Some(now),
        items_read: Some(read_count),
        items_written: Some(created_count),
        items_updated: Some(0),
        items_skipped: Some(skipped_count),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ClaimKey {
    subject_key: String,
    predicate: String,
}

impl ClaimKey {
    fn new(assertion: &MemoryAssertion) -> Self {
        Self {
            subject_key: subject_key(assertion),
            predicate: assertion.predicate.trim().to_lowercase(),
        }
    }
}

#[derive(Debug, Clone)]
struct DriftCandidate {
    memory_id: MemoryId,
    scope: Scope,
    assertion_id: String,
    object_key: String,
    confidence: Option<f32>,
    effective_at: Timestamp,
}

impl DriftCandidate {
    fn new(record: &MemoryRecord, assertion: &MemoryAssertion, index: usize) -> Self {
        Self {
            memory_id: record.id.clone(),
            scope: record.scope.clone(),
            assertion_id: assertion_target_id(&record.id, index),
            object_key: serde_json::to_string(&assertion.object)
                .unwrap_or_else(|_| assertion.object.to_string()),
            confidence: assertion.confidence,
            effective_at: assertion.valid_from.unwrap_or(record.created_at),
        }
    }
}

fn drift_candidate(record: &MemoryRecord, now: Timestamp) -> bool {
    if record.status != MemoryStatus::Active {
        return false;
    }
    record
        .policy
        .expires_at
        .is_none_or(|expires_at| expires_at > now)
}

fn drift_record(
    contradiction_id: ContradictionId,
    request: &ConsolidationRequest,
    previous: &DriftCandidate,
    current: &DriftCandidate,
    now: Timestamp,
) -> Contradiction {
    Contradiction {
        id: contradiction_id,
        scope: current.scope.clone(),
        kind: ContradictionKind::Temporal,
        targets: vec![
            assertion_target(&previous.assertion_id, "previous_claim"),
            assertion_target(&current.assertion_id, "current_claim"),
            memory_target(&previous.memory_id, "previous_source"),
            memory_target(&current.memory_id, "current_source"),
        ],
        severity: severity(previous, current),
        status: ContradictionStatus::Open,
        reasoning: Some(
            "Assertion object changed over time for the same subject and predicate.".to_owned(),
        ),
        detected_by: Some(DerivationRef {
            kind: DerivationKind::Consolidation,
            model: None,
            prompt_hash: None,
            input_refs: vec![
                memory_ref(previous.memory_id.clone()),
                memory_ref(current.memory_id.clone()),
            ],
            created_at: now,
        }),
        resolution: None,
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![
                memory_ref(previous.memory_id.clone()),
                memory_ref(current.memory_id.clone()),
            ],
            derivations: Vec::new(),
            confidence: Some(severity(previous, current)),
            method: Some("semantic_drift_detection".to_owned()),
        },
        detected_at: now,
        updated_at: None,
    }
}

fn drift_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    candidate: &DriftCandidate,
    contradiction_id: &ContradictionId,
    role: &str,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::ContradictionDetected,
        scope: candidate.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(candidate.memory_id.clone()),
        payload: json!({
            "reason": "semantic_drift_detection",
            "assertionId": candidate.assertion_id,
            "contradictionId": contradiction_id.to_string(),
            "role": role,
            "effectiveAt": candidate.effective_at.to_rfc3339(),
        }),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![memory_ref(candidate.memory_id.clone())],
            derivations: Vec::new(),
            confidence: candidate.confidence,
            method: Some("semantic_drift_detection".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    }
}

fn contradiction_pair_key(contradiction: &Contradiction) -> Option<String> {
    let assertions = contradiction
        .targets
        .iter()
        .filter(|target| target.target_type == ContradictionTargetType::Assertion)
        .map(|target| target.target_id.as_str())
        .collect::<Vec<_>>();
    (assertions.len() == 2).then(|| assertion_pair_key(assertions[0], assertions[1]))
}

fn assertion_pair_key(left: &str, right: &str) -> String {
    if left <= right {
        format!("{left}\u{1f}{right}")
    } else {
        format!("{right}\u{1f}{left}")
    }
}

fn assertion_target(assertion_id: &str, role: &str) -> ContradictionTarget {
    ContradictionTarget {
        target_type: ContradictionTargetType::Assertion,
        target_id: assertion_id.to_owned(),
        role: Some(role.to_owned()),
    }
}

fn memory_target(memory_id: &MemoryId, role: &str) -> ContradictionTarget {
    ContradictionTarget {
        target_type: ContradictionTargetType::Memory,
        target_id: memory_id.to_string(),
        role: Some(role.to_owned()),
    }
}

fn assertion_target_id(memory_id: &MemoryId, index: usize) -> String {
    format!("{memory_id}#assertion-{index}")
}

fn subject_key(assertion: &MemoryAssertion) -> String {
    assertion
        .subject
        .id
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| assertion.subject.name.clone())
        .unwrap_or_else(|| "unknown-subject".to_owned())
        .trim()
        .to_lowercase()
}

fn severity(previous: &DriftCandidate, current: &DriftCandidate) -> f32 {
    previous
        .confidence
        .zip(current.confidence)
        .map(|(previous, current)| previous.min(current))
        .unwrap_or(0.5)
}
