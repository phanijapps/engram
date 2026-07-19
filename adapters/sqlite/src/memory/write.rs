//! SQL-backed write-memory orchestration.
//!
//! The write path mirrors the in-memory baseline while persisting records,
//! events, and idempotency responses through SQLite repository helpers.

use engram_domain::*;
use engram_memory::{self, CoreResult};
use serde_json::json;

use crate::memory::{
    engine::SqlMemoryService, transactional_write::write_memory_transaction,
    validation::validate_write_request,
};

/// Writes a memory through SQL-backed repository ports.
///
/// The operation validates, authorizes, creates the canonical record and event,
/// and stores the idempotency response so retried writes reuse the same event.
pub(crate) async fn write_memory(
    service: &SqlMemoryService,
    request: WriteMemoryRequest,
) -> CoreResult<WriteMemoryResponse> {
    validate_write_request(&request)?;
    service
        .authorizer
        .can_write(&request.requester, &request.scope, &request.policy)?;

    let idempotency_key = request.idempotency_key.clone();
    let idempotency_lookup_key = idempotency_key_for(&request);
    if let Some(key) = &idempotency_lookup_key
        && let Some(existing) = service.store.get_idempotent_response(key)?
    {
        let mut response = existing;
        response.deduplicated = Some(true);
        return Ok(response);
    }

    let now = service.clock.now();
    let memory_id = service.ids.new_id("memory");

    // Enrich content.entities with extracted cue anchors before persisting.
    let content = {
        let extracted = engram_memory::extract(&request.content.text);
        let caller_entities = request.content.entities;
        let entities = engram_memory::merge_entities(extracted, caller_entities);
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
    write_memory_transaction(
        &service.store,
        idempotency_lookup_key.as_deref(),
        record,
        event,
    )
}

fn idempotency_key_for(request: &WriteMemoryRequest) -> Option<String> {
    request.idempotency_key.as_ref().map(|key| {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            request.scope.tenant,
            request.scope.subject.as_deref().unwrap_or_default(),
            request.scope.workspace.as_deref().unwrap_or_default(),
            key
        )
    })
}
