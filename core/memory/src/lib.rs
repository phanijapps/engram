//! Memory behavior ports.
//!
//! This crate owns the canonical memory-facing service and repository
//! contracts. It deliberately does not know how knowledge graphs, ontologies,
//! vector indexes, or source ingestion are stored.

use async_trait::async_trait;
use engram_domain::*;
pub use engram_runtime::{
    Clock, CoreError, CoreResult, IdGenerator, PolicyAuthorizer, ScopeMatcher,
};

/// Persistence port for memory records and append-only lifecycle events.
///
/// Implementations must preserve the portable `MemoryRecord` shape losslessly
/// and keep status changes auditable through `MemoryEvent`. SQL, event-sourced,
/// document, or in-memory adapters may choose different internal layouts, but
/// they must not persist knowledge graph or ontology state through this port.
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Stores a memory record and returns the persisted representation.
    async fn put_memory(&self, record: MemoryRecord) -> CoreResult<MemoryRecord>;

    /// Looks up a memory by ID inside the caller-provided scope boundary.
    async fn get_memory(&self, id: &MemoryId, scope: &Scope) -> CoreResult<Option<MemoryRecord>>;

    /// Appends a lifecycle event without rewriting prior events.
    async fn append_event(&self, event: MemoryEvent) -> CoreResult<MemoryEvent>;

    /// Updates lifecycle status while preserving policy and provenance history.
    async fn update_memory_status(
        &self,
        id: &MemoryId,
        scope: &Scope,
        status: MemoryStatus,
    ) -> CoreResult<MemoryRecord>;
}

/// Read port for append-only memory lifecycle events.
///
/// Event reads are separate from record writes because audit, evaluation,
/// consolidation, and debugging need history without direct mutation access.
/// Implementations must preserve event ordering as recorded by the adapter and
/// apply the supplied scope boundary before returning events.
#[async_trait]
pub trait MemoryEventRepository: Send + Sync {
    /// Looks up a lifecycle event by ID inside the caller-provided scope.
    async fn get_event(&self, id: &EventId, scope: &Scope) -> CoreResult<Option<MemoryEvent>>;

    /// Lists lifecycle events for one memory inside the caller-provided scope.
    async fn list_events_for_memory(
        &self,
        memory_id: &MemoryId,
        scope: &Scope,
    ) -> CoreResult<Vec<MemoryEvent>>;

    /// Lists lifecycle events visible to the supplied scope.
    async fn list_events_for_scope(&self, scope: &Scope) -> CoreResult<Vec<MemoryEvent>>;
}

/// Public service contract for memory workflows.
///
/// This is the application-facing boundary for agent memory behavior. Concrete
/// services enforce scope and policy for writes, retrieval, and forgetting while
/// delegating storage to memory repositories and retrieval composition to higher
/// layers. Knowledge graph storage belongs behind knowledge ports, not here.
#[async_trait]
pub trait MemoryService: Send + Sync {
    /// Writes a memory and records the corresponding lifecycle event.
    async fn write_memory(&self, request: WriteMemoryRequest) -> CoreResult<WriteMemoryResponse>;

    /// Retrieves policy-checked context for the request.
    async fn retrieve(&self, request: RetrievalRequest) -> CoreResult<ContextPayload>;

    /// Applies delete, redact, tombstone, or archive behavior to a target.
    async fn forget(&self, request: ForgetRequest) -> CoreResult<ForgetResult>;
}
