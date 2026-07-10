//! Backend-neutral export port (engram-host-sdk brief, S5).
//!
//! [`ExportImport`] reads the semantic state of a [`Scope`] — knowledge sources,
//! documents, chunks, entities, relationships, and memory records — into one
//! [`ImportData`] payload. It is the **export half** of scope-to-scope movement:
//! import stays on the existing [`MigrationService`] handle
//! ([`MigrationService`] is sync with `dry_run_import` + `apply_import`), so
//! this port is export-only and the two traits are independent (no sync/async
//! impedance).
//!
//! [`MigrationService`]: crate::migration::MigrationService
//!
//! Export reads from the existing **concrete store methods** in the adapter
//! layer — the impl is engine-specific and lives in `engram-conformance`. The
//! exported [`ImportData`] reuses the existing `*ImportRecord` types, so
//! `export` → `dry_run_import` → `apply_import` round-trips for single-backend
//! scope-to-scope movement.
//!
//! v1 covers the families whose concrete stores expose scope-wide listing
//! methods (knowledge + memory). Hierarchy and belief export is deferred where
//! no concrete list method exists, and contradictions are not in the round-trip
//! ([`ImportData`] carries no contradictions vec). See `docs/backlog.md`,
//! `export-import-hierarchy-belief`.
//!
//! ADR-0022: this port is engine-neutral — it names no engine type and holds no
//! SQL (enforced by `.codex/hooks/check-engine-neutrality.sh`).

use async_trait::async_trait;
use engram_domain::Scope;
use engram_runtime::CoreResult;

use crate::migration::ImportData;

/// Export-only port: reads a scope's semantic state into one [`ImportData`]
/// payload through the wired concrete stores.
///
/// The payload reuses the existing `*ImportRecord` types, so callers move a
/// scope backend-to-backend by exporting here and importing through the existing
/// [`MigrationService`] handle. Import is **not** on this trait — it stays on
/// `MigrationService` (sync), which keeps this port independent of the import
/// pipeline.
///
/// # v1 coverage
///
/// v1 exports knowledge (sources, documents, chunks, entities, relationships)
/// and memory — the families whose concrete stores expose scope-wide listing
/// methods. Concept schemes/concepts, hierarchy, and belief export are deferred
/// where no concrete list method exists.
///
/// [`MigrationService`]: crate::migration::MigrationService
#[async_trait]
pub trait ExportImport: Send + Sync {
    /// Exports the semantic state visible to `scope` into one [`ImportData`]
    /// payload.
    ///
    /// Records are filtered to those visible to `scope` (tenant match, narrowed
    /// by the optional scope dimensions), mirroring the existing concrete-store
    /// visibility rules. The returned [`ImportData`] is ready for
    /// `MigrationService::dry_run_import`.
    async fn export(&self, scope: &Scope) -> CoreResult<ImportData>;
}
