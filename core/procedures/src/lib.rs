//! Procedure behavior port for the engram engine (RFC-0016 Layer 6).
//!
//! Owns the storage-neutral [`ProcedureRepository`] contract that adapters
//! implement. Domain records (`Procedure`, `ProcedureStats`) live in
//! `engram-domain`; raw storage, SQL, and model calls stay outside this crate.

use async_trait::async_trait;
use engram_domain::{Procedure, ProcedureId, ProcedureStats, Scope};
use engram_runtime::CoreResult;

/// Persistence port for replayable procedures with success/failure accounting.
///
/// The server records outcomes (the caller drives [`Self::increment_success`] /
/// [`Self::increment_failure`]); it does not decide when a procedure applies.
#[async_trait]
pub trait ProcedureRepository: Send + Sync {
    /// Stores or updates a procedure (upsert by id).
    async fn upsert_procedure(&self, procedure: Procedure) -> CoreResult<Procedure>;

    /// Looks up one procedure by id inside the supplied scope.
    async fn get_procedure(&self, id: &ProcedureId, scope: &Scope)
    -> CoreResult<Option<Procedure>>;

    /// Looks up one procedure by name inside the supplied scope.
    async fn get_procedure_by_name(
        &self,
        name: &str,
        scope: &Scope,
    ) -> CoreResult<Option<Procedure>>;

    /// Lists procedures visible to the supplied scope.
    async fn list_procedures(&self, scope: &Scope) -> CoreResult<Vec<Procedure>>;

    /// Bumps the success counter for a procedure; returns the updated record.
    async fn increment_success(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure>;

    /// Bumps the failure counter for a procedure; returns the updated record.
    async fn increment_failure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure>;

    /// Aggregate statistics over the procedures in a scope.
    async fn procedure_stats(&self, scope: &Scope) -> CoreResult<ProcedureStats>;

    /// Deletes a procedure by id inside the supplied scope.
    async fn delete_procedure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<bool>;
}
