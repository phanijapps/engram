//! Forget lifecycle behavior for the in-memory adapter.
//!
//! This module owns memory-target lifecycle mutation for `delete`, `redact`,
//! `tombstone`, and `archive`. It keeps audit events visible while ensuring
//! normal retrieval cannot leak forgotten or redacted memory content.

use engram_core::{CoreError, CoreResult};
use engram_domain::*;
use serde_json::json;

use crate::{
    scope::scope_allows, service::InMemoryMemoryService, validation::validate_forget_request,
};

/// Applies forget lifecycle behavior to an in-memory memory target.
///
/// The operation validates request shape, finds the target within scope,
/// authorizes the mutation, applies the requested delete mode, and appends an
/// audit event for successful lifecycle changes.
pub(crate) async fn forget(
    service: &InMemoryMemoryService,
    request: ForgetRequest,
) -> CoreResult<ForgetResult> {
    validate_forget_request(&request)?;
    if request.target_type != ForgetTargetType::Memory {
        return Err(CoreError::InvalidRequest {
            reason: "only memory forget targets are implemented".to_owned(),
        });
    }

    let memory_id = MemoryId::from(request.target_id.clone());
    let existing = {
        let state = service.lock_state()?;
        state
            .memories
            .get(memory_id.as_str())
            .filter(|record| scope_allows(&record.scope, &request.scope))
            .cloned()
    };
    let Some(existing) = existing else {
        return Ok(ForgetResult {
            target_type: "memory".to_owned(),
            target_id: request.target_id,
            status: ForgetStatus::NotFound,
            event: None,
        });
    };

    service
        .authorizer
        .can_forget(&request.requester, &existing.scope, &existing.policy)?;

    let now = service.clock.now();
    let event = MemoryEvent {
        id: service.ids.new_id("event"),
        kind: event_kind_for(&request.mode),
        scope: existing.scope.clone(),
        actor: request.requester.actor.clone(),
        memory_id: Some(existing.id.clone()),
        payload: json!({
            "mode": delete_mode_name(&request.mode),
            "reason": request.reason,
        }),
        provenance: Provenance {
            source: "forget_request".to_owned(),
            actor: request.requester.actor,
            observed_at: now,
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: None,
            method: Some("manual".to_owned()),
        },
        occurred_at: now,
        recorded_at: now,
    };

    let mut state = service.lock_state()?;
    match request.mode {
        DeleteMode::Delete => {
            state.memories.remove(existing.id.as_str());
        }
        DeleteMode::Redact => {
            let record = state
                .memories
                .get_mut(existing.id.as_str())
                .ok_or_else(|| CoreError::NotFound {
                    target_type: "memory",
                    target_id: existing.id.to_string(),
                })?;
            record.status = MemoryStatus::Redacted;
            record.content.text.clear();
            record.content.summary = None;
            record.content.entities.clear();
            record.content.structured = None;
            record.links.clear();
            record.assertions.clear();
            record.updated_at = Some(now);
        }
        DeleteMode::Tombstone => {
            let record = state
                .memories
                .get_mut(existing.id.as_str())
                .ok_or_else(|| CoreError::NotFound {
                    target_type: "memory",
                    target_id: existing.id.to_string(),
                })?;
            record.status = MemoryStatus::Forgotten;
            record.content.text.clear();
            record.content.summary = None;
            record.content.structured = None;
            record.updated_at = Some(now);
        }
        DeleteMode::Archive => {
            let record = state
                .memories
                .get_mut(existing.id.as_str())
                .ok_or_else(|| CoreError::NotFound {
                    target_type: "memory",
                    target_id: existing.id.to_string(),
                })?;
            record.status = MemoryStatus::Archived;
            record.updated_at = Some(now);
        }
    }
    state.events.push(event.clone());

    Ok(ForgetResult {
        target_type: "memory".to_owned(),
        target_id: existing.id.to_string(),
        status: forget_status_for(&event.payload),
        event: Some(event),
    })
}

fn event_kind_for(mode: &DeleteMode) -> MemoryEventKind {
    match mode {
        DeleteMode::Delete | DeleteMode::Tombstone | DeleteMode::Archive => {
            MemoryEventKind::Forgotten
        }
        DeleteMode::Redact => MemoryEventKind::Redacted,
    }
}

fn delete_mode_name(mode: &DeleteMode) -> &'static str {
    match mode {
        DeleteMode::Delete => "delete",
        DeleteMode::Redact => "redact",
        DeleteMode::Tombstone => "tombstone",
        DeleteMode::Archive => "archive",
    }
}

fn forget_status_for(payload: &Scalar) -> ForgetStatus {
    match payload.get("mode").and_then(Scalar::as_str) {
        Some("delete") => ForgetStatus::Deleted,
        Some("redact") => ForgetStatus::Redacted,
        Some("archive") => ForgetStatus::Archived,
        _ => ForgetStatus::Tombstoned,
    }
}
