//! Concrete in-memory consolidation tasks.
//!
//! This module owns process-local consolidation algorithms for the in-memory
//! adapter. Core remains responsible for gating and audit orchestration; this
//! module only mutates scoped in-memory records and returns task outcomes.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use engram_core::{ConsolidationMutationExecutor, ConsolidationMutationOutcome, CoreResult};
use engram_domain::*;
use serde_json::json;

use crate::{scope::scope_allows, service::InMemoryMemoryService};

/// Mutating consolidation executor for the in-memory adapter.
///
/// The executor currently implements exact-text compaction only. It preserves
/// the oldest active memory in each duplicate group, archives later duplicates,
/// and appends `Consolidated` events for the archived records.
#[derive(Clone)]
pub struct InMemoryConsolidationExecutor {
    service: InMemoryMemoryService,
}

impl InMemoryConsolidationExecutor {
    /// Creates an executor that mutates the same state as the supplied service.
    ///
    /// Clone the `InMemoryMemoryService` passed to tests or examples so writes,
    /// retrieval, and consolidation operate over one process-local store.
    pub fn new(service: InMemoryMemoryService) -> Self {
        Self { service }
    }

    /// Creates a shared executor for `GatedConsolidationService` composition.
    ///
    /// The returned value is typed as `Arc<Self>` so callers can pass it through
    /// Rust's trait-object coercion to the core `ConsolidationMutationExecutor`
    /// port while still constructing it from the concrete in-memory service.
    pub fn shared(service: InMemoryMemoryService) -> Arc<Self> {
        Arc::new(Self::new(service))
    }
}

#[async_trait]
impl ConsolidationMutationExecutor for InMemoryConsolidationExecutor {
    async fn execute(
        &self,
        request: &ConsolidationRequest,
        planned_tasks: &[ConsolidationTaskKind],
        started_at: Timestamp,
    ) -> CoreResult<ConsolidationMutationOutcome> {
        let mut tasks = Vec::with_capacity(planned_tasks.len());
        let mut stats = empty_stats();

        for task in planned_tasks {
            match task {
                ConsolidationTaskKind::Compaction => {
                    let result = self.compact_duplicates(request, started_at, &mut stats)?;
                    tasks.push(result);
                }
                unsupported => tasks.push(skipped_task(unsupported.clone(), started_at)),
            }
        }

        Ok(ConsolidationMutationOutcome::new(tasks, stats, Vec::new()))
    }
}

impl InMemoryConsolidationExecutor {
    fn compact_duplicates(
        &self,
        request: &ConsolidationRequest,
        started_at: Timestamp,
        stats: &mut ConsolidationStats,
    ) -> CoreResult<ConsolidationTaskResult> {
        let now = self.service.clock.now();
        let mut state = self.service.lock_state()?;
        let mut groups = BTreeMap::<String, Vec<MemoryCandidate>>::new();
        let mut read_count = 0_u64;
        let mut skipped_count = 0_u64;

        for record in state.memories.values() {
            if !scope_allows(&record.scope, &request.scope) {
                continue;
            }
            read_count += 1;
            let Some(candidate) = compaction_candidate(record, started_at) else {
                skipped_count += 1;
                continue;
            };
            groups
                .entry(candidate.normalized_text.clone())
                .or_default()
                .push(candidate);
        }

        let mut archive_ids = Vec::new();
        let mut preserved_pairs = Vec::new();
        for candidates in groups.values_mut() {
            candidates.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            let Some(preserved) = candidates.first() else {
                continue;
            };
            if candidates.len() == 1 {
                skipped_count += 1;
                continue;
            }
            skipped_count += 1;
            for duplicate in candidates.iter().skip(1) {
                archive_ids.push(duplicate.id.clone());
                preserved_pairs.push((duplicate.id.clone(), preserved.id.clone()));
            }
        }

        for (archived_id, preserved_id) in &preserved_pairs {
            let event = {
                let Some(record) = state.memories.get_mut(archived_id.as_str()) else {
                    continue;
                };
                record.status = MemoryStatus::Archived;
                record.updated_at = Some(now);
                consolidated_event(&self.service, request, record, preserved_id, now)
            };
            state.events.push(event);
        }

        let archived_count = archive_ids.len() as u64;
        stats.memories_read = Some(read_count);
        stats.memories_written = Some(0);
        stats.records_pruned = Some(archived_count);
        stats.model_calls = Some(0);

        Ok(ConsolidationTaskResult {
            task: ConsolidationTaskKind::Compaction,
            status: ConsolidationTaskStatus::Completed,
            started_at,
            completed_at: Some(now),
            items_read: Some(read_count),
            items_written: Some(0),
            items_updated: Some(archived_count),
            items_skipped: Some(skipped_count),
            model_calls: Some(0),
            errors: Vec::new(),
            output_refs: archive_ids
                .into_iter()
                .map(|id| EvidenceRef {
                    target_type: EvidenceTargetType::Memory,
                    target_id: Some(id.to_string()),
                    uri: None,
                    quote: None,
                    location: None,
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
struct MemoryCandidate {
    id: MemoryId,
    created_at: Timestamp,
    normalized_text: String,
}

fn compaction_candidate(record: &MemoryRecord, now: Timestamp) -> Option<MemoryCandidate> {
    if record.status != MemoryStatus::Active {
        return None;
    }
    if let Some(expires_at) = record.policy.expires_at
        && expires_at <= now
    {
        return None;
    }
    let normalized_text = normalize_text(&record.content.text)?;
    Some(MemoryCandidate {
        id: record.id.clone(),
        created_at: record.created_at,
        normalized_text,
    })
}

fn normalize_text(text: &str) -> Option<String> {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

fn consolidated_event(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    record: &MemoryRecord,
    preserved_id: &MemoryId,
    now: Timestamp,
) -> MemoryEvent {
    MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::Consolidated,
        scope: record.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(record.id.clone()),
        payload: json!({
            "reason": "exact_duplicate_compaction",
            "preservedMemoryId": preserved_id.to_string(),
            "archivedMemoryId": record.id.to_string(),
        }),
        provenance: Provenance {
            source: "consolidation".to_owned(),
            actor: request.requester.actor.clone(),
            observed_at: now,
            evidence: vec![EvidenceRef {
                target_type: EvidenceTargetType::Memory,
                target_id: Some(preserved_id.to_string()),
                uri: None,
                quote: None,
                location: None,
            }],
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("compaction".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    }
}

fn skipped_task(task: ConsolidationTaskKind, timestamp: Timestamp) -> ConsolidationTaskResult {
    ConsolidationTaskResult {
        task,
        status: ConsolidationTaskStatus::Skipped,
        started_at: timestamp,
        completed_at: Some(timestamp),
        items_read: Some(0),
        items_written: Some(0),
        items_updated: Some(0),
        items_skipped: Some(0),
        model_calls: Some(0),
        errors: Vec::new(),
        output_refs: Vec::new(),
    }
}

fn empty_stats() -> ConsolidationStats {
    ConsolidationStats {
        memories_read: Some(0),
        memories_written: Some(0),
        beliefs_synthesized: Some(0),
        contradictions_detected: Some(0),
        hierarchy_nodes_created: Some(0),
        hierarchy_relations_created: Some(0),
        records_decayed: Some(0),
        records_pruned: Some(0),
        model_calls: Some(0),
    }
}
