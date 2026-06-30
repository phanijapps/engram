//! Write-memory behavior for the in-memory adapter.
//!
//! This module owns the v1 write transaction shape for process-local storage:
//! validate, authorize, create a memory, append a written event, and preserve
//! idempotency without leaking storage details into the core ports.

use engram_core::CoreResult;
use engram_domain::*;
use serde_json::json;

use crate::{service::InMemoryMemoryService, validation::validate_write_request};

/// Writes a memory and appends its lifecycle event in one in-memory mutation.
///
/// The operation preserves the v1 write contract, applies policy before state
/// mutation, and returns existing records for scoped idempotent retries without
/// appending duplicate events.
pub(crate) async fn write_memory(
    service: &InMemoryMemoryService,
    request: WriteMemoryRequest,
) -> CoreResult<WriteMemoryResponse> {
    validate_write_request(&request)?;
    service
        .authorizer
        .can_write(&request.requester, &request.scope, &request.policy)?;

    let idempotency_key = request.idempotency_key.clone();
    let idempotency_lookup_key = idempotency_key_for(&request);
    if let Some(key) = &idempotency_lookup_key {
        let state = service.lock_state()?;
        if let Some(existing) = state.idempotency.get(key) {
            let mut response = existing.clone();
            response.deduplicated = Some(true);
            return Ok(response);
        }
    }

    let now = service.clock.now();
    let memory_id = service.ids.new_id("memory");
    let event_id = service.ids.new_id("event");
    let record = MemoryRecord {
        id: memory_id.clone(),
        kind: request.kind,
        content: request.content,
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
        id: event_id,
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
    let response = WriteMemoryResponse {
        record,
        event,
        deduplicated: Some(false),
    };

    let mut state = service.lock_state()?;
    state
        .memories
        .insert(response.record.id.to_string(), response.record.clone());
    state.events.push(response.event.clone());
    if let Some(key) = idempotency_lookup_key {
        state.idempotency.insert(key, response.clone());
    }
    Ok(response)
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
