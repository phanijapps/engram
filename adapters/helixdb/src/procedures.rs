//! Durable scoped procedures and atomic accounting.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_procedures::ProcedureRepository;
use engram_runtime::{CoreError, CoreResult};
impl HelixStore {
    fn bump_procedure(
        &self,
        id: &ProcedureId,
        scope: &Scope,
        success: bool,
    ) -> CoreResult<Procedure> {
        self.check_scope(scope).map_err(core)?;
        self.domain_atomic(|s| {
            let mut v = s
                .get::<Procedure>(PROCEDURE, id.as_str())
                .map_err(core)?
                .ok_or_else(|| missing("procedure", id))?;
            let n = if success {
                &mut v.success_count
            } else {
                &mut v.failure_count
            };
            *n = n.checked_add(1).ok_or_else(|| CoreError::InvalidRequest {
                reason: "procedure counter exhausted".into(),
            })?;
            v.updated_at = Some(chrono::Utc::now());
            s.put(PROCEDURE, &v).map_err(core)
        })
    }
}
#[async_trait]
impl ProcedureRepository for HelixStore {
    async fn upsert_procedure(&self, v: Procedure) -> CoreResult<Procedure> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(PROCEDURE, &v).map_err(core)
    }
    async fn get_procedure(
        &self,
        id: &ProcedureId,
        scope: &Scope,
    ) -> CoreResult<Option<Procedure>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(PROCEDURE, id.as_str()).map_err(core)
    }
    async fn get_procedure_by_name(
        &self,
        name: &str,
        scope: &Scope,
    ) -> CoreResult<Option<Procedure>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        Ok(self
            .filtered::<Procedure>(PROCEDURE, "name", name)
            .map_err(core)?
            .into_iter()
            .next())
    }
    async fn list_procedures(&self, scope: &Scope) -> CoreResult<Vec<Procedure>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.all(PROCEDURE).map_err(core)
    }
    async fn increment_success(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure> {
        self.bump_procedure(id, scope, true)
    }
    async fn increment_failure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<Procedure> {
        self.bump_procedure(id, scope, false)
    }
    async fn procedure_stats(&self, scope: &Scope) -> CoreResult<ProcedureStats> {
        let rows = self.list_procedures(scope).await?;
        Ok(ProcedureStats {
            total: rows.len(),
            total_success: rows.iter().map(|v| v.success_count as u64).sum(),
            total_failure: rows.iter().map(|v| v.failure_count as u64).sum(),
        })
    }
    async fn delete_procedure(&self, id: &ProcedureId, scope: &Scope) -> CoreResult<bool> {
        self.check_scope(scope).map_err(core)?;
        self.remove(PROCEDURE, id.as_str()).map_err(core)
    }
}
