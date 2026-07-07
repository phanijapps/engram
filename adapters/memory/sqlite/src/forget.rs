//! SQL-backed forget lifecycle behavior.
//!
//! This module applies delete, redact, tombstone, and archive behavior through
//! SQL repository operations while preserving lifecycle audit events.

use engram_domain::*;
use engram_memory::{CoreError, CoreResult, MemoryRepository};
use serde_json::json;

use crate::{engine::SqlMemoryService, validation::validate_forget_request};

/// Applies forget lifecycle behavior to a SQL-backed memory target.
///
/// This operation validates the request, authorizes against the stored memory
/// policy, applies the requested lifecycle mode, and appends an auditable event.
/// Unsupported target types fail explicitly instead of silently doing nothing.
pub(crate) async fn forget(
    service: &SqlMemoryService,
    request: ForgetRequest,
) -> CoreResult<ForgetResult> {
    validate_forget_request(&request)?;
    if request.target_type != ForgetTargetType::Memory {
        return Err(CoreError::InvalidRequest {
            reason: "only memory forget targets are implemented".to_owned(),
        });
    }

    let memory_id = MemoryId::from(request.target_id.clone());
    let Some(existing) = service.store.get_memory(&memory_id, &request.scope).await? else {
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

    match request.mode {
        DeleteMode::Delete => {
            service.store.remove_memory(&existing.id, &existing.scope)?;
        }
        DeleteMode::Redact => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Redacted;
            record.content.text.clear();
            record.content.summary = None;
            record.content.entities.clear();
            record.content.structured = None;
            record.links.clear();
            record.assertions.clear();
            record.updated_at = Some(now);
            service.store.put_memory(record).await?;
        }
        DeleteMode::Tombstone => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Forgotten;
            record.content.text.clear();
            record.content.summary = None;
            record.content.structured = None;
            record.updated_at = Some(now);
            service.store.put_memory(record).await?;
        }
        DeleteMode::Archive => {
            let mut record = existing.clone();
            record.status = MemoryStatus::Archived;
            record.updated_at = Some(now);
            service.store.put_memory(record).await?;
        }
    }
    let event = service.store.append_event(event).await?;

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
