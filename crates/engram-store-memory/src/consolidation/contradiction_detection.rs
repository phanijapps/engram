//! Assertion contradiction detection for in-memory consolidation.
//!
//! This module owns deterministic review-record creation for explicit memory
//! assertions. It compares structured assertion objects only; semantic
//! contradiction detection, resolution, belief retraction, and model-assisted
//! review remain separate responsibilities.

use std::collections::{BTreeMap, BTreeSet};

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Detects conflicting scoped active assertions and writes contradiction records.
///
/// Conflicts are exact: same normalized subject key and predicate, different
/// JSON object values. Existing open contradictions with the same assertion pair
/// are treated as already reported and skipped.
pub(super) fn detect_assertion_contradictions(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
    let mut read_count = 0_u64;
    let mut skipped_count = 0_u64;
    let mut groups = BTreeMap::<ClaimKey, Vec<AssertionCandidate>>::new();

    for record in state.memories.values() {
        if !scope_allows(&record.scope, &request.scope) {
            continue;
        }
        read_count += 1;
        if !contradiction_candidate(record, started_at) {
            skipped_count += record.assertions.len() as u64;
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
                .push(AssertionCandidate::new(record, assertion, index));
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
    for candidates in groups.values() {
        for (left_index, left) in candidates.iter().enumerate() {
            for right in candidates.iter().skip(left_index + 1) {
                if left.object_key == right.object_key {
                    skipped_count += 1;
                    continue;
                }
                let pair_key = assertion_pair_key(&left.assertion_id, &right.assertion_id);
                if existing_pairs.contains(&pair_key) || !created_pairs.insert(pair_key) {
                    skipped_count += 1;
                    continue;
                }
                let contradiction_id = service.ids.new_id("contradiction");
                contradictions.push(contradiction_record(
                    contradiction_id.clone(),
                    request,
                    left,
                    right,
                    now,
                ));
                events.push(contradiction_event(
                    service,
                    request,
                    left,
                    &contradiction_id,
                    now,
                ));
                events.push(contradiction_event(
                    service,
                    request,
                    right,
                    &contradiction_id,
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
        task: ConsolidationTaskKind::BeliefContradictionDetection,
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
struct AssertionCandidate {
    memory_id: MemoryId,
    scope: Scope,
    assertion_id: String,
    object_key: String,
    confidence: Option<f32>,
}

impl AssertionCandidate {
    fn new(record: &MemoryRecord, assertion: &MemoryAssertion, index: usize) -> Self {
        Self {
            memory_id: record.id.clone(),
            scope: record.scope.clone(),
            assertion_id: assertion_target_id(&record.id, index),
            object_key: serde_json::to_string(&assertion.object)
                .unwrap_or_else(|_| assertion.object.to_string()),
            confidence: assertion.confidence,
        }
    }
}

fn contradiction_candidate(record: &MemoryRecord, now: Timestamp) -> bool {
    if record.status != MemoryStatus::Active {
        return false;
    }
    record
        .policy
        .expires_at
        .is_none_or(|expires_at| expires_at > now)
}

fn contradiction_record(
    contradiction_id: ContradictionId,
    request: &ConsolidationRequest,
    left: &AssertionCandidate,
    right: &AssertionCandidate,
    now: Timestamp,
) -> Contradiction {
    Contradiction {
        id: contradiction_id,
        scope: left.scope.clone(),
        kind: ContradictionKind::Logical,
        targets: vec![
            assertion_target(&left.assertion_id, "claim"),
            assertion_target(&right.assertion_id, "counterclaim"),
            memory_target(&left.memory_id, "source"),
            memory_target(&right.memory_id, "source"),
        ],
        severity: severity(left, right),
        status: ContradictionStatus::Open,
        reasoning: Some("Conflicting explicit memory assertions.".to_owned()),
        detected_by: Some(DerivationRef {
            kind: DerivationKind::Consolidation,
            model: None,
            prompt_hash: None,
            input_refs: vec![
                memory_ref(left.memory_id.clone()),
                memory_ref(right.memory_id.clone()),
            ],
            created_at: now,
        }),
        resolution: None,
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![
                memory_ref(left.memory_id.clone()),
                memory_ref(right.memory_id.clone()),
            ],
            derivations: Vec::new(),
            confidence: Some(severity(left, right)),
            method: Some("assertion_contradiction_detection".to_owned()),
        },
        detected_at: now,
        updated_at: None,
    }
}

fn contradiction_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    candidate: &AssertionCandidate,
    contradiction_id: &ContradictionId,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::ContradictionDetected,
        scope: candidate.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(candidate.memory_id.clone()),
        payload: json!({
            "reason": "assertion_contradiction_detection",
            "assertionId": candidate.assertion_id,
            "contradictionId": contradiction_id.to_string(),
        }),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![memory_ref(candidate.memory_id.clone())],
            derivations: Vec::new(),
            confidence: candidate.confidence,
            method: Some("assertion_contradiction_detection".to_owned()),
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

fn severity(left: &AssertionCandidate, right: &AssertionCandidate) -> f32 {
    left.confidence
        .zip(right.confidence)
        .map(|(left, right)| left.min(right))
        .unwrap_or(0.5)
}
