//! Memory capability fixture.
//!
//! Exercises the full memory lifecycle — write, retrieve, forget — against an
//! in-memory SQLite memory service so the `memory` capability is only reported
//! Supported when the adapter actually persists and isolates by scope.

use engram_domain::{
    Actor, ActorKind, DeleteMode, ForgetRequest, ForgetTargetType, MemoryContent, MemoryKind,
    Requester, RetrievalRequest, WriteMemoryRequest,
};
use engram_memory::MemoryService;
use engram_runtime::{CoreError, CoreResult};
use engram_store_sqlite::SqlMemoryService;
use futures::executor::block_on;

use super::support::{policy, provenance, scope};

/// Runs the memory capability fixture.
///
/// # Errors
///
/// Returns `CoreError::Adapter` if write/retrieve/forget or scope isolation fail.
pub fn run_memory_fixture() -> CoreResult<()> {
    let service = SqlMemoryService::open_in_memory()?;

    let req = write_request("tenant-a");
    let stored = block_on(service.write_memory(req)).map_err(fixture_err("write_memory"))?;
    let memory_id = stored.record.id.to_string();

    let visible = block_on(service.retrieve(retrieve_request("tenant-a")))
        .map_err(fixture_err("retrieve"))?;
    if !visible.items.iter().any(|r| r.target_id == memory_id) {
        return Err(fixture_err("retrieve")(CoreError::NotFound {
            target_type: "memory",
            target_id: memory_id.clone(),
        }));
    }

    // Scope isolation: tenant-b must not see tenant-a's memory.
    let hidden = block_on(service.retrieve(retrieve_request("tenant-b")))
        .map_err(fixture_err("retrieve"))?;
    if hidden.items.iter().any(|r| r.target_id == memory_id) {
        return Err(fixture_err("scope_isolation")(CoreError::Conflict {
            reason: "memory leaked across tenants".to_string(),
        }));
    }

    block_on(service.forget(forget_request(&memory_id, "tenant-a")))
        .map_err(fixture_err("forget"))?;
    Ok(())
}

fn write_request(tenant: &str) -> WriteMemoryRequest {
    WriteMemoryRequest {
        kind: MemoryKind::Observation,
        content: MemoryContent {
            text: "conformance memory".to_string(),
            summary: None,
            entities: Vec::new(),
            language: None,
            format: None,
            structured: None,
            hash: None,
        },
        scope: scope(tenant),
        requester: requester(),
        provenance: provenance(),
        policy: policy(),
        links: Vec::new(),
        idempotency_key: None,
    }
}

fn retrieve_request(tenant: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: "conformance".to_string(),
        scope: scope(tenant),
        requester: requester(),
        modes: Vec::new(),
        filters: None,
        cues: Vec::new(),
        limit: Some(10),
        budget: None,
        include_explanations: None,
    }
}

fn forget_request(id: &str, tenant: &str) -> ForgetRequest {
    ForgetRequest {
        target_type: ForgetTargetType::Memory,
        target_id: id.to_string(),
        scope: scope(tenant),
        requester: requester(),
        mode: DeleteMode::Tombstone,
        reason: None,
    }
}

fn requester() -> Requester {
    Requester {
        actor: Actor {
            id: engram_domain::Id::from("conformance-agent"),
            kind: ActorKind::Agent,
            display_name: Some("Conformance".to_string()),
            metadata: None,
        },
        roles: Vec::new(),
        permissions: vec!["memory.write".to_string(), "memory.retrieve".to_string()],
        on_behalf_of: None,
    }
}

fn fixture_err<'a>(op: &'a str) -> impl Fn(CoreError) -> CoreError + 'a {
    move |e| CoreError::Adapter {
        adapter: "conformance.memory".to_string(),
        message: format!("{op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fixture_passes() {
        assert!(run_memory_fixture().is_ok());
    }
}
