//! Exact duplicate compaction for in-memory consolidation.
//!
//! This module owns the conservative in-memory duplicate algorithm only:
//! identify scoped active records with the same normalized text, preserve the
//! oldest record, archive later records, and emit audit events. It does not
//! plan consolidation runs, call models, summarize content, or delete records.

use std::collections::BTreeMap;

use engram_domain::*;
use engram_runtime::CoreResult;
use serde_json::json;

use crate::{
    consolidation::common::{add_counter, memory_ref},
    scope::scope_allows,
    service::InMemoryMemoryService,
};

/// Archives later exact-text duplicates inside the requested scope.
///
/// The function holds the in-memory state lock while it scans and mutates so
/// the process-local adapter sees one coherent transaction. It reports all
/// archived records through task counters and output references.
pub(super) fn compact_duplicates(
    service: &InMemoryMemoryService,
    request: &ConsolidationRequest,
    started_at: Timestamp,
    stats: &mut ConsolidationStats,
) -> CoreResult<ConsolidationTaskResult> {
    let now = service.clock.now();
    let mut state = service.lock_state()?;
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
            consolidated_event(service, request, record, preserved_id, now)
        };
        state.events.push(event);
    }

    let archived_count = archive_ids.len() as u64;
    add_counter(&mut stats.memories_read, read_count);
    stats.memories_written = Some(0);
    add_counter(&mut stats.records_pruned, archived_count);
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
        output_refs: archive_ids.into_iter().map(memory_ref).collect(),
    })
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
            evidence: vec![memory_ref(preserved_id.clone())],
            derivations: Vec::new(),
            confidence: Some(1.0),
            method: Some("compaction".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    }
}
