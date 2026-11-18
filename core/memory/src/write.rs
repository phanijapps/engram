//! Governed write-memory orchestration.
//!
//! The write path mirrors the in-memory baseline while persisting records,
//! events, and idempotency responses through portable storage contributions.

use crate::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{service::GovernedMemoryService, validation::validate_write_request};

/// Writes a memory through governed repository ports.
///
/// The operation validates, authorizes, creates the canonical record and event,
/// and stores the idempotency response so retried writes reuse the same event.
pub(crate) async fn write_memory(
    service: &GovernedMemoryService,
    request: WriteMemoryRequest,
) -> CoreResult<WriteMemoryResponse> {
    validate_write_request(&request)?;
    service
        .authorizer
        .can_write(&request.requester, &request.scope, &request.policy)?;

    let idempotency_key = request.idempotency_key.clone();
    let idempotency_lookup_key = request.idempotency_key.clone();
    if let Some(key) = &idempotency_lookup_key
        && let Some(existing) = service
            .store
            .idempotent_response(&request.scope, key)
            .await?
    {
        if existing.record.scope != request.scope
            || existing.event.scope != request.scope
            || existing.event.memory_id.as_ref() != Some(&existing.record.id)
            || existing.event.kind != MemoryEventKind::Written
        {
            return Err(crate::CoreError::Conflict {
                reason: "idempotent response identity mismatch".into(),
            });
        }
        let mut response = existing;
        response.deduplicated = Some(true);
        return Ok(response);
    }

    let now = service.clock.now();
    let memory_id = service.ids.new_id("memory");

    // Enrich content.entities with extracted cue anchors before persisting.
    let content = {
        let extracted = crate::extract(&request.content.text);
        let caller_entities = request.content.entities;
        let entities = crate::merge_entities(extracted, caller_entities);
        MemoryContent {
            entities,
            ..request.content
        }
    };

    let record = MemoryRecord {
        id: memory_id.clone(),
        kind: request.kind,
        content,
        scope: request.scope.clone(),
        provenance: request.provenance.clone(),
        policy: request.policy,
        status: MemoryStatus::Active,
        links: request.links,
        assertions: Vec::new(),
        verdict: request.verdict,
        created_at: now,
        updated_at: None,
        metadata: None,
    };
    let event = MemoryEvent {
        id: service.ids.new_id("event"),
        kind: MemoryEventKind::Written,
        scope: request.scope,
        actor: request.requester.actor,
        memory_id: Some(memory_id),
        payload: idempotency_key
            .as_ref()
            .map_or_else(|| json!({}), |key| json!({ "idempotencyKey": key })),
        provenance: request.provenance,
        occurred_at: now,
        recorded_at: now,
    };
    service
        .store
        .commit_write(idempotency_lookup_key.as_deref(), record, event)
        .await
        .result
}
